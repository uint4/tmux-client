//! Validated tmux identities and command targets.

use std::ffi::OsString;
use std::fmt;
use std::str::FromStr;

use crate::{Error, Result, TmuxText};

macro_rules! id_type {
    ($name:ident, $prefix:literal, $kind:literal) => {
        #[doc = concat!("A validated tmux ", $kind, " identity.")]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u64);

        impl $name {
            #[doc = concat!("Creates a ", $kind, " identity from its numeric component.")]
            #[must_use]
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            /// Returns the numeric component.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, concat!($prefix, "{}"), self.0)
            }
        }

        impl FromStr for $name {
            type Err = Error;

            fn from_str(value: &str) -> Result<Self> {
                let numeric = value
                    .strip_prefix($prefix)
                    .ok_or_else(|| Error::InvalidId {
                        kind: $kind,
                        value: value.to_owned(),
                    })?;
                let parsed = numeric.parse().map_err(|_| Error::InvalidId {
                    kind: $kind,
                    value: value.to_owned(),
                })?;
                Ok(Self(parsed))
            }
        }
    };
}

id_type!(SessionId, "$", "session");
id_type!(WindowId, "@", "window");
id_type!(PaneId, "%", "pane");

/// A validated tmux session name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SessionName(String);

impl SessionName {
    /// Validates a session name.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() || value.contains('.') || value.contains(':') {
            return Err(Error::InvalidSessionName(value));
        }
        Ok(Self(value))
    }

    /// Returns the session name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for SessionName {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

/// The server-visible name of an attached tmux client.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClientName(TmuxText);

impl ClientName {
    /// Creates a client name, rejecting empty values.
    pub fn new(value: impl Into<TmuxText>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(Error::InvalidArgument {
                argument: "client name",
                message: "must not be empty".to_owned(),
            });
        }
        Ok(Self(value))
    }

    /// Returns the byte-preserving name.
    #[must_use]
    pub const fn as_text(&self) -> &TmuxText {
        &self.0
    }

    pub(crate) fn to_os_string(&self) -> OsString {
        self.0.to_os_string()
    }
}

impl fmt::Display for ClientName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A target that identifies a session.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SessionTarget {
    /// An immutable tmux session ID.
    Id(SessionId),
    /// A session name.
    Name(SessionName),
}

impl fmt::Display for SessionTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(id) => id.fmt(formatter),
            Self::Name(name) => name.fmt(formatter),
        }
    }
}

impl From<SessionId> for SessionTarget {
    fn from(value: SessionId) -> Self {
        Self::Id(value)
    }
}

impl From<SessionName> for SessionTarget {
    fn from(value: SessionName) -> Self {
        Self::Name(value)
    }
}

/// A target that identifies a window or a window link.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum WindowTarget {
    /// An immutable tmux window ID.
    Id(WindowId),
    /// A window index in a session.
    Link(WindowLink),
}

impl fmt::Display for WindowTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(id) => id.fmt(formatter),
            Self::Link(link) => link.fmt(formatter),
        }
    }
}

impl From<WindowId> for WindowTarget {
    fn from(value: WindowId) -> Self {
        Self::Id(value)
    }
}

impl From<WindowLink> for WindowTarget {
    fn from(value: WindowLink) -> Self {
        Self::Link(value)
    }
}

/// A target that identifies a pane.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PaneTarget {
    /// An immutable tmux pane ID.
    Id(PaneId),
}

impl fmt::Display for PaneTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(id) => id.fmt(formatter),
        }
    }
}

impl From<PaneId> for PaneTarget {
    fn from(value: PaneId) -> Self {
        Self::Id(value)
    }
}

/// A session-specific link to a window.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct WindowLink {
    session: SessionTarget,
    index: u32,
}

impl WindowLink {
    /// Creates a window link target.
    #[must_use]
    pub fn new(session: impl Into<SessionTarget>, index: u32) -> Self {
        Self {
            session: session.into(),
            index,
        }
    }

    /// Returns the owning session target.
    #[must_use]
    pub const fn session(&self) -> &SessionTarget {
        &self.session
    }

    /// Returns the window index in that session.
    #[must_use]
    pub const fn index(&self) -> u32 {
        self.index
    }
}

impl fmt::Display for WindowLink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.session, self.index)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{PaneId, SessionId, SessionName, WindowId};

    #[test]
    fn ids_round_trip() {
        assert_eq!(
            SessionId::from_str("$4")
                .ok()
                .map(|id| id.to_string())
                .as_deref(),
            Some("$4")
        );
        assert_eq!(
            WindowId::from_str("@3")
                .ok()
                .map(|id| id.to_string())
                .as_deref(),
            Some("@3")
        );
        assert_eq!(
            PaneId::from_str("%2")
                .ok()
                .map(|id| id.to_string())
                .as_deref(),
            Some("%2")
        );
    }

    #[test]
    fn invalid_session_names_are_rejected() {
        assert!(SessionName::new("").is_err());
        assert!(SessionName::new("bad:name").is_err());
        assert!(SessionName::new("bad.name").is_err());
    }
}
