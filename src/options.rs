//! Uniform option, hook, and sparse-array representations.

use std::collections::BTreeMap;
use std::ffi::OsString;

use crate::{Error, OptionErrorKind, Result, TmuxText};

/// The scope used by tmux option commands.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OptionScope {
    /// Server-global options (`set-option -s`).
    Server,
    /// Global defaults for session options (`set-option -g`).
    GlobalSession,
    /// Session options.
    Session,
    /// Global defaults for window options (`set-option -gw`).
    GlobalWindow,
    /// Window options (`set-window-option`).
    Window,
    /// Pane options (`set-option -p`).
    Pane,
}

impl OptionScope {
    pub(crate) fn show_flags(self) -> &'static [&'static str] {
        match self {
            Self::Server => &["-s"],
            Self::GlobalSession => &["-g"],
            Self::Session => &[],
            Self::GlobalWindow => &["-g", "-w"],
            Self::Window => &["-w"],
            Self::Pane => &["-p"],
        }
    }
}

/// A value accepted by tmux's option and environment commands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptionValue {
    /// A tmux on/off flag.
    Flag(bool),
    /// A signed numeric value.
    Number(i64),
    /// Byte-preserving textual data.
    Text(TmuxText),
}

impl OptionValue {
    pub(crate) fn to_os_string(&self) -> OsString {
        match self {
            Self::Flag(value) => OsString::from(if *value { "on" } else { "off" }),
            Self::Number(value) => OsString::from(value.to_string()),
            Self::Text(value) => value.to_os_string(),
        }
    }

    /// Returns the value as a boolean when tmux encoded a known flag value.
    #[must_use]
    pub fn as_flag(&self) -> Option<bool> {
        match self {
            Self::Flag(value) => Some(*value),
            Self::Text(value) if value.as_bytes() == b"on" || value.as_bytes() == b"1" => {
                Some(true)
            }
            Self::Text(value) if value.as_bytes() == b"off" || value.as_bytes() == b"0" => {
                Some(false)
            }
            Self::Number(_) | Self::Text(_) => None,
        }
    }

    /// Returns a signed number when one is available.
    pub fn as_number(&self) -> Result<Option<i64>> {
        match self {
            Self::Number(value) => Ok(Some(*value)),
            Self::Text(value) => {
                let Ok(value) = value.to_str() else {
                    return Ok(None);
                };
                Ok(value.parse().ok())
            }
            Self::Flag(_) => Ok(None),
        }
    }

    /// Returns the byte-preserving textual representation when stored as text.
    #[must_use]
    pub fn as_text(&self) -> Option<&TmuxText> {
        match self {
            Self::Text(value) => Some(value),
            Self::Flag(_) | Self::Number(_) => None,
        }
    }
}

impl From<bool> for OptionValue {
    fn from(value: bool) -> Self {
        Self::Flag(value)
    }
}

impl From<i64> for OptionValue {
    fn from(value: i64) -> Self {
        Self::Number(value)
    }
}

impl From<&str> for OptionValue {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}

impl From<String> for OptionValue {
    fn from(value: String) -> Self {
        Self::Text(value.into())
    }
}

impl From<TmuxText> for OptionValue {
    fn from(value: TmuxText) -> Self {
        Self::Text(value)
    }
}

/// A deterministic map of tmux option names to values.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OptionMap(BTreeMap<String, OptionValue>);

impl OptionMap {
    /// Creates an empty map.
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns an option by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&OptionValue> {
        self.0.get(name)
    }

    /// Inserts an option value.
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        value: impl Into<OptionValue>,
    ) -> Option<OptionValue> {
        self.0.insert(name.into(), value.into())
    }

    /// Iterates in lexical option-name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &OptionValue)> {
        self.0.iter().map(|(name, value)| (name.as_str(), value))
    }

    /// Returns the number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the map is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn parse(bytes: &[u8]) -> Result<Self> {
        let mut map = Self::new();
        for line in bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            let split = line
                .iter()
                .position(|byte| *byte == b' ')
                .unwrap_or(line.len());
            let name = std::str::from_utf8(&line[..split]).map_err(|source| Error::Option {
                kind: OptionErrorKind::Parse,
                message: format!("option name is not UTF-8: {source}"),
            })?;
            let value_start = usize::min(split.saturating_add(1), line.len());
            map.insert(name, TmuxText::new(line[value_start..].to_vec()));
        }
        Ok(map)
    }
}

/// Sparse indexed option values such as hooks and `command-alias[]`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SparseOptionMap(BTreeMap<String, BTreeMap<u32, OptionValue>>);

impl SparseOptionMap {
    /// Creates an empty sparse map.
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns all populated indexes for a base option name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&BTreeMap<u32, OptionValue>> {
        self.0.get(name)
    }

    /// Sets one sparse value.
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        index: u32,
        value: impl Into<OptionValue>,
    ) -> Option<OptionValue> {
        self.0
            .entry(name.into())
            .or_default()
            .insert(index, value.into())
    }

    /// Iterates over base names in lexical order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &BTreeMap<u32, OptionValue>)> {
        self.0.iter().map(|(name, values)| (name.as_str(), values))
    }

    pub(crate) fn parse(bytes: &[u8]) -> Result<Self> {
        let options = OptionMap::parse(bytes)?;
        let mut sparse = Self::new();
        for (name, value) in options.0 {
            let Some(open) = name.rfind('[') else {
                continue;
            };
            let Some(index) = name.strip_suffix(']').and_then(|name| name.get(open + 1..)) else {
                return Err(Error::Option {
                    kind: OptionErrorKind::SparseIndex,
                    message: format!("malformed indexed option `{name}`"),
                });
            };
            let base = &name[..open];
            let index = index.parse().map_err(|_| Error::Option {
                kind: OptionErrorKind::SparseIndex,
                message: format!("invalid index in `{name}`"),
            })?;
            sparse.insert(base, index, value);
        }
        Ok(sparse)
    }
}

#[cfg(test)]
mod tests {
    use super::{OptionMap, SparseOptionMap};

    #[test]
    fn parses_option_output_without_losing_spaces() {
        let Ok(options) = OptionMap::parse(b"status-left hello world\nbase-index 1\n") else {
            return;
        };
        assert_eq!(
            options
                .get("status-left")
                .and_then(super::OptionValue::as_text)
                .map(crate::TmuxText::as_bytes),
            Some(b"hello world".as_slice())
        );
    }

    #[test]
    fn parses_sparse_arrays() {
        let Ok(options) = SparseOptionMap::parse(b"hook[1] first\nhook[9] ninth\n") else {
            return;
        };
        assert_eq!(
            options.get("hook").map(std::collections::BTreeMap::len),
            Some(2)
        );
    }
}
