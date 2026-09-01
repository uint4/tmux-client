//! Attached-client snapshots and interactive attachment handles.

use std::collections::BTreeMap;
use std::process::ExitStatus;

use tokio::process::Child;

use crate::{
    ClientName, ClientSnapshot, Command, CommandSummary, Error, ObjectKind, Pane, PaneId, Result,
    Server, Session, SessionName, SessionTarget, TmuxText, Window, WindowId,
};

/// A tmux client discovered with `list-clients`.
#[derive(Clone, Debug)]
pub struct Client {
    server: Server,
    name: ClientName,
    session_name: Option<SessionName>,
    snapshot: ClientSnapshot,
}

impl Client {
    pub(crate) fn from_row(server: Server, row: BTreeMap<&'static str, TmuxText>) -> Result<Self> {
        let snapshot = ClientSnapshot::new(row);
        let name = snapshot.name()?.ok_or_else(|| Error::Decode {
            context: "client row",
            message: "client_name was empty".to_owned(),
        })?;
        let session_name = snapshot.session_name()?;
        Ok(Self {
            server,
            name,
            session_name,
            snapshot,
        })
    }

    /// Returns the byte-preserving tmux client name.
    #[must_use]
    pub const fn name(&self) -> &ClientName {
        &self.name
    }

    /// Returns the attached session name captured by this snapshot.
    #[must_use]
    pub const fn session_name(&self) -> Option<&SessionName> {
        self.session_name.as_ref()
    }

    /// Returns the owned client snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &ClientSnapshot {
        &self.snapshot
    }

    /// Returns the server handle.
    #[must_use]
    pub const fn server(&self) -> &Server {
        &self.server
    }

    /// Replaces this object's state with a fresh client snapshot.
    pub async fn refresh(&mut self) -> Result<()> {
        let name = self.name.clone();
        *self = self
            .server
            .clients()
            .await?
            .into_iter()
            .find(|client| client.name == name)
            .ok_or_else(|| Error::ObjectNotFound {
                kind: ObjectKind::Client,
                target: name.to_string(),
            })?;
        Ok(())
    }

    /// Resolves the currently attached session, if any.
    pub async fn session(&self) -> Result<Option<Session>> {
        let Some(name) = self.session_name() else {
            return Ok(None);
        };
        self.server.session_by_name(name).await.map(Some)
    }

    /// Resolves the active window attached to this client.
    pub async fn window(&self) -> Result<Option<Window>> {
        let Some(id) = snapshot_id::<WindowId>(&self.snapshot, "window_id", "client window ID")?
        else {
            return Ok(None);
        };
        let Some(session) = self.session().await? else {
            return Ok(None);
        };
        session.window(id).await.map(Some)
    }

    /// Resolves the active pane attached to this client.
    pub async fn pane(&self) -> Result<Option<Pane>> {
        let Some(id) = snapshot_id::<PaneId>(&self.snapshot, "pane_id", "client pane ID")? else {
            return Ok(None);
        };
        self.server.pane(id).await.map(Some)
    }

    /// Runs an arbitrary tmux command targeted at this client.
    pub async fn cmd(&self, command: Command) -> Result<crate::CommandResult> {
        self.server
            .cmd(command.arg("-t").arg(self.name.to_os_string()))
            .await
    }

    /// Detaches this client.
    pub async fn detach(&self, hangup: bool) -> Result<()> {
        let mut command = Command::new("detach-client");
        if hangup {
            command = command.arg("-P");
        }
        self.server
            .checked(
                "detach client",
                command.arg("-t").arg(self.name.to_os_string()),
            )
            .await
            .map(|_| ())
    }

    /// Switches this client to another session.
    pub async fn switch_session(&self, target: impl Into<SessionTarget>) -> Result<()> {
        self.server
            .checked(
                "switch client session",
                Command::new("switch-client")
                    .arg("-c")
                    .arg(self.name.to_os_string())
                    .arg("-t")
                    .arg(target.into().to_string()),
            )
            .await
            .map(|_| ())
    }

    /// Suspends this client process.
    pub async fn suspend(&self) -> Result<()> {
        self.server
            .checked(
                "suspend client",
                Command::new("suspend-client")
                    .arg("-t")
                    .arg(self.name.to_os_string()),
            )
            .await
            .map(|_| ())
    }

    /// Locks this client.
    pub async fn lock(&self) -> Result<()> {
        self.server
            .checked(
                "lock client",
                Command::new("lock-client")
                    .arg("-t")
                    .arg(self.name.to_os_string()),
            )
            .await
            .map(|_| ())
    }

    /// Reports an updated terminal size for a control-mode client.
    pub async fn resize(&self, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            return Err(Error::InvalidArgument {
                argument: "client size",
                message: "width and height must be non-zero".to_owned(),
            });
        }
        self.server
            .checked(
                "resize client",
                Command::new("refresh-client")
                    .arg("-C")
                    .arg(format!("{width},{height}"))
                    .arg("-t")
                    .arg(self.name.to_os_string()),
            )
            .await
            .map(|_| ())
    }
}

impl PartialEq for Client {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.server.config() == other.server.config()
    }
}

impl Eq for Client {}

fn snapshot_id<T>(
    snapshot: &ClientSnapshot,
    token: &'static str,
    context: &'static str,
) -> Result<Option<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let Some(value) = snapshot.get(token) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    let value = value.to_str().map_err(|source| Error::Decode {
        context,
        message: source.to_string(),
    })?;
    value
        .parse::<T>()
        .map(Some)
        .map_err(|source| Error::Decode {
            context,
            message: source.to_string(),
        })
}

/// A running, interactive `attach-session` child process.
///
/// Dropping the handle terminates the spawned tmux client process because the
/// underlying Tokio child is configured with `kill_on_drop`. It never destroys
/// the attached session.
#[derive(Debug)]
pub struct AttachedClient {
    child: Child,
    summary: CommandSummary,
}

impl AttachedClient {
    pub(crate) const fn new(child: Child, summary: CommandSummary) -> Self {
        Self { child, summary }
    }

    /// Returns the child process ID when it is still available.
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Returns the redacted attachment command.
    #[must_use]
    pub const fn summary(&self) -> &CommandSummary {
        &self.summary
    }

    /// Waits for the interactive client to exit.
    pub async fn wait(mut self) -> Result<ExitStatus> {
        self.child.wait().await.map_err(|source| Error::Io {
            operation: "wait for attached client",
            source,
        })
    }

    /// Requests termination and waits for the client process.
    pub async fn terminate(&mut self) -> Result<ExitStatus> {
        self.child.kill().await.map_err(|source| Error::Io {
            operation: "terminate attached client",
            source,
        })?;
        self.child.wait().await.map_err(|source| Error::Io {
            operation: "wait for terminated attached client",
            source,
        })
    }
}
