//! Typed, in-memory query expressions and iterator extensions.

use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

use regex::RegexBuilder;

use crate::{Error, FormatDescriptor, Result, SnapshotFields, TmuxText};

type Predicate<T> = dyn Fn(&T) -> bool + Send + Sync + 'static;

/// A composable, typed predicate over a tmux object or snapshot.
pub struct FilterExpr<T> {
    description: String,
    predicate: Arc<Predicate<T>>,
}

impl<T> Clone for FilterExpr<T> {
    fn clone(&self) -> Self {
        Self {
            description: self.description.clone(),
            predicate: Arc::clone(&self.predicate),
        }
    }
}

impl<T> fmt::Debug for FilterExpr<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("FilterExpr")
            .field(&self.description)
            .finish()
    }
}

impl<T: 'static> FilterExpr<T> {
    /// Creates an expression from an explicit predicate and diagnostic description.
    #[must_use]
    pub fn new(
        description: impl Into<String>,
        predicate: impl Fn(&T) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            description: description.into(),
            predicate: Arc::new(predicate),
        }
    }

    /// Returns a diagnostic description that never evaluates tmux data.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Evaluates the predicate.
    #[must_use]
    pub fn matches(&self, value: &T) -> bool {
        (self.predicate)(value)
    }

    /// Requires both expressions to match.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        let description = format!("({}) AND ({})", self.description, other.description);
        Self::new(description, move |value| {
            self.matches(value) && other.matches(value)
        })
    }

    /// Requires either expression to match.
    #[must_use]
    pub fn or(self, other: Self) -> Self {
        let description = format!("({}) OR ({})", self.description, other.description);
        Self::new(description, move |value| {
            self.matches(value) || other.matches(value)
        })
    }

    /// Inverts an expression.
    #[must_use]
    pub fn negate(self) -> Self {
        let description = format!("NOT ({})", self.description);
        Self::new(description, move |value| !self.matches(value))
    }

    /// Compares an extracted value for equality.
    #[must_use]
    pub fn equal<V>(
        field: &'static str,
        accessor: impl Fn(&T) -> V + Send + Sync + 'static,
        expected: V,
    ) -> Self
    where
        V: PartialEq + Send + Sync + 'static,
    {
        Self::new(format!("{field} == <value>"), move |value| {
            accessor(value) == expected
        })
    }

    /// Compares an extracted value for inequality.
    #[must_use]
    pub fn not_equal<V>(
        field: &'static str,
        accessor: impl Fn(&T) -> V + Send + Sync + 'static,
        expected: V,
    ) -> Self
    where
        V: PartialEq + Send + Sync + 'static,
    {
        Self::new(format!("{field} != <value>"), move |value| {
            accessor(value) != expected
        })
    }

    /// Compares an ordered value using one of the four ordering relations.
    #[must_use]
    pub fn compare<V>(
        field: &'static str,
        accessor: impl Fn(&T) -> V + Send + Sync + 'static,
        expected: V,
        comparison: Comparison,
    ) -> Self
    where
        V: PartialOrd + Send + Sync + 'static,
    {
        Self::new(format!("{field} {comparison} <value>"), move |value| {
            accessor(value)
                .partial_cmp(&expected)
                .is_some_and(|ordering| comparison.accepts(ordering))
        })
    }

    /// Requires an extracted value to occur in a supplied collection.
    #[must_use]
    pub fn is_in<V>(
        field: &'static str,
        accessor: impl Fn(&T) -> V + Send + Sync + 'static,
        values: impl IntoIterator<Item = V>,
    ) -> Self
    where
        V: PartialEq + Send + Sync + 'static,
    {
        let values = values.into_iter().collect::<Vec<_>>();
        Self::new(format!("{field} IN <values>"), move |value| {
            values.contains(&accessor(value))
        })
    }

    /// Requires an extracted value not to occur in a supplied collection.
    #[must_use]
    pub fn not_in<V>(
        field: &'static str,
        accessor: impl Fn(&T) -> V + Send + Sync + 'static,
        values: impl IntoIterator<Item = V>,
    ) -> Self
    where
        V: PartialEq + Send + Sync + 'static,
    {
        Self::is_in(field, accessor, values).negate()
    }

    /// Applies a text comparison, optionally with Unicode case folding.
    #[must_use]
    pub fn text(
        field: &'static str,
        accessor: impl Fn(&T) -> String + Send + Sync + 'static,
        expected: impl Into<String>,
        comparison: TextComparison,
        case_insensitive: bool,
    ) -> Self {
        let mut expected = expected.into();
        if case_insensitive {
            expected = expected.to_lowercase();
        }
        Self::new(format!("{field} {comparison} <text>"), move |value| {
            let mut actual = accessor(value);
            if case_insensitive {
                actual = actual.to_lowercase();
            }
            comparison.accepts(&actual, &expected)
        })
    }

    /// Matches an extracted string with a compiled regular expression.
    pub fn regex(
        field: &'static str,
        accessor: impl Fn(&T) -> String + Send + Sync + 'static,
        expression: &str,
        case_insensitive: bool,
    ) -> Result<Self> {
        let regex = RegexBuilder::new(expression)
            .case_insensitive(case_insensitive)
            .build()
            .map_err(|source| Error::InvalidArgument {
                argument: "regular expression",
                message: source.to_string(),
            })?;
        Ok(Self::new(
            format!("{field} ~= /{expression}/"),
            move |value| regex.is_match(&accessor(value)),
        ))
    }
}

impl<T> FilterExpr<T>
where
    T: SnapshotFields + 'static,
{
    /// Compares a catalog-selected raw field without dynamic attribute names.
    #[must_use]
    pub fn format_equal(descriptor: &'static FormatDescriptor, expected: TmuxText) -> Self {
        Self::new(
            format!("{} == <format-value>", descriptor.token()),
            move |snapshot: &T| snapshot.raw_field(descriptor) == Some(&expected),
        )
    }

    /// Applies a text operation to a catalog-selected field.
    #[must_use]
    pub fn format_text(
        descriptor: &'static FormatDescriptor,
        expected: impl Into<String>,
        comparison: TextComparison,
        case_insensitive: bool,
    ) -> Self {
        Self::text(
            descriptor.token(),
            move |snapshot: &T| {
                snapshot
                    .raw_field(descriptor)
                    .map_or_else(String::new, |value| value.to_string_lossy().into_owned())
            },
            expected,
            comparison,
            case_insensitive,
        )
    }
}

impl<T: 'static> std::ops::Not for FilterExpr<T> {
    type Output = Self;

    fn not(self) -> Self::Output {
        self.negate()
    }
}

/// An ordered comparison operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Comparison {
    /// Less than.
    Less,
    /// Less than or equal.
    LessOrEqual,
    /// Greater than.
    Greater,
    /// Greater than or equal.
    GreaterOrEqual,
}

impl Comparison {
    fn accepts(self, ordering: Ordering) -> bool {
        match self {
            Self::Less => ordering == Ordering::Less,
            Self::LessOrEqual => ordering != Ordering::Greater,
            Self::Greater => ordering == Ordering::Greater,
            Self::GreaterOrEqual => ordering != Ordering::Less,
        }
    }
}

impl fmt::Display for Comparison {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
        })
    }
}

/// A string comparison operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TextComparison {
    /// Exact equality.
    Exact,
    /// Substring containment.
    Contains,
    /// Prefix match.
    StartsWith,
    /// Suffix match.
    EndsWith,
}

impl TextComparison {
    fn accepts(self, actual: &str, expected: &str) -> bool {
        match self {
            Self::Exact => actual == expected,
            Self::Contains => actual.contains(expected),
            Self::StartsWith => actual.starts_with(expected),
            Self::EndsWith => actual.ends_with(expected),
        }
    }
}

impl fmt::Display for TextComparison {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Exact => "EXACT",
            Self::Contains => "CONTAINS",
            Self::StartsWith => "STARTS WITH",
            Self::EndsWith => "ENDS WITH",
        })
    }
}

/// Iterator returned by [`QueryIteratorExt::filter_expr`].
#[derive(Debug)]
pub struct Filtered<I>
where
    I: Iterator,
{
    iterator: I,
    expression: FilterExpr<I::Item>,
}

impl<I> Iterator for Filtered<I>
where
    I: Iterator,
    I::Item: 'static,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.iterator.find(|value| self.expression.matches(value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.iterator.size_hint();
        (0, upper)
    }
}

/// Adds typed expression filtering to every iterator.
pub trait QueryIteratorExt: Iterator + Sized
where
    Self::Item: 'static,
{
    /// Filters this iterator using a typed expression.
    fn filter_expr(self, expression: FilterExpr<Self::Item>) -> Filtered<Self> {
        Filtered {
            iterator: self,
            expression,
        }
    }
}

impl<I> QueryIteratorExt for I
where
    I: Iterator,
    I::Item: 'static,
{
}

#[cfg(test)]
mod tests {
    use super::{Comparison, FilterExpr, QueryIteratorExt, TextComparison};

    #[derive(Debug)]
    struct Item {
        name: String,
        count: u32,
    }

    #[test]
    fn expressions_compose_and_filter_iterators() {
        let count = FilterExpr::compare(
            "count",
            |item: &Item| item.count,
            2,
            Comparison::GreaterOrEqual,
        );
        let name = FilterExpr::text(
            "name",
            |item: &Item| item.name.clone(),
            "ALP",
            TextComparison::StartsWith,
            true,
        );
        let values = vec![
            Item {
                name: "alpha".to_owned(),
                count: 1,
            },
            Item {
                name: "Alpine".to_owned(),
                count: 3,
            },
        ];
        let result = values
            .into_iter()
            .filter_expr(count.and(name))
            .collect::<Vec<_>>();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Alpine");
    }

    #[test]
    fn regex_filters() {
        let Ok(expression) =
            FilterExpr::regex("name", |item: &Item| item.name.clone(), "^a.+a$", true)
        else {
            return;
        };
        assert!(expression.matches(&Item {
            name: "Alpha".to_owned(),
            count: 0
        }));
    }
}
