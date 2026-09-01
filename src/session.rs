//! Session snapshots, hierarchy traversal, and mutations.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::future::Future;

use crate::{
    AttachedClient, Client, Command, Error, NewWindow, ObjectKind, OptionMap, OptionScope,
    OptionValue, Pane, Result, Server, SessionId, SessionName, SessionSnapshot, SparseOptionMap,
    TmuxText, Window, WindowId, WindowTarget,
};

/// A tmux session handle with an owned snapshot.
#[derive(Clone, Debug)]
pub struct Session {
    server: Server,
    id: SessionId,
    name: Option<SessionName>,
    snapshot: SessionSnapshot,
}

impl Session {
    pub(crate) fn from_row(server: Server, row: BTreeMap<&'static str, TmuxText>) -> Result<Self> {
        let snapshot = SessionSnapshot::new(row);
        let id = snapshot.id()?.ok_or_else(|| Error::Decode {
            context: "session row",
            message: "session_id was empty".to_owned(),
        })?;
        let name = snapshot.name()?;
        Ok(Self {
            server,
            id,
            name,
            snapshot,
        })
    }

    /// Resolves the session containing the pane named by `TMUX_PANE`.
    pub async fn from_environment() -> Result<Self> {
        Server::from_environment()?
            .pane_from_environment()
            .await?
            .session()
            .await
    }

    /// Returns the immutable tmux ID.
    #[must_use]
    pub const fn id(&self) -> SessionId {
        self.id
    }

    /// Returns the name captured in the current snapshot.
    #[must_use]
    pub const fn name(&self) -> Option<&SessionName> {
        self.name.as_ref()
    }

    /// Returns the owned snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &SessionSnapshot {
        &self.snapshot
    }

    /// Returns the server handle used by this object.
    #[must_use]
    pub const fn server(&self) -> &Server {
        &self.server
    }

    /// Replaces this object's state with a fresh snapshot.
    pub async fn refresh(&mut self) -> Result<()> {
        *self = self.server.session(self.id).await?;
        Ok(())
    }

    /// Runs an arbitrary tmux command targeted at this session.
    pub async fn cmd(&self, command: Command) -> Result<crate::CommandResult> {
        self.server.cmd(command.target(self.id)).await
    }

    /// Lists window links owned by this session.
    pub async fn windows(&self) -> Result<Vec<Window>> {
        Ok(self
            .server
            .windows()
            .await?
            .into_iter()
            .filter(|window| window.session_id() == self.id)
            .collect())
    }

    /// Lists windows, returning an empty collection on failure.
    pub async fn windows_or_empty(&self) -> Vec<Window> {
        self.windows().await.unwrap_or_default()
    }

    /// Lists every pane belonging to this session.
    pub async fn panes(&self) -> Result<Vec<Pane>> {
        Ok(self
            .server
            .panes()
            .await?
            .into_iter()
            .filter(|pane| pane.session_id() == self.id)
            .collect())
    }

    /// Lists panes, returning an empty collection on failure.
    pub async fn panes_or_empty(&self) -> Vec<Pane> {
        self.panes().await.unwrap_or_default()
    }

    /// Returns the active pane in this session's active window.
    pub async fn active_pane(&self) -> Result<Pane> {
        self.active_window().await?.active_pane().await
    }

    /// Returns the active window link.
    pub async fn active_window(&self) -> Result<Window> {
        let mut active = Vec::new();
        for window in self.windows().await? {
            if window.snapshot().active()? == Some(true) {
                active.push(window);
            }
        }
        match active.len() {
            0 => Err(Error::ObjectNotFound {
                kind: ObjectKind::Window,
                target: format!("active window in {}", self.id),
            }),
            1 => active.into_iter().next().ok_or_else(|| Error::Decode {
                context: "active window",
                message: "single result disappeared".to_owned(),
            }),
            count => Err(Error::MultipleObjects {
                kind: ObjectKind::Window,
                count,
                query: format!("active windows in {}", self.id),
            }),
        }
    }

    /// Resolves one window ID within this session.
    pub async fn window(&self, id: WindowId) -> Result<Window> {
        self.windows()
            .await?
            .into_iter()
            .find(|window| window.id() == id)
            .ok_or_else(|| Error::ObjectNotFound {
                kind: ObjectKind::Window,
                target: format!("{id} in {}", self.id),
            })
    }

    /// Lists clients currently attached to this session.
    pub async fn clients(&self) -> Result<Vec<Client>> {
        let name = self.name().map(ToString::to_string);
        Ok(self
            .server
            .clients()
            .await?
            .into_iter()
            .filter(|client| client.session_name().map(ToString::to_string) == name)
            .collect())
    }

    /// Creates a window in this session.
    pub async fn new_window(&self, options: NewWindow) -> Result<Window> {
        Window::create(self, options).await
    }

    /// Creates a temporary window, runs an async operation, and attempts cleanup.
    pub async fn with_window<F, Fut, T>(&self, options: NewWindow, operation: F) -> Result<T>
    where
        F: FnOnce(Window) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let window = self.new_window(options).await?;
        let cleanup = window.clone();
        let outcome = operation(window).await;
        let cleanup_result = cleanup.kill().await;
        match (outcome, cleanup_result) {
            (Ok(value), Ok(())) => Ok(value),
            (Ok(_), Err(source)) => Err(Error::Cleanup {
                operation: "with_window",
                source: Box::new(source),
            }),
            (Err(source), _) => Err(source),
        }
    }

    /// Renames the session and refreshes the snapshot.
    pub async fn rename(&mut self, name: impl Into<String>) -> Result<()> {
        let name = SessionName::new(name)?;
        self.server
            .checked(
                "rename session",
                Command::new("rename-session")
                    .target(self.id)
                    .arg(name.to_string()),
            )
            .await?;
        self.refresh().await
    }

    /// Selects this session for the invoking client.
    pub async fn select(&self) -> Result<()> {
        self.server
            .checked(
                "select session",
                Command::new("switch-client").target(self.id),
            )
            .await
            .map(|_| ())
    }

    /// Locks clients attached to this session.
    pub async fn lock(&self) -> Result<()> {
        self.server
            .checked("lock session", Command::new("lock-session").target(self.id))
            .await
            .map(|_| ())
    }

    /// Kills one selected window in this session.
    pub async fn kill_window(&self, target: impl Into<WindowTarget>) -> Result<()> {
        self.server
            .checked(
                "kill session window",
                Command::new("kill-window").target(target.into()),
            )
            .await
            .map(|_| ())
    }

    /// Selects a window link in this session.
    pub async fn select_window(&self, window: impl Into<WindowTarget>) -> Result<()> {
        self.server
            .checked(
                "select window",
                Command::new("select-window").target(window.into()),
            )
            .await
            .map(|_| ())
    }

    /// Selects the next window.
    pub async fn next_window(&self, alert: bool) -> Result<()> {
        let command = if alert {
            Command::new("next-window").arg("-a")
        } else {
            Command::new("next-window")
        };
        self.server
            .checked("next window", command.target(self.id))
            .await
            .map(|_| ())
    }

    /// Selects the previous window.
    pub async fn previous_window(&self, alert: bool) -> Result<()> {
        let command = if alert {
            Command::new("previous-window").arg("-a")
        } else {
            Command::new("previous-window")
        };
        self.server
            .checked("previous window", command.target(self.id))
            .await
            .map(|_| ())
    }

    /// Selects the last active window.
    pub async fn last_window(&self) -> Result<()> {
        self.server
            .checked("last window", Command::new("last-window").target(self.id))
            .await
            .map(|_| ())
    }

    /// Renumbers this session's window links.
    pub async fn renumber_windows(&self) -> Result<()> {
        self.server
            .checked(
                "renumber windows",
                Command::new("move-window").arg("-r").target(self.id),
            )
            .await
            .map(|_| ())
    }

    /// Detaches every client attached to this session.
    pub async fn detach_clients(&self) -> Result<()> {
        self.server
            .checked(
                "detach session clients",
                Command::new("detach-client")
                    .arg("-s")
                    .arg(self.id.to_string()),
            )
            .await
            .map(|_| ())
    }

    /// Detaches all clients and runs a shell command for each detached client.
    pub async fn detach_clients_with_command(&self, shell_command: OsString) -> Result<()> {
        self.server
            .checked(
                "detach session clients",
                Command::new("detach-client")
                    .arg("-s")
                    .arg(self.id.to_string())
                    .arg("-E")
                    .sensitive_arg(shell_command),
            )
            .await
            .map(|_| ())
    }

    /// Applies extended `kill-session` flags.
    pub async fn kill_with(&self, all_except: bool, clear_alerts: bool, group: bool) -> Result<()> {
        if group {
            self.server
                .version()
                .await?
                .require("kill session group", crate::ReleaseVersion::new(3, 7, None))?;
        }
        let mut command = Command::new("kill-session").target(self.id);
        if all_except {
            command = command.arg("-a");
        }
        if clear_alerts {
            command = command.arg("-C");
        }
        if group {
            command = command.arg("-g");
        }
        self.server
            .checked("kill session", command)
            .await
            .map(|_| ())
    }

    /// Starts an interactive attachment.
    pub fn attach(&self, read_only: bool) -> Result<AttachedClient> {
        self.server.attach(self.id, read_only)
    }

    /// Destroys the session. Dropping the handle does not call this method.
    pub async fn kill(&self) -> Result<()> {
        self.server
            .checked("kill session", Command::new("kill-session").target(self.id))
            .await
            .map(|_| ())
    }

    /// Shows options at session scope.
    pub async fn options(&self) -> Result<OptionMap> {
        self.server
            .show_options(OptionScope::Session, Some(&self.id.to_string()))
            .await
    }

    /// Shows sparse options and hooks at session scope.
    pub async fn sparse_options(&self) -> Result<SparseOptionMap> {
        self.server
            .show_sparse_options(OptionScope::Session, Some(&self.id.to_string()))
            .await
    }

    /// Sets a session option.
    pub async fn set_option(&self, name: &str, value: &OptionValue, append: bool) -> Result<()> {
        self.server
            .set_option(
                OptionScope::Session,
                Some(&self.id.to_string()),
                name,
                value,
                append,
            )
            .await
    }

    /// Unsets a session option.
    pub async fn unset_option(&self, name: &str) -> Result<()> {
        self.server
            .unset_option(OptionScope::Session, Some(&self.id.to_string()), name)
            .await
    }

    /// Shows this session's environment. Removed entries have a `None` value.
    pub async fn environment(&self) -> Result<BTreeMap<String, Option<TmuxText>>> {
        let result = self
            .server
            .checked(
                "show session environment",
                Command::new("show-environment").target(self.id),
            )
            .await?;
        crate::server::parse_environment(result.stdout().as_bytes())
    }

    /// Returns one session environment entry.
    ///
    /// The outer option distinguishes an unknown name; the inner option is
    /// `None` when tmux records the name as explicitly removed.
    pub async fn environment_entry(&self, name: &str) -> Result<Option<Option<TmuxText>>> {
        crate::server::validate_name("environment name", name)?;
        Ok(self.environment().await?.remove(name))
    }

    /// Sets a session environment variable.
    pub async fn set_environment(&self, name: &str, value: impl Into<OsString>) -> Result<()> {
        self.set_environment_with(name, value, false, false).await
    }

    /// Sets a session environment variable with format expansion or hidden storage.
    pub async fn set_environment_with(
        &self,
        name: &str,
        value: impl Into<OsString>,
        expand_formats: bool,
        hidden: bool,
    ) -> Result<()> {
        crate::server::validate_name("environment name", name)?;
        let mut command = Command::new("set-environment").target(self.id);
        if expand_formats {
            command = command.arg("-F");
        }
        if hidden {
            command = command.arg("-h");
        }
        self.server
            .checked(
                "set session environment",
                command.arg(name).sensitive_arg(value),
            )
            .await
            .map(|_| ())
    }

    /// Unsets a session environment variable.
    pub async fn unset_environment(&self, name: &str) -> Result<()> {
        crate::server::validate_name("environment name", name)?;
        self.server
            .checked(
                "unset session environment",
                Command::new("set-environment")
                    .arg("-u")
                    .target(self.id)
                    .arg(name),
            )
            .await
            .map(|_| ())
    }

    /// Marks a session environment variable as removed for future child processes.
    pub async fn remove_environment(&self, name: &str) -> Result<()> {
        crate::server::validate_name("environment name", name)?;
        self.server
            .checked(
                "remove session environment",
                Command::new("set-environment")
                    .arg("-r")
                    .target(self.id)
                    .arg(name),
            )
            .await
            .map(|_| ())
    }

    /// Returns all distinct window IDs linked into this session.
    pub async fn window_ids(&self) -> Result<BTreeSet<WindowId>> {
        Ok(self
            .windows()
            .await?
            .into_iter()
            .map(|window| window.id())
            .collect())
    }
}

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.server.config() == other.server.config()
    }
}

impl Eq for Session {}
