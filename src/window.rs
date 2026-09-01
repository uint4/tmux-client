//! Window-link snapshots, layouts, and mutations.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::future::Future;
use std::path::PathBuf;

use crate::{
    Command, Error, NewPane, ObjectKind, OptionMap, OptionScope, OptionValue, Pane,
    ResizeDirection, Result, Server, Session, SessionId, SessionTarget, SparseOptionMap, SplitPane,
    TmuxText, WindowId, WindowLink, WindowSnapshot, WindowTarget,
};

/// A built-in tmux window layout or a caller-provided layout string.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Layout {
    /// `even-horizontal`.
    EvenHorizontal,
    /// `even-vertical`.
    EvenVertical,
    /// `main-horizontal`.
    MainHorizontal,
    /// `main-vertical`.
    MainVertical,
    /// `tiled`.
    Tiled,
    /// A serialized or custom layout name.
    Custom(String),
}

impl std::fmt::Display for Layout {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::EvenHorizontal => "even-horizontal",
            Self::EvenVertical => "even-vertical",
            Self::MainHorizontal => "main-horizontal",
            Self::MainVertical => "main-vertical",
            Self::Tiled => "tiled",
            Self::Custom(value) => value,
        })
    }
}

/// Direction used by `rotate-window`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Rotation {
    /// Rotate panes upward.
    Up,
    /// Rotate panes downward.
    Down,
}

/// Placement relative to a target window.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WindowPosition {
    /// Insert before the target (`-b`).
    Before,
    /// Insert after the target (`-a`).
    After,
}

/// Typed arguments for the three `resize-window` modes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WindowResize {
    adjustment: Option<(ResizeDirection, u32)>,
    width: Option<u32>,
    height: Option<u32>,
    expand: bool,
    shrink: bool,
}

impl WindowResize {
    /// Creates empty resize arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adjusts one edge by a non-zero number of cells.
    pub fn adjustment(mut self, direction: ResizeDirection, cells: u32) -> Result<Self> {
        if cells == 0 {
            return Err(Error::InvalidArgument {
                argument: "window resize adjustment",
                message: "must be non-zero".to_owned(),
            });
        }
        self.adjustment = Some((direction, cells));
        Ok(self)
    }

    /// Sets a non-zero absolute width.
    pub fn width(mut self, width: u32) -> Result<Self> {
        if width == 0 {
            return Err(Error::InvalidArgument {
                argument: "window width",
                message: "must be non-zero".to_owned(),
            });
        }
        self.width = Some(width);
        Ok(self)
    }

    /// Sets a non-zero absolute height.
    pub fn height(mut self, height: u32) -> Result<Self> {
        if height == 0 {
            return Err(Error::InvalidArgument {
                argument: "window height",
                message: "must be non-zero".to_owned(),
            });
        }
        self.height = Some(height);
        Ok(self)
    }

    /// Expands the window to the available size.
    #[must_use]
    pub const fn expand(mut self, enabled: bool) -> Self {
        self.expand = enabled;
        self
    }

    /// Shrinks the window to the smallest size.
    #[must_use]
    pub const fn shrink(mut self, enabled: bool) -> Self {
        self.shrink = enabled;
        self
    }
}

/// Typed arguments for `respawn-window`.
#[derive(Clone, Debug, Default)]
pub struct RespawnWindow {
    command: Option<OsString>,
    cwd: Option<PathBuf>,
    environment: BTreeMap<String, OsString>,
    kill: bool,
}

impl RespawnWindow {
    /// Creates default respawn arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the replacement command.
    #[must_use]
    pub fn command(mut self, command: impl Into<OsString>) -> Self {
        self.command = Some(command.into());
        self
    }

    /// Sets the starting directory.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Adds an environment entry.
    pub fn environment(
        mut self,
        name: impl Into<String>,
        value: impl Into<OsString>,
    ) -> Result<Self> {
        let name = name.into();
        crate::server::validate_name("environment name", &name)?;
        self.environment.insert(name, value.into());
        Ok(self)
    }

    /// Kills active processes before replacement.
    #[must_use]
    pub const fn kill(mut self, enabled: bool) -> Self {
        self.kill = enabled;
        self
    }
}

/// Typed arguments for creating a window.
#[derive(Clone, Debug)]
pub struct NewWindow {
    name: Option<OsString>,
    cwd: Option<PathBuf>,
    index: Option<u32>,
    detached: bool,
    kill_existing: bool,
    shell_command: Option<OsString>,
    environment: BTreeMap<String, OsString>,
    relative_to: Option<WindowTarget>,
    position: Option<WindowPosition>,
    select_existing: bool,
}

impl Default for NewWindow {
    fn default() -> Self {
        Self {
            name: None,
            cwd: None,
            index: None,
            detached: true,
            kill_existing: false,
            shell_command: None,
            environment: BTreeMap::new(),
            relative_to: None,
            position: None,
            select_existing: false,
        }
    }
}

impl NewWindow {
    /// Creates default arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the window name.
    #[must_use]
    pub fn name(mut self, name: impl Into<OsString>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the starting directory.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Places the new link at a specific index.
    #[must_use]
    pub const fn index(mut self, index: u32) -> Self {
        self.index = Some(index);
        self
    }

    /// Chooses whether the new window remains detached.
    #[must_use]
    pub const fn detached(mut self, detached: bool) -> Self {
        self.detached = detached;
        self
    }

    /// Replaces an existing link at the requested index.
    #[must_use]
    pub const fn kill_existing(mut self, enabled: bool) -> Self {
        self.kill_existing = enabled;
        self
    }

    /// Sets the command for the first pane.
    #[must_use]
    pub fn shell_command(mut self, command: impl Into<OsString>) -> Self {
        self.shell_command = Some(command.into());
        self
    }

    /// Adds a window environment entry. The value is redacted in diagnostics.
    pub fn environment(
        mut self,
        name: impl Into<String>,
        value: impl Into<OsString>,
    ) -> Result<Self> {
        let name = name.into();
        crate::server::validate_name("environment name", &name)?;
        self.environment.insert(name, value.into());
        Ok(self)
    }

    /// Places the new window relative to an existing target.
    #[must_use]
    pub fn relative_to(
        mut self,
        target: impl Into<WindowTarget>,
        position: WindowPosition,
    ) -> Self {
        self.relative_to = Some(target.into());
        self.position = Some(position);
        self
    }

    /// Selects an existing window with the requested name instead of creating it.
    #[must_use]
    pub const fn select_existing(mut self, enabled: bool) -> Self {
        self.select_existing = enabled;
        self
    }
}

/// A tmux window identity plus one session-specific link and owned snapshot.
#[derive(Clone, Debug)]
pub struct Window {
    server: Server,
    id: WindowId,
    session_id: SessionId,
    link: WindowLink,
    snapshot: WindowSnapshot,
}

impl Window {
    pub(crate) fn from_row(server: Server, row: BTreeMap<&'static str, TmuxText>) -> Result<Self> {
        let snapshot = WindowSnapshot::new(row);
        let id = snapshot.id()?.ok_or_else(|| Error::Decode {
            context: "window row",
            message: "window_id was empty".to_owned(),
        })?;
        let session_id = snapshot
            .get("session_id")
            .ok_or_else(|| Error::Decode {
                context: "window row",
                message: "session_id was absent".to_owned(),
            })?
            .to_str()
            .map_err(|source| Error::Decode {
                context: "window session ID",
                message: source.to_string(),
            })?
            .parse()?;
        let index = snapshot.index()?.ok_or_else(|| Error::Decode {
            context: "window row",
            message: "window_index was empty".to_owned(),
        })?;
        Ok(Self {
            server,
            id,
            session_id,
            link: WindowLink::new(session_id, index),
            snapshot,
        })
    }

    /// Resolves the window containing the pane named by `TMUX_PANE`.
    pub async fn from_environment() -> Result<Self> {
        Server::from_environment()?
            .pane_from_environment()
            .await?
            .window()
            .await
    }

    pub(crate) async fn create(session: &Session, options: NewWindow) -> Result<Self> {
        let target = options.relative_to.as_ref().map_or_else(
            || {
                options.index.map_or_else(
                    || session.id().to_string(),
                    |index| format!("{}:{index}", session.id()),
                )
            },
            ToString::to_string,
        );
        let mut command = Command::new("new-window")
            .arg("-P")
            .arg("-F")
            .arg("#{window_id}")
            .arg("-t")
            .arg(target);
        if options.detached {
            command = command.arg("-d");
        }
        if options.kill_existing {
            command = command.arg("-k");
        }
        if options.select_existing {
            command = command.arg("-S");
        }
        if let Some(position) = options.position {
            command = command.arg(match position {
                WindowPosition::Before => "-b",
                WindowPosition::After => "-a",
            });
        }
        if let Some(name) = options.name {
            command = command.arg("-n").arg(name);
        }
        if let Some(cwd) = options.cwd {
            command = command.arg("-c").arg(cwd);
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
        let result = session.server().checked("create window", command).await?;
        let id: WindowId =
            crate::server::single_line(result.stdout(), "new-window window ID")?.parse()?;
        session.window(id).await
    }

    /// Returns the immutable window ID.
    #[must_use]
    pub const fn id(&self) -> WindowId {
        self.id
    }

    /// Returns this session-specific window link.
    #[must_use]
    pub const fn link(&self) -> &WindowLink {
        &self.link
    }

    /// Returns the owning session ID.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Returns the owned snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &WindowSnapshot {
        &self.snapshot
    }

    /// Returns the server handle.
    #[must_use]
    pub const fn server(&self) -> &Server {
        &self.server
    }

    /// Refreshes this exact window link. A moved or unlinked row is stale.
    pub async fn refresh(&mut self) -> Result<()> {
        let link = self.link.clone();
        *self = self
            .server
            .windows()
            .await?
            .into_iter()
            .find(|window| window.link == link)
            .ok_or_else(|| Error::ObjectNotFound {
                kind: ObjectKind::Window,
                target: link.to_string(),
            })?;
        Ok(())
    }

    /// Resolves the parent session.
    pub async fn session(&self) -> Result<Session> {
        self.server.session(self.session_id()).await
    }

    /// Resolves every session containing a link to this window.
    pub async fn linked_sessions(&self) -> Result<Vec<Session>> {
        let mut ids = self
            .server
            .windows()
            .await?
            .into_iter()
            .filter(|window| window.id == self.id)
            .map(|window| window.session_id)
            .collect::<std::collections::BTreeSet<_>>();
        let mut sessions = Vec::with_capacity(ids.len());
        while let Some(id) = ids.pop_first() {
            sessions.push(self.server.session(id).await?);
        }
        Ok(sessions)
    }

    /// Lists panes in this window.
    pub async fn panes(&self) -> Result<Vec<Pane>> {
        Ok(self
            .server
            .panes()
            .await?
            .into_iter()
            .filter(|pane| pane.window_id() == self.id && pane.session_id() == self.session_id())
            .collect())
    }

    /// Lists panes, returning an empty collection on failure.
    pub async fn panes_or_empty(&self) -> Vec<Pane> {
        self.panes().await.unwrap_or_default()
    }

    /// Returns the active pane.
    pub async fn active_pane(&self) -> Result<Pane> {
        for pane in self.panes().await? {
            if pane.snapshot().active()? == Some(true) {
                return Ok(pane);
            }
        }
        Err(Error::ObjectNotFound {
            kind: ObjectKind::Pane,
            target: format!("active pane in {}", self.link),
        })
    }

    /// Runs an arbitrary tmux command targeted at this exact link.
    pub async fn cmd(&self, command: Command) -> Result<crate::CommandResult> {
        self.server.cmd(command.target(&self.link)).await
    }

    /// Splits this window's active pane.
    pub async fn split(&self, options: SplitPane) -> Result<Pane> {
        let pane = self.active_pane().await?;
        pane.split(options).await
    }

    /// Creates a floating pane in this window. Requires tmux 3.7+.
    pub async fn new_pane(&self, options: NewPane) -> Result<Pane> {
        self.active_pane().await?.new_pane(options).await
    }

    /// Creates a window before or after this link.
    pub async fn new_window(&self, options: NewWindow, position: WindowPosition) -> Result<Window> {
        self.session()
            .await?
            .new_window(options.relative_to(self.link.clone(), position))
            .await
    }

    /// Selects a pane in this window.
    pub async fn select_pane(&self, pane: crate::PaneId) -> Result<Pane> {
        self.server
            .checked(
                "select window pane",
                Command::new("select-pane").target(pane),
            )
            .await?;
        self.server.pane(pane).await
    }

    /// Selects the last active pane.
    pub async fn last_pane(&self) -> Result<Pane> {
        self.last_pane_with(None, false).await
    }

    /// Selects the last active pane with explicit input and zoom behavior.
    pub async fn last_pane_with(
        &self,
        input_enabled: Option<bool>,
        keep_zoom: bool,
    ) -> Result<Pane> {
        let mut command = Command::new("last-pane").target(&self.link);
        if let Some(enabled) = input_enabled {
            command = command.arg(if enabled { "-e" } else { "-d" });
        }
        if keep_zoom {
            command = command.arg("-Z");
        }
        self.server.checked("last pane", command).await?;
        self.active_pane().await
    }

    /// Creates a temporary pane, runs an async operation, and attempts cleanup.
    pub async fn with_pane<F, Fut, T>(&self, options: SplitPane, operation: F) -> Result<T>
    where
        F: FnOnce(Pane) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let pane = self.split(options).await?;
        let cleanup = pane.clone();
        let outcome = operation(pane).await;
        let cleanup_result = cleanup.kill().await;
        match (outcome, cleanup_result) {
            (Ok(value), Ok(())) => Ok(value),
            (Ok(_), Err(source)) => Err(Error::Cleanup {
                operation: "with_pane",
                source: Box::new(source),
            }),
            (Err(source), _) => Err(source),
        }
    }

    /// Selects this window link.
    pub async fn select(&self) -> Result<()> {
        self.server
            .checked(
                "select window",
                Command::new("select-window").target(&self.link),
            )
            .await
            .map(|_| ())
    }

    /// Renames the window and refreshes the snapshot.
    pub async fn rename(&mut self, name: impl Into<OsString>) -> Result<()> {
        self.server
            .checked(
                "rename window",
                Command::new("rename-window").target(&self.link).arg(name),
            )
            .await?;
        self.refresh().await
    }

    /// Applies a built-in or serialized layout.
    pub async fn select_layout(&mut self, layout: &Layout) -> Result<()> {
        self.server
            .checked(
                "select layout",
                Command::new("select-layout")
                    .target(&self.link)
                    .arg(layout.to_string()),
            )
            .await?;
        self.refresh().await
    }

    /// Advances to the next built-in layout and refreshes the snapshot.
    pub async fn next_layout(&mut self) -> Result<()> {
        self.server
            .checked(
                "next layout",
                Command::new("next-layout").target(&self.link),
            )
            .await?;
        self.refresh().await
    }

    /// Returns to the previous layout and refreshes the snapshot.
    pub async fn previous_layout(&mut self) -> Result<()> {
        self.server
            .checked(
                "previous layout",
                Command::new("previous-layout").target(&self.link),
            )
            .await?;
        self.refresh().await
    }

    /// Spreads panes evenly with `select-layout -E`.
    pub async fn spread_layout(&mut self) -> Result<()> {
        self.server
            .checked(
                "spread layout",
                Command::new("select-layout").arg("-E").target(&self.link),
            )
            .await?;
        self.refresh().await
    }

    /// Resizes the window to an absolute width and/or height.
    pub async fn resize(&mut self, width: Option<u32>, height: Option<u32>) -> Result<()> {
        if width.is_none() && height.is_none() {
            return Err(Error::InvalidArgument {
                argument: "window size",
                message: "width or height is required".to_owned(),
            });
        }
        if width == Some(0) || height == Some(0) {
            return Err(Error::InvalidArgument {
                argument: "window size",
                message: "dimensions must be non-zero".to_owned(),
            });
        }
        let mut command = Command::new("resize-window").target(&self.link);
        if let Some(width) = width {
            command = command.arg("-x").arg(width.to_string());
        }
        if let Some(height) = height {
            command = command.arg("-y").arg(height.to_string());
        }
        self.server.checked("resize window", command).await?;
        self.refresh().await
    }

    /// Applies typed adjustment, absolute, expand, or shrink resizing.
    pub async fn resize_with(&mut self, options: WindowResize) -> Result<()> {
        let modes = usize::from(options.adjustment.is_some())
            + usize::from(options.width.is_some() || options.height.is_some())
            + usize::from(options.expand)
            + usize::from(options.shrink);
        if modes != 1 {
            return Err(Error::InvalidArgument {
                argument: "window resize mode",
                message: "choose exactly one of adjustment, absolute size, expand, or shrink"
                    .to_owned(),
            });
        }
        let mut command = Command::new("resize-window").target(&self.link);
        if let Some((direction, cells)) = options.adjustment {
            command = command.arg(direction.flag()).arg(cells.to_string());
        }
        if let Some(width) = options.width {
            command = command.arg("-x").arg(width.to_string());
        }
        if let Some(height) = options.height {
            command = command.arg("-y").arg(height.to_string());
        }
        if options.expand {
            command = command.arg("-A");
        }
        if options.shrink {
            command = command.arg("-a");
        }
        self.server.checked("resize window", command).await?;
        self.refresh().await
    }

    /// Rotates panes in the window.
    pub async fn rotate(&self, direction: Rotation) -> Result<()> {
        let mut command = Command::new("rotate-window");
        if direction == Rotation::Down {
            command = command.arg("-D");
        }
        self.server
            .checked("rotate window", command.target(&self.link))
            .await
            .map(|_| ())
    }

    /// Rotates panes without unzooming the window.
    pub async fn rotate_keep_zoom(&self, direction: Rotation) -> Result<()> {
        let direction = match direction {
            Rotation::Up => "-U",
            Rotation::Down => "-D",
        };
        self.server
            .checked(
                "rotate window",
                Command::new("rotate-window")
                    .arg(direction)
                    .arg("-Z")
                    .target(&self.link),
            )
            .await
            .map(|_| ())
    }

    /// Swaps this window with another target.
    pub async fn swap_with(
        &self,
        destination: impl Into<WindowTarget>,
        detached: bool,
    ) -> Result<()> {
        let mut command = Command::new("swap-window")
            .arg("-s")
            .arg(self.link.to_string());
        if detached {
            command = command.arg("-d");
        }
        command = command.arg("-t").arg(destination.into().to_string());
        self.server
            .checked("swap window", command)
            .await
            .map(|_| ())
    }

    /// Links this window into another session and returns the new link snapshot.
    pub async fn link_to(
        &self,
        session: impl Into<SessionTarget>,
        index: Option<u32>,
        kill_existing: bool,
    ) -> Result<Window> {
        self.link_to_with(session, index, kill_existing, None, true)
            .await
    }

    /// Links this window with placement and selection flags.
    pub async fn link_to_with(
        &self,
        session: impl Into<SessionTarget>,
        index: Option<u32>,
        kill_existing: bool,
        position: Option<WindowPosition>,
        detached: bool,
    ) -> Result<Window> {
        let session = session.into();
        let destination_session_id = match &session {
            SessionTarget::Id(id) => *id,
            SessionTarget::Name(name) => self.server.session_by_name(name).await?.id(),
        };
        let destination =
            index.map_or_else(|| session.to_string(), |index| format!("{session}:{index}"));
        let mut command = Command::new("link-window")
            .arg("-s")
            .arg(self.link.to_string())
            .arg("-t")
            .arg(&destination);
        if kill_existing {
            command = command.arg("-k");
        }
        if let Some(position) = position {
            command = command.arg(match position {
                WindowPosition::Before => "-b",
                WindowPosition::After => "-a",
            });
        }
        if detached {
            command = command.arg("-d");
        }
        self.server.checked("link window", command).await?;
        self.server
            .windows()
            .await?
            .into_iter()
            .find(|window| {
                window.id == self.id
                    && window.session_id == destination_session_id
                    && index.is_none_or(|index| window.link.index() == index)
            })
            .ok_or(Error::ObjectNotFound {
                kind: ObjectKind::Window,
                target: destination,
            })
    }

    /// Moves this link to another destination.
    pub async fn move_to(
        &self,
        session: impl Into<SessionTarget>,
        index: Option<u32>,
        kill_existing: bool,
    ) -> Result<()> {
        self.move_to_with(session, index, kill_existing, None, true)
            .await
    }

    /// Moves this window with placement and selection flags.
    pub async fn move_to_with(
        &self,
        session: impl Into<SessionTarget>,
        index: Option<u32>,
        kill_existing: bool,
        position: Option<WindowPosition>,
        detached: bool,
    ) -> Result<()> {
        let session = session.into();
        let destination =
            index.map_or_else(|| session.to_string(), |index| format!("{session}:{index}"));
        let mut command = Command::new("move-window")
            .arg("-s")
            .arg(self.link.to_string())
            .arg("-t")
            .arg(destination);
        if kill_existing {
            command = command.arg("-k");
        }
        if let Some(position) = position {
            command = command.arg(match position {
                WindowPosition::Before => "-b",
                WindowPosition::After => "-a",
            });
        }
        if detached {
            command = command.arg("-d");
        }
        self.server
            .checked("move window", command)
            .await
            .map(|_| ())
    }

    /// Unlinks this window from its session.
    pub async fn unlink(&self, kill_if_last: bool) -> Result<()> {
        let mut command = Command::new("unlink-window");
        if kill_if_last {
            command = command.arg("-k");
        }
        self.server
            .checked("unlink window", command.target(&self.link))
            .await
            .map(|_| ())
    }

    /// Respawns the active pane for this window.
    pub async fn respawn(
        &self,
        command: Option<OsString>,
        cwd: Option<PathBuf>,
        kill: bool,
    ) -> Result<()> {
        let mut tmux = Command::new("respawn-window").target(&self.link);
        if kill {
            tmux = tmux.arg("-k");
        }
        if let Some(cwd) = cwd {
            tmux = tmux.arg("-c").arg(cwd);
        }
        if let Some(command) = command {
            tmux = tmux.sensitive_arg(command);
        }
        self.server
            .checked("respawn window", tmux)
            .await
            .map(|_| ())
    }

    /// Respawns a window with typed environment and process arguments.
    pub async fn respawn_with(&self, options: RespawnWindow) -> Result<()> {
        let mut command = Command::new("respawn-window").target(&self.link);
        if options.kill {
            command = command.arg("-k");
        }
        if let Some(cwd) = options.cwd {
            command = command.arg("-c").arg(cwd);
        }
        for (name, value) in options.environment {
            command =
                command
                    .arg("-e")
                    .sensitive_arg(format!("{}={}", name, value.to_string_lossy()));
        }
        if let Some(shell_command) = options.command {
            command = command.sensitive_arg(shell_command);
        }
        self.server
            .checked("respawn window", command)
            .await
            .map(|_| ())
    }

    /// Destroys the underlying window and all of its links.
    pub async fn kill(&self) -> Result<()> {
        self.server
            .checked("kill window", Command::new("kill-window").target(self.id))
            .await
            .map(|_| ())
    }

    /// Kills every other window in this session.
    pub async fn kill_others(&self) -> Result<()> {
        self.server
            .checked(
                "kill other windows",
                Command::new("kill-window").arg("-a").target(&self.link),
            )
            .await
            .map(|_| ())
    }

    /// Shows window-scoped options.
    pub async fn options(&self) -> Result<OptionMap> {
        self.server
            .show_options(OptionScope::Window, Some(&self.link.to_string()))
            .await
    }

    /// Shows sparse window options and hooks.
    pub async fn sparse_options(&self) -> Result<SparseOptionMap> {
        self.server
            .show_sparse_options(OptionScope::Window, Some(&self.link.to_string()))
            .await
    }

    /// Sets a window option.
    pub async fn set_option(&self, name: &str, value: &OptionValue, append: bool) -> Result<()> {
        self.server
            .set_option(
                OptionScope::Window,
                Some(&self.link.to_string()),
                name,
                value,
                append,
            )
            .await
    }

    /// Unsets a window option.
    pub async fn unset_option(&self, name: &str) -> Result<()> {
        self.server
            .unset_option(OptionScope::Window, Some(&self.link.to_string()), name)
            .await
    }
}

impl PartialEq for Window {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.link == other.link
            && self.server.config() == other.server.config()
    }
}

impl Eq for Window {}
