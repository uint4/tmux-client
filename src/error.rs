//! Errors returned by the library.

use std::io;
use std::time::Duration;

use thiserror::Error;

use crate::{CommandResult, CommandSummary, ReleaseVersion, TmuxVersion};

/// A result returned by `tmux-client`.
pub type Result<T> = std::result::Result<T, Error>;

/// The kind of tmux object involved in an error.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObjectKind {
    /// A server.
    Server,
    /// A session.
    Session,
    /// A window.
    Window,
    /// A pane.
    Pane,
    /// A client.
    Client,
    /// A buffer.
    Buffer,
}

impl std::fmt::Display for ObjectKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Server => "server",
            Self::Session => "session",
            Self::Window => "window",
            Self::Pane => "pane",
            Self::Client => "client",
            Self::Buffer => "buffer",
        })
    }
}

/// A category of option parsing or mutation failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OptionErrorKind {
    /// An option value could not be parsed.
    Parse,
    /// An option value had an unsupported representation.
    Unsupported,
    /// An indexed option was malformed.
    SparseIndex,
}

impl std::fmt::Display for OptionErrorKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Parse => "parse",
            Self::Unsupported => "unsupported",
            Self::SparseIndex => "sparse-index",
        })
    }
}

/// A non-exhaustive error produced by configuration, execution, or parsing.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The configured tmux executable was not found.
    #[error("tmux executable {executable:?} was not found")]
    ExecutableNotFound {
        /// The configured executable path.
        executable: std::path::PathBuf,
    },

    /// A process or file operation failed.
    #[error("{operation} failed: {source}")]
    Io {
        /// A static operation name.
        operation: &'static str,
        /// The underlying operating-system error.
        #[source]
        source: io::Error,
    },

    /// A command exceeded the configured timeout.
    #[error("tmux command `{summary}` timed out after {timeout:?}")]
    Timeout {
        /// The redacted command description.
        summary: CommandSummary,
        /// The configured timeout.
        timeout: Duration,
    },

    /// tmux returned a non-zero status for a typed operation.
    #[error("tmux {operation} failed: {result:?}")]
    CommandFailed {
        /// The attempted high-level operation.
        operation: &'static str,
        /// The complete byte-preserving result.
        result: Box<CommandResult>,
    },

    /// No tmux server was available on the requested socket.
    #[error("no tmux server is available: {message}")]
    ServerUnavailable {
        /// Diagnostic text returned by tmux.
        message: String,
    },

    /// A requested tmux object did not exist.
    #[error("{kind} {target} was not found")]
    ObjectNotFound {
        /// The expected object kind.
        kind: ObjectKind,
        /// The target used for lookup.
        target: String,
    },

    /// A lookup that expected one object returned multiple objects.
    #[error("query `{query}` matched {count} {kind} objects")]
    MultipleObjects {
        /// The object kind.
        kind: ObjectKind,
        /// The number of matches.
        count: usize,
        /// A redacted query description.
        query: String,
    },

    /// An ID did not use the required prefix and numeric suffix.
    #[error("invalid {kind} ID `{value}`")]
    InvalidId {
        /// The ID kind.
        kind: &'static str,
        /// The invalid representation.
        value: String,
    },

    /// A session name used tmux-reserved target punctuation.
    #[error("invalid tmux session name `{0}`")]
    InvalidSessionName(String),

    /// A public argument did not satisfy its contract.
    #[error("invalid {argument}: {message}")]
    InvalidArgument {
        /// The argument name.
        argument: &'static str,
        /// The validation failure.
        message: String,
    },

    /// tmux returned an unrecognized version.
    #[error("invalid tmux version `{0}`")]
    InvalidVersion(String),

    /// The connected tmux version lacks a requested capability.
    #[error("{capability} requires tmux {required} or newer, found {found}")]
    Unsupported {
        /// The requested operation or field.
        capability: &'static str,
        /// The minimum release.
        required: ReleaseVersion,
        /// The actual connected version.
        found: TmuxVersion,
    },

    /// Structured tmux output could not be decoded.
    #[error("could not decode {context}: {message}")]
    Decode {
        /// The output family being decoded.
        context: &'static str,
        /// A safe explanation without untrusted output interpolation.
        message: String,
    },

    /// An option value or index was invalid.
    #[error("option {kind} error: {message}")]
    Option {
        /// The failure category.
        kind: OptionErrorKind,
        /// The failure description.
        message: String,
    },

    /// An operation required being inside an attached tmux client.
    #[error("the operation requires an attached tmux client")]
    NotInsideTmux,

    /// A wait condition expired before it was satisfied.
    #[error("timed out waiting for {condition}")]
    WaitTimeout {
        /// A description of the condition.
        condition: String,
    },

    /// The main operation succeeded but deterministic cleanup failed.
    #[error("cleanup after {operation} failed: {source}")]
    Cleanup {
        /// The operation whose scope was being cleaned up.
        operation: &'static str,
        /// The cleanup error.
        #[source]
        source: Box<Error>,
    },
}
