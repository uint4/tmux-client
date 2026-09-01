//! Shell-free tmux command construction and results.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::process::ExitStatus;

use crate::{Error, Result, TmuxText};

/// A redacted, human-readable command description.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CommandSummary(String);

impl CommandSummary {
    /// Returns the redacted command text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommandSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Argument {
    value: OsString,
    sensitive: bool,
}

/// A tmux subcommand and its arguments.
///
/// The tmux executable, socket, config, and color flags are supplied by
/// [`Server`](crate::Server), not by this value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command {
    subcommand: OsString,
    arguments: Vec<Argument>,
    input: Option<TmuxText>,
    sensitive_input: bool,
}

impl Command {
    /// Creates a command for a tmux subcommand such as `list-sessions`.
    #[must_use]
    pub fn new(subcommand: impl Into<OsString>) -> Self {
        Self {
            subcommand: subcommand.into(),
            arguments: Vec::new(),
            input: None,
            sensitive_input: false,
        }
    }

    /// Adds a visible command argument.
    #[must_use]
    pub fn arg(mut self, argument: impl Into<OsString>) -> Self {
        self.arguments.push(Argument {
            value: argument.into(),
            sensitive: false,
        });
        self
    }

    /// Adds several visible command arguments.
    #[must_use]
    pub fn args<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.arguments
            .extend(arguments.into_iter().map(|value| Argument {
                value: value.into(),
                sensitive: false,
            }));
        self
    }

    /// Adds an argument whose value is omitted from diagnostics.
    #[must_use]
    pub fn sensitive_arg(mut self, argument: impl Into<OsString>) -> Self {
        self.arguments.push(Argument {
            value: argument.into(),
            sensitive: true,
        });
        self
    }

    /// Adds a conventional `-t <target>` pair.
    #[must_use]
    pub fn target(self, target: impl fmt::Display) -> Self {
        self.arg("-t").arg(target.to_string())
    }

    /// Supplies byte-preserving standard input to the tmux process.
    #[must_use]
    pub fn input(mut self, input: impl Into<TmuxText>) -> Self {
        self.input = Some(input.into());
        self.sensitive_input = false;
        self
    }

    /// Supplies standard input while marking its content sensitive.
    #[must_use]
    pub fn sensitive_input(mut self, input: impl Into<TmuxText>) -> Self {
        self.input = Some(input.into());
        self.sensitive_input = true;
        self
    }

    /// Returns the tmux subcommand.
    #[must_use]
    pub fn subcommand(&self) -> &OsStr {
        &self.subcommand
    }

    /// Returns the arguments in execution order.
    #[must_use]
    pub fn arguments(&self) -> impl ExactSizeIterator<Item = &OsStr> {
        self.arguments
            .iter()
            .map(|argument| argument.value.as_os_str())
    }

    pub(crate) const fn standard_input(&self) -> Option<&TmuxText> {
        self.input.as_ref()
    }

    /// Creates a redacted diagnostic description.
    #[must_use]
    pub fn summary(&self) -> CommandSummary {
        let mut parts = Vec::with_capacity(self.arguments.len() + 1);
        parts.push(self.subcommand.to_string_lossy().into_owned());
        parts.extend(self.arguments.iter().map(|argument| {
            if argument.sensitive {
                "<redacted>".to_owned()
            } else {
                argument.value.to_string_lossy().into_owned()
            }
        }));
        if self.input.is_some() {
            parts.push(if self.sensitive_input {
                "<stdin:redacted>".to_owned()
            } else {
                "<stdin>".to_owned()
            });
        }
        CommandSummary(parts.join(" "))
    }
}

/// The raw result of a completed tmux command.
///
/// A non-zero exit status is data at this layer. Call [`Self::ensure_success`]
/// or use a typed operation to translate it into [`Error::CommandFailed`].
#[derive(Clone, Debug)]
pub struct CommandResult {
    status: ExitStatus,
    stdout: TmuxText,
    stderr: TmuxText,
    summary: CommandSummary,
}

impl CommandResult {
    pub(crate) fn new(
        status: ExitStatus,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        summary: CommandSummary,
    ) -> Self {
        Self {
            status,
            stdout: stdout.into(),
            stderr: stderr.into(),
            summary,
        }
    }

    /// Returns whether tmux exited successfully.
    #[must_use]
    pub fn success(&self) -> bool {
        self.status.success()
    }

    /// Returns the operating-system exit status.
    #[must_use]
    pub const fn status(&self) -> ExitStatus {
        self.status
    }

    /// Returns raw standard output.
    #[must_use]
    pub const fn stdout(&self) -> &TmuxText {
        &self.stdout
    }

    /// Returns raw standard error.
    #[must_use]
    pub const fn stderr(&self) -> &TmuxText {
        &self.stderr
    }

    /// Returns the redacted command summary.
    #[must_use]
    pub const fn summary(&self) -> &CommandSummary {
        &self.summary
    }

    /// Iterates over standard-output lines without requiring UTF-8.
    pub fn stdout_lines(&self) -> impl Iterator<Item = TmuxText> + '_ {
        self.stdout
            .as_bytes()
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| TmuxText::new(line.strip_suffix(b"\r").unwrap_or(line).to_vec()))
    }

    /// Iterates over standard-error lines without requiring UTF-8.
    pub fn stderr_lines(&self) -> impl Iterator<Item = TmuxText> + '_ {
        self.stderr
            .as_bytes()
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| TmuxText::new(line.strip_suffix(b"\r").unwrap_or(line).to_vec()))
    }

    /// Turns a non-zero status into a typed error.
    pub fn ensure_success(self, operation: &'static str) -> Result<Self> {
        if self.success() {
            Ok(self)
        } else {
            Err(Error::CommandFailed {
                operation,
                result: Box::new(self),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Command;

    #[test]
    fn redacts_sensitive_arguments() {
        let command = Command::new("set-environment")
            .arg("TOKEN")
            .sensitive_arg("secret")
            .sensitive_input("secret input");
        assert_eq!(
            command.summary().as_str(),
            "set-environment TOKEN <redacted> <stdin:redacted>"
        );
        assert!(!command.summary().as_str().contains("secret"));
    }
}
