//! tmux version parsing and capability comparisons.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use crate::{Error, Result};

/// A released tmux version, including letter suffixes such as `3.2a`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReleaseVersion {
    major: u16,
    minor: u16,
    suffix: Option<char>,
}

impl ReleaseVersion {
    /// The oldest supported tmux version.
    pub const MINIMUM: Self = Self::new(3, 2, Some('a'));

    /// Creates a released version.
    #[must_use]
    pub const fn new(major: u16, minor: u16, suffix: Option<char>) -> Self {
        Self {
            major,
            minor,
            suffix,
        }
    }

    /// Returns the major number.
    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }

    /// Returns the minor number.
    #[must_use]
    pub const fn minor(self) -> u16 {
        self.minor
    }

    /// Returns the optional letter suffix.
    #[must_use]
    pub const fn suffix(self) -> Option<char> {
        self.suffix
    }
}

impl Ord for ReleaseVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.suffix).cmp(&(other.major, other.minor, other.suffix))
    }
}

impl PartialOrd for ReleaseVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for ReleaseVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)?;
        if let Some(suffix) = self.suffix {
            suffix.fmt(formatter)?;
        }
        Ok(())
    }
}

impl FromStr for ReleaseVersion {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let value = value.trim();
        let dot = value
            .find('.')
            .ok_or_else(|| Error::InvalidVersion(value.to_owned()))?;
        let (major, tail) = value.split_at(dot);
        let tail = &tail[1..];
        let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || digits + 1 < tail.len() {
            return Err(Error::InvalidVersion(value.to_owned()));
        }
        let suffix = tail[digits..].chars().next();
        if suffix.is_some_and(|suffix| !suffix.is_ascii_lowercase()) {
            return Err(Error::InvalidVersion(value.to_owned()));
        }
        Ok(Self {
            major: major
                .parse()
                .map_err(|_| Error::InvalidVersion(value.to_owned()))?,
            minor: tail[..digits]
                .parse()
                .map_err(|_| Error::InvalidVersion(value.to_owned()))?,
            suffix,
        })
    }
}

/// A tmux version reported by `tmux -V`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TmuxVersion {
    /// A tagged release.
    Release(ReleaseVersion),
    /// An unreleased development build, which is assumed to have current capabilities.
    Development(String),
}

impl TmuxVersion {
    /// Parses the complete output of `tmux -V`.
    pub fn parse(value: &str) -> Result<Self> {
        let raw = value.trim();
        let version = raw.strip_prefix("tmux ").unwrap_or(raw);
        if version.eq_ignore_ascii_case("master") || version.contains('-') {
            return Ok(Self::Development(version.to_owned()));
        }
        Ok(Self::Release(version.parse()?))
    }

    /// Returns whether this version supplies a released capability.
    #[must_use]
    pub fn meets(&self, required: ReleaseVersion) -> bool {
        match self {
            Self::Release(found) => *found >= required,
            Self::Development(_) => true,
        }
    }

    /// Returns an error if this version is older than a capability requirement.
    pub fn require(&self, capability: &'static str, required: ReleaseVersion) -> Result<()> {
        if self.meets(required) {
            return Ok(());
        }
        Err(Error::Unsupported {
            capability,
            required,
            found: self.clone(),
        })
    }
}

impl fmt::Display for TmuxVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Release(version) => version.fmt(formatter),
            Self::Development(version) => formatter.write_str(version),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ReleaseVersion, TmuxVersion};

    #[test]
    fn parses_release_suffixes() {
        assert_eq!(
            TmuxVersion::parse("tmux 3.2a").ok(),
            Some(TmuxVersion::Release(ReleaseVersion::new(3, 2, Some('a'))))
        );
        assert_eq!(
            TmuxVersion::parse("3.6").ok(),
            Some(TmuxVersion::Release(ReleaseVersion::new(3, 6, None)))
        );
    }

    #[test]
    fn orders_lettered_releases() {
        let plain = ReleaseVersion::new(3, 2, None);
        let patched = ReleaseVersion::new(3, 2, Some('a'));
        assert!(patched > plain);
    }
}
