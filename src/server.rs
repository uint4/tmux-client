//! Server configuration, command execution, discovery, and server-level operations.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command as ProcessCommand;
use tokio::sync::OnceCell;

use crate::formats::{fields_for_listing, parse_rows, render_format};
use crate::{
    AttachedClient, Client, Command, CommandResult, Error, FormatScope, NativeFilter, ObjectKind,
    OptionMap, OptionScope, OptionValue, Pane, PaneId, Result, Session, SessionId, SessionName,
    SessionTarget, SparseOptionMap, TmuxText, TmuxVersion, Window, WindowId,
};

/// The tmux socket identity used by a server handle.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Socket {
    /// Let tmux use its default socket.
    #[default]
    Default,
    /// Select a named socket with `-L`.
    Name(OsString),
    /// Select an exact socket path with `-S`.
    Path(PathBuf),
}

/// A tmux client color mode supplied on each command.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ColorMode {
    /// Let tmux detect color support.
    #[default]
    Default,
    /// Request 88-color mode (`-8`).
    Colors88,
    /// Request 256-color mode (`-2`).
    Colors256,
}

/// Immutable process and socket configuration shared by all object handles.
#[derive(Clone, Eq, PartialEq)]
pub struct ServerConfig {
    executable: PathBuf,
    socket: Socket,
    config_file: Option<PathBuf>,
    color_mode: ColorMode,
    environment: BTreeMap<OsString, OsString>,
    clear_environment: bool,
    timeout: Duration,
}

impl ServerConfig {
    /// Returns the configured tmux executable.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns the socket selector.
    #[must_use]
    pub const fn socket(&self) -> &Socket {
        &self.socket
    }

    /// Returns the optional tmux configuration file.
    #[must_use]
    pub fn config_file(&self) -> Option<&Path> {
        self.config_file.as_deref()
    }

    /// Returns the requested color mode.
    #[must_use]
    pub const fn color_mode(&self) -> ColorMode {
        self.color_mode
    }

    /// Returns process-environment overrides.
    #[must_use]
    pub const fn environment(&self) -> &BTreeMap<OsString, OsString> {
        &self.environment
    }

    /// Returns whether the child environment is cleared before overrides.
    #[must_use]
    pub const fn clears_environment(&self) -> bool {
        self.clear_environment
    }

    /// Returns the command timeout.
    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerConfig")
            .field("executable", &self.executable)
            .field("socket", &self.socket)
            .field("config_file", &self.config_file)
            .field("color_mode", &self.color_mode)
            .field(
                "environment_keys",
                &self.environment.keys().collect::<Vec<_>>(),
            )
            .field("clear_environment", &self.clear_environment)
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// Builder for [`Server`].
#[derive(Clone, Debug)]
pub struct ServerBuilder {
    config: ServerConfig,
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self {
            config: ServerConfig {
                executable: PathBuf::from("tmux"),
                socket: Socket::Default,
                config_file: None,
                color_mode: ColorMode::Default,
                environment: BTreeMap::new(),
                clear_environment: false,
                timeout: Duration::from_secs(30),
            },
        }
    }
}

impl ServerBuilder {
    /// Creates a builder with production defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the tmux executable path.
    #[must_use]
    pub fn executable(mut self, executable: impl Into<PathBuf>) -> Self {
        self.config.executable = executable.into();
        self
    }

    /// Uses a named tmux socket.
    #[must_use]
    pub fn socket_name(mut self, name: impl Into<OsString>) -> Self {
        self.config.socket = Socket::Name(name.into());
        self
    }

    /// Uses an exact tmux socket path.
    #[must_use]
    pub fn socket_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.socket = Socket::Path(path.into());
        self
    }

    /// Selects a socket explicitly.
    #[must_use]
    pub fn socket(mut self, socket: Socket) -> Self {
        self.config.socket = socket;
        self
    }

    /// Sets the tmux configuration file.
    #[must_use]
    pub fn config_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.config_file = Some(path.into());
        self
    }

    /// Sets the color mode.
    #[must_use]
    pub const fn color_mode(mut self, color_mode: ColorMode) -> Self {
        self.config.color_mode = color_mode;
        self
    }

    /// Sets one child-process environment override.
    #[must_use]
    pub fn environment(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.config.environment.insert(name.into(), value.into());
        self
    }

    /// Clears the inherited environment before applying overrides.
    #[must_use]
    pub const fn clear_environment(mut self, clear: bool) -> Self {
        self.config.clear_environment = clear;
        self
    }

    /// Sets the maximum duration for ordinary commands.
    pub fn timeout(mut self, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() {
            return Err(Error::InvalidArgument {
                argument: "timeout",
                message: "must be greater than zero".to_owned(),
            });
        }
        self.config.timeout = timeout;
        Ok(self)
    }

    /// Builds a cheap, cloneable server handle.
    #[must_use]
    pub fn build(self) -> Server {
        Server {
            inner: Arc::new(ServerInner {
                config: self.config,
                version: OnceCell::new(),
            }),
        }
    }
}

pub(crate) struct ServerInner {
    config: ServerConfig,
    version: OnceCell<TmuxVersion>,
}

impl fmt::Debug for ServerInner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerInner")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// A cheap handle to one tmux server configuration.
#[derive(Clone, Debug)]
pub struct Server {
    pub(crate) inner: Arc<ServerInner>,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    /// Creates a handle using the `tmux` executable and default socket.
    #[must_use]
    pub fn new() -> Self {
        ServerBuilder::new().build()
    }

    /// Starts configuring a server handle.
    #[must_use]
    pub fn builder() -> ServerBuilder {
        ServerBuilder::new()
    }

    /// Creates a server for the socket recorded in the `TMUX` environment variable.
    pub fn from_environment() -> Result<Self> {
        let value = std::env::var_os("TMUX").ok_or(Error::NotInsideTmux)?;
        let path = value
            .to_string_lossy()
            .split(',')
            .next()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or(Error::NotInsideTmux)?;
        Ok(Self::builder().socket_path(path).build())
    }

    /// Returns shared immutable configuration.
    #[must_use]
    pub fn config(&self) -> &ServerConfig {
        &self.inner.config
    }

    fn process(&self) -> ProcessCommand {
        let config = &self.inner.config;
        let mut process = ProcessCommand::new(&config.executable);
        process.kill_on_drop(true);
        if config.clear_environment {
            process.env_clear();
        }
        process.envs(&config.environment);
        match &config.socket {
            Socket::Default => {}
            Socket::Name(name) => {
                process.arg("-L").arg(name);
            }
            Socket::Path(path) => {
                process.arg("-S").arg(path);
            }
        }
        if let Some(path) = &config.config_file {
            process.arg("-f").arg(path);
        }
        match config.color_mode {
            ColorMode::Default => {}
            ColorMode::Colors88 => {
                process.arg("-8");
            }
            ColorMode::Colors256 => {
                process.arg("-2");
            }
        }
        process
    }

    /// Executes a command without treating a non-zero tmux exit status as an error.
    pub async fn cmd(&self, command: Command) -> Result<CommandResult> {
        let summary = command.summary();
        let input = command.standard_input().cloned();
        let mut process = self.process();
        process
            .arg(command.subcommand())
            .args(command.arguments())
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(feature = "tracing")]
        tracing::debug!(command = %summary, "executing tmux command");
        let mut child = process.spawn().map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                Error::ExecutableNotFound {
                    executable: self.inner.config.executable.clone(),
                }
            } else {
                Error::Io {
                    operation: "spawn tmux",
                    source,
                }
            }
        })?;
        let timeout = self.inner.config.timeout;
        let execution = async move {
            if let Some(input) = input {
                let mut stdin = child.stdin.take().ok_or_else(|| Error::Io {
                    operation: "open tmux stdin",
                    source: std::io::Error::other("Tokio did not provide piped stdin"),
                })?;
                stdin
                    .write_all(input.as_bytes())
                    .await
                    .map_err(|source| Error::Io {
                        operation: "write tmux stdin",
                        source,
                    })?;
                stdin.shutdown().await.map_err(|source| Error::Io {
                    operation: "close tmux stdin",
                    source,
                })?;
            }
            child.wait_with_output().await.map_err(|source| Error::Io {
                operation: "wait for tmux",
                source,
            })
        };
        let output = tokio::time::timeout(timeout, execution)
            .await
            .map_err(|_| Error::Timeout {
                summary: summary.clone(),
                timeout,
            })??;
        Ok(CommandResult::new(
            output.status,
            output.stdout,
            output.stderr,
            summary,
        ))
    }

    pub(crate) async fn checked(
        &self,
        operation: &'static str,
        command: Command,
    ) -> Result<CommandResult> {
        self.cmd(command).await?.ensure_success(operation)
    }

    /// Reads and caches the version reported by the configured executable.
    pub async fn version(&self) -> Result<&TmuxVersion> {
        self.inner
            .version
            .get_or_try_init(|| async {
                let result = self.checked("read version", Command::new("-V")).await?;
                let text = result.stdout().to_str().map_err(|source| Error::Decode {
                    context: "tmux version",
                    message: source.to_string(),
                })?;
                let version = TmuxVersion::parse(text)?;
                version.require("tmux-client", crate::ReleaseVersion::MINIMUM)?;
                Ok(version)
            })
            .await
    }

    pub(crate) async fn list_rows(
        &self,
        subcommand: &'static str,
        scopes: &[FormatScope],
        arguments: impl IntoIterator<Item = OsString>,
        filter: Option<&NativeFilter>,
    ) -> Result<Vec<BTreeMap<&'static str, TmuxText>>> {
        let fields = fields_for_listing(scopes, self.version().await?);
        let mut command = Command::new(subcommand).args(arguments);
        if let Some(filter) = filter {
            command = command.arg("-f").arg(filter.as_str());
        }
        command = command.arg("-F").arg(render_format(&fields));
        let result = self.checked(subcommand, command).await?;
        parse_rows(result.stdout().as_bytes(), &fields)
    }

    /// Lists sessions, preserving an error when the server is unavailable.
    pub async fn sessions(&self) -> Result<Vec<Session>> {
        self.sessions_filtered(None).await
    }

    /// Lists sessions with an optional native tmux filter.
    pub async fn sessions_filtered(&self, filter: Option<&NativeFilter>) -> Result<Vec<Session>> {
        self.list_rows("list-sessions", &[FormatScope::Session], [], filter)
            .await?
            .into_iter()
            .map(|row| Session::from_row(self.clone(), row))
            .collect()
    }

    /// Lists sessions, returning an empty collection on discovery failure.
    pub async fn sessions_or_empty(&self) -> Vec<Session> {
        self.sessions().await.unwrap_or_default()
    }

    /// Lists sessions with at least one attached client.
    pub async fn attached_sessions(&self) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();
        for session in self.sessions().await? {
            if session
                .snapshot()
                .attached_clients()?
                .is_some_and(|count| count > 0)
            {
                sessions.push(session);
            }
        }
        Ok(sessions)
    }

    /// Resolves a session ID to a fresh snapshot.
    pub async fn session(&self, id: SessionId) -> Result<Session> {
        self.sessions()
            .await?
            .into_iter()
            .find(|session| session.id() == id)
            .ok_or_else(|| Error::ObjectNotFound {
                kind: ObjectKind::Session,
                target: id.to_string(),
            })
    }

    /// Resolves an exact session name to a fresh snapshot.
    pub async fn session_by_name(&self, name: &SessionName) -> Result<Session> {
        let matches = self
            .sessions()
            .await?
            .into_iter()
            .filter(|session| session.name() == Some(name))
            .collect::<Vec<_>>();
        match matches.len() {
            0 => Err(Error::ObjectNotFound {
                kind: ObjectKind::Session,
                target: name.to_string(),
            }),
            1 => matches.into_iter().next().ok_or_else(|| Error::Decode {
                context: "session lookup",
                message: "single match disappeared".to_owned(),
            }),
            count => Err(Error::MultipleObjects {
                kind: ObjectKind::Session,
                count,
                query: format!("session_name == {name}"),
            }),
        }
    }

    /// Lists every window link. A window linked into two sessions appears twice.
    pub async fn windows(&self) -> Result<Vec<Window>> {
        self.windows_filtered(None).await
    }

    /// Lists window links with an optional native tmux filter.
    pub async fn windows_filtered(&self, filter: Option<&NativeFilter>) -> Result<Vec<Window>> {
        self.list_rows(
            "list-windows",
            &[FormatScope::Session, FormatScope::Window],
            [OsString::from("-a")],
            filter,
        )
        .await?
        .into_iter()
        .map(|row| Window::from_row(self.clone(), row))
        .collect()
    }

    /// Lists window links, returning an empty collection on discovery failure.
    pub async fn windows_or_empty(&self) -> Vec<Window> {
        self.windows().await.unwrap_or_default()
    }

    /// Resolves a window ID. Fails loudly when the window has multiple links.
    pub async fn window(&self, id: WindowId) -> Result<Window> {
        let matches = self
            .windows()
            .await?
            .into_iter()
            .filter(|window| window.id() == id)
            .collect::<Vec<_>>();
        match matches.len() {
            0 => Err(Error::ObjectNotFound {
                kind: ObjectKind::Window,
                target: id.to_string(),
            }),
            1 => matches.into_iter().next().ok_or_else(|| Error::Decode {
                context: "window lookup",
                message: "single match disappeared".to_owned(),
            }),
            count => Err(Error::MultipleObjects {
                kind: ObjectKind::Window,
                count,
                query: format!("window_id == {id}; select a WindowLink instead"),
            }),
        }
    }

    /// Lists every pane with its parent identities.
    pub async fn panes(&self) -> Result<Vec<Pane>> {
        self.panes_filtered(None).await
    }

    /// Lists panes with an optional native tmux filter.
    pub async fn panes_filtered(&self, filter: Option<&NativeFilter>) -> Result<Vec<Pane>> {
        self.list_rows(
            "list-panes",
            &[FormatScope::Session, FormatScope::Window, FormatScope::Pane],
            [OsString::from("-a")],
            filter,
        )
        .await?
        .into_iter()
        .map(|row| Pane::from_row(self.clone(), row))
        .collect()
    }

    /// Lists panes, returning an empty collection on discovery failure.
    pub async fn panes_or_empty(&self) -> Vec<Pane> {
        self.panes().await.unwrap_or_default()
    }

    /// Resolves a pane ID to a fresh snapshot.
    pub async fn pane(&self, id: PaneId) -> Result<Pane> {
        self.panes()
            .await?
            .into_iter()
            .find(|pane| pane.id() == id)
            .ok_or_else(|| Error::ObjectNotFound {
                kind: ObjectKind::Pane,
                target: id.to_string(),
            })
    }

    /// Lists attached tmux clients.
    pub async fn clients(&self) -> Result<Vec<Client>> {
        self.list_rows(
            "list-clients",
            &[
                FormatScope::Session,
                FormatScope::Window,
                FormatScope::Pane,
                FormatScope::Client,
            ],
            [],
            None,
        )
        .await?
        .into_iter()
        .map(|row| Client::from_row(self.clone(), row))
        .collect()
    }

    /// Lists clients, returning an empty collection on discovery failure.
    pub async fn clients_or_empty(&self) -> Vec<Client> {
        self.clients().await.unwrap_or_default()
    }

    /// Returns whether an exact session name exists.
    pub async fn has_session(&self, name: &SessionName) -> Result<bool> {
        let target = format!("={name}");
        let result = self
            .cmd(Command::new("has-session").arg("-t").arg(target))
            .await?;
        Ok(result.success())
    }

    /// Kills a selected session target.
    pub async fn kill_session(&self, target: impl Into<SessionTarget>) -> Result<()> {
        self.checked(
            "kill session",
            Command::new("kill-session").target(target.into()),
        )
        .await
        .map(|_| ())
    }

    /// Creates a session and hydrates its initial snapshot.
    pub async fn new_session(&self, options: NewSession) -> Result<Session> {
        let mut command = Command::new("new-session")
            .arg("-P")
            .arg("-F")
            .arg("#{session_id}");
        if options.detached {
            command = command.arg("-d");
        }
        if options.attach_if_exists {
            command = command.arg("-A");
        }
        if let Some(name) = options.name {
            command = command.arg("-s").arg(name.to_string());
        }
        if let Some(window_name) = options.window_name {
            command = command.arg("-n").arg(window_name);
        }
        if let Some(cwd) = options.cwd {
            command = command.arg("-c").arg(cwd);
        }
        if let Some((width, height)) = options.size {
            command = command
                .arg("-x")
                .arg(width.to_string())
                .arg("-y")
                .arg(height.to_string());
        }
        if let Some(group) = options.group {
            command = command.arg("-t").arg(group.to_string());
        }
        for (name, value) in options.environment {
            command =
                command
                    .arg("-e")
                    .sensitive_arg(format!("{}={}", name, value.to_string_lossy()));
        }
        if let Some(shell_command) = options.shell_command {
            command = command.sensitive_arg(shell_command);
        }
        let result = self.checked("create session", command).await?;
        let id = single_line(result.stdout(), "new-session session ID")?.parse()?;
        self.session(id).await
    }

    /// Creates a temporary session, runs an async operation, and always attempts cleanup.
    pub async fn with_session<F, Fut, T>(&self, options: NewSession, operation: F) -> Result<T>
    where
        F: FnOnce(Session) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let session = self.new_session(options).await?;
        let cleanup = session.clone();
        let outcome = operation(session).await;
        let cleanup_result = cleanup.kill().await;
        match (outcome, cleanup_result) {
            (Ok(value), Ok(())) => Ok(value),
            (Ok(_), Err(source)) => Err(Error::Cleanup {
                operation: "with_session",
                source: Box::new(source),
            }),
            (Err(source), _) => Err(source),
        }
    }

    /// Kills the tmux server. A missing server is treated as already killed.
    pub async fn kill(&self) -> Result<()> {
        let result = self.cmd(Command::new("kill-server")).await?;
        if result.success() || daemon_not_running(result.stderr().as_bytes()) {
            Ok(())
        } else {
            result.ensure_success("kill server").map(|_| ())
        }
    }

    /// Returns whether the configured tmux server responds.
    pub async fn is_alive(&self) -> Result<bool> {
        Ok(self.cmd(Command::new("list-sessions")).await?.success())
    }

    /// Returns a server-unavailable error unless tmux responds successfully.
    pub async fn ensure_alive(&self) -> Result<()> {
        let result = self.cmd(Command::new("list-sessions")).await?;
        if result.success() {
            Ok(())
        } else {
            Err(Error::ServerUnavailable {
                message: result.stderr().to_string_lossy().into_owned(),
            })
        }
    }

    /// Resolves `TMUX_PANE` using this server's socket configuration.
    pub async fn pane_from_environment(&self) -> Result<Pane> {
        let value = std::env::var("TMUX_PANE").map_err(|_| Error::NotInsideTmux)?;
        let id = value.parse()?;
        self.pane(id).await
    }

    /// Sources a tmux configuration file.
    pub async fn source_file(&self, path: impl AsRef<Path>) -> Result<()> {
        self.checked(
            "source config",
            Command::new("source-file").arg(path.as_ref()),
        )
        .await
        .map(|_| ())
    }

    /// Sources or validates a config file with explicit flags.
    pub async fn source_file_with(
        &self,
        path: impl AsRef<Path>,
        quiet: bool,
        parse_only: bool,
        verbose: bool,
    ) -> Result<()> {
        let mut command = Command::new("source-file");
        for (enabled, flag) in [(quiet, "-q"), (parse_only, "-n"), (verbose, "-v")] {
            if enabled {
                command = command.arg(flag);
            }
        }
        self.checked("source config", command.arg(path.as_ref()))
            .await
            .map(|_| ())
    }

    /// Displays a message and returns tmux's byte-preserving expansion.
    pub async fn display_message(&self, message: impl Into<OsString>) -> Result<TmuxText> {
        self.checked(
            "display message",
            Command::new("display-message").arg("-p").arg(message),
        )
        .await
        .map(|result| result.stdout().clone())
    }

    /// Shows options at a scope and optional target.
    pub async fn show_options(
        &self,
        scope: OptionScope,
        target: Option<&str>,
    ) -> Result<OptionMap> {
        let mut command = Command::new("show-options")
            .args(scope.show_flags())
            .arg("-A");
        if let Some(target) = target {
            command = command.arg("-t").arg(target);
        }
        let result = self.checked("show options", command).await?;
        OptionMap::parse(result.stdout().as_bytes())
    }

    /// Shows indexed options and hooks as sparse arrays.
    pub async fn show_sparse_options(
        &self,
        scope: OptionScope,
        target: Option<&str>,
    ) -> Result<SparseOptionMap> {
        let mut command = Command::new("show-options").args(scope.show_flags());
        if let Some(target) = target {
            command = command.arg("-t").arg(target);
        }
        let result = self.checked("show sparse options", command).await?;
        SparseOptionMap::parse(result.stdout().as_bytes())
    }

    /// Sets an option or hook value.
    pub async fn set_option(
        &self,
        scope: OptionScope,
        target: Option<&str>,
        name: &str,
        value: &OptionValue,
        append: bool,
    ) -> Result<()> {
        validate_name("option name", name)?;
        let mut command = Command::new("set-option").args(scope.show_flags());
        if append {
            command = command.arg("-a");
        }
        if let Some(target) = target {
            command = command.arg("-t").arg(target);
        }
        command = command.arg(name).arg(value.to_os_string());
        self.checked("set option", command).await.map(|_| ())
    }

    /// Unsets an option or hook.
    pub async fn unset_option(
        &self,
        scope: OptionScope,
        target: Option<&str>,
        name: &str,
    ) -> Result<()> {
        validate_name("option name", name)?;
        let mut command = Command::new("set-option")
            .args(scope.show_flags())
            .arg("-u");
        if let Some(target) = target {
            command = command.arg("-t").arg(target);
        }
        self.checked("unset option", command.arg(name))
            .await
            .map(|_| ())
    }

    /// Shows the global tmux environment. Removed entries have a `None` value.
    pub async fn environment(&self) -> Result<BTreeMap<String, Option<TmuxText>>> {
        let result = self
            .checked(
                "show environment",
                Command::new("show-environment").arg("-g"),
            )
            .await?;
        parse_environment(result.stdout().as_bytes())
    }

    /// Returns one global environment entry.
    ///
    /// The outer option distinguishes an unknown name; the inner option is
    /// `None` when tmux records the name as explicitly removed.
    pub async fn environment_entry(&self, name: &str) -> Result<Option<Option<TmuxText>>> {
        validate_name("environment name", name)?;
        Ok(self.environment().await?.remove(name))
    }

    /// Sets a global tmux environment variable.
    pub async fn set_environment(&self, name: &str, value: impl Into<OsString>) -> Result<()> {
        self.set_environment_with(name, value, false, false).await
    }

    /// Sets a global environment variable with format expansion or hidden storage.
    pub async fn set_environment_with(
        &self,
        name: &str,
        value: impl Into<OsString>,
        expand_formats: bool,
        hidden: bool,
    ) -> Result<()> {
        validate_name("environment name", name)?;
        let mut command = Command::new("set-environment").arg("-g");
        if expand_formats {
            command = command.arg("-F");
        }
        if hidden {
            command = command.arg("-h");
        }
        self.checked("set environment", command.arg(name).sensitive_arg(value))
            .await
            .map(|_| ())
    }

    /// Unsets a global tmux environment variable.
    pub async fn unset_environment(&self, name: &str) -> Result<()> {
        validate_name("environment name", name)?;
        self.checked(
            "unset environment",
            Command::new("set-environment")
                .arg("-g")
                .arg("-u")
                .arg(name),
        )
        .await
        .map(|_| ())
    }

    /// Removes a global variable while retaining it in tmux's update list.
    pub async fn remove_environment(&self, name: &str) -> Result<()> {
        validate_name("environment name", name)?;
        self.checked(
            "remove environment",
            Command::new("set-environment")
                .arg("-g")
                .arg("-r")
                .arg(name),
        )
        .await
        .map(|_| ())
    }

    /// Spawns an interactive tmux attachment with inherited standard streams.
    pub fn attach(
        &self,
        target: impl Into<SessionTarget>,
        read_only: bool,
    ) -> Result<AttachedClient> {
        let mut command = Command::new("attach-session");
        if read_only {
            command = command.arg("-r");
        }
        command = command.arg("-t").arg(target.into().to_string());
        let summary = command.summary();
        let mut process = self.process();
        process
            .arg(command.subcommand())
            .args(command.arguments())
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let child = process.spawn().map_err(|source| Error::Io {
            operation: "attach tmux client",
            source,
        })?;
        Ok(AttachedClient::new(child, summary))
    }
}

/// Typed arguments for creating a session.
#[derive(Clone, Debug)]
pub struct NewSession {
    name: Option<SessionName>,
    detached: bool,
    attach_if_exists: bool,
    window_name: Option<OsString>,
    cwd: Option<PathBuf>,
    size: Option<(u32, u32)>,
    group: Option<SessionTarget>,
    environment: BTreeMap<String, OsString>,
    shell_command: Option<OsString>,
}

impl Default for NewSession {
    fn default() -> Self {
        Self {
            name: None,
            detached: true,
            attach_if_exists: false,
            window_name: None,
            cwd: None,
            size: None,
            group: None,
            environment: BTreeMap::new(),
            shell_command: None,
        }
    }
}

impl NewSession {
    /// Creates default detached-session arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Validates and sets the session name.
    pub fn name(mut self, name: impl Into<String>) -> Result<Self> {
        self.name = Some(SessionName::new(name)?);
        Ok(self)
    }

    /// Chooses whether tmux should create the session detached.
    #[must_use]
    pub const fn detached(mut self, detached: bool) -> Self {
        self.detached = detached;
        self
    }

    /// Enables tmux's attach-or-create behavior (`-A`).
    #[must_use]
    pub const fn attach_if_exists(mut self, enabled: bool) -> Self {
        self.attach_if_exists = enabled;
        self
    }

    /// Sets the first window's name.
    #[must_use]
    pub fn window_name(mut self, name: impl Into<OsString>) -> Self {
        self.window_name = Some(name.into());
        self
    }

    /// Sets the starting directory.
    #[must_use]
    pub fn cwd(mut self, path: impl Into<PathBuf>) -> Self {
        self.cwd = Some(path.into());
        self
    }

    /// Sets an initial terminal size.
    pub fn size(mut self, width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(Error::InvalidArgument {
                argument: "session size",
                message: "width and height must be non-zero".to_owned(),
            });
        }
        self.size = Some((width, height));
        Ok(self)
    }

    /// Adds the session to a session group.
    #[must_use]
    pub fn group(mut self, target: impl Into<SessionTarget>) -> Self {
        self.group = Some(target.into());
        self
    }

    /// Sets an initial session environment variable. Values are redacted in diagnostics.
    pub fn environment(
        mut self,
        name: impl Into<String>,
        value: impl Into<OsString>,
    ) -> Result<Self> {
        let name = name.into();
        validate_name("environment name", &name)?;
        self.environment.insert(name, value.into());
        Ok(self)
    }

    /// Sets the tmux shell command for the first pane.
    #[must_use]
    pub fn shell_command(mut self, command: impl Into<OsString>) -> Self {
        self.shell_command = Some(command.into());
        self
    }
}

pub(crate) fn single_line<'a>(text: &'a TmuxText, context: &'static str) -> Result<&'a str> {
    let value = text.to_str().map_err(|source| Error::Decode {
        context,
        message: source.to_string(),
    })?;
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains('\n') {
        return Err(Error::Decode {
            context,
            message: "expected exactly one non-empty output line".to_owned(),
        });
    }
    Ok(value)
}

pub(crate) fn validate_name(argument: &'static str, name: &str) -> Result<()> {
    if name.is_empty() || name.contains(['=', '\0', '\n']) {
        return Err(Error::InvalidArgument {
            argument,
            message: "must be non-empty and contain no '=', NUL, or newline".to_owned(),
        });
    }
    Ok(())
}

pub(crate) fn parse_environment(bytes: &[u8]) -> Result<BTreeMap<String, Option<TmuxText>>> {
    let mut environment = BTreeMap::new();
    for line in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        if let Some(name) = line.strip_prefix(b"-") {
            let name = std::str::from_utf8(name).map_err(|source| Error::Decode {
                context: "environment name",
                message: source.to_string(),
            })?;
            environment.insert(name.to_owned(), None);
            continue;
        }
        let Some(split) = line.iter().position(|byte| *byte == b'=') else {
            return Err(Error::Decode {
                context: "tmux environment",
                message: "entry had no '=' separator".to_owned(),
            });
        };
        let name = std::str::from_utf8(&line[..split]).map_err(|source| Error::Decode {
            context: "environment name",
            message: source.to_string(),
        })?;
        environment.insert(
            name.to_owned(),
            Some(TmuxText::new(line[split + 1..].to_vec())),
        );
    }
    Ok(environment)
}

fn daemon_not_running(stderr: &[u8]) -> bool {
    contains_bytes(stderr, b"no server running")
        || (contains_bytes(stderr, b"error connecting to")
            && contains_bytes(stderr, b"No such file or directory"))
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Server, parse_environment};

    #[test]
    fn builder_redacts_environment_values() {
        let server = Server::builder().environment("TOKEN", "secret").build();
        let debug = format!("{:?}", server.config());
        assert!(debug.contains("TOKEN"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn zero_timeout_is_rejected() {
        assert!(Server::builder().timeout(Duration::ZERO).is_err());
    }

    #[test]
    fn environment_parser_preserves_values() {
        let Ok(values) = parse_environment(b"A=x=y\n-B\n") else {
            return;
        };
        assert_eq!(
            values
                .get("A")
                .and_then(Option::as_ref)
                .map(crate::TmuxText::as_bytes),
            Some(b"x=y".as_slice())
        );
        assert_eq!(values.get("B"), Some(&None));
    }
}
