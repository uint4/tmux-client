//! Pane snapshots, capture, input, splitting, resizing, and process operations.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::{
    BufferName, ClientName, Command, Error, ObjectKind, OptionMap, OptionScope, OptionValue,
    PaneId, PaneSnapshot, ReleaseVersion, Result, Server, Session, SessionId, SparseOptionMap,
    TmuxText, Window, WindowId, WindowTarget,
};

/// Orientation for a new pane.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SplitDirection {
    /// Stack panes from top to bottom (tmux default).
    #[default]
    Vertical,
    /// Place panes side by side (`-h`).
    Horizontal,
}

/// Initial pane size.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PaneSize {
    /// Exact cells in the split direction.
    Cells(u32),
    /// A percentage from 1 through 100.
    Percent(u8),
}

/// Typed arguments for `split-window`.
#[derive(Clone, Debug)]
pub struct SplitPane {
    direction: SplitDirection,
    size: Option<PaneSize>,
    detached: bool,
    before: bool,
    full_window: bool,
    cwd: Option<PathBuf>,
    shell_command: Option<OsString>,
    environment: BTreeMap<String, OsString>,
}

/// Typed arguments for a tmux 3.7+ floating `new-pane`.
#[derive(Clone, Debug, Default)]
pub struct NewPane {
    cwd: Option<PathBuf>,
    attach: bool,
    shell_command: Option<OsString>,
    environment: BTreeMap<String, OsString>,
    width: Option<u32>,
    height: Option<u32>,
    x: Option<i32>,
    y: Option<i32>,
    zoom: bool,
    empty: bool,
    style: Option<OsString>,
    active_border_style: Option<OsString>,
    inactive_border_style: Option<OsString>,
    message: Option<OsString>,
    keep: bool,
}

impl NewPane {
    /// Creates default floating-pane arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the starting directory.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Activates the new pane instead of detaching.
    #[must_use]
    pub const fn attach(mut self, enabled: bool) -> Self {
        self.attach = enabled;
        self
    }

    /// Sets the pane shell command.
    #[must_use]
    pub fn shell_command(mut self, command: impl Into<OsString>) -> Self {
        self.shell_command = Some(command.into());
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

    /// Sets a non-zero width in cells.
    pub fn width(mut self, width: u32) -> Result<Self> {
        if width == 0 {
            return Err(Error::InvalidArgument {
                argument: "floating pane width",
                message: "must be non-zero".to_owned(),
            });
        }
        self.width = Some(width);
        Ok(self)
    }

    /// Sets a non-zero height in cells.
    pub fn height(mut self, height: u32) -> Result<Self> {
        if height == 0 {
            return Err(Error::InvalidArgument {
                argument: "floating pane height",
                message: "must be non-zero".to_owned(),
            });
        }
        self.height = Some(height);
        Ok(self)
    }

    /// Sets the x coordinate.
    #[must_use]
    pub const fn x(mut self, x: i32) -> Self {
        self.x = Some(x);
        self
    }

    /// Sets the y coordinate.
    #[must_use]
    pub const fn y(mut self, y: i32) -> Self {
        self.y = Some(y);
        self
    }

    /// Zooms the new pane.
    #[must_use]
    pub const fn zoom(mut self, enabled: bool) -> Self {
        self.zoom = enabled;
        self
    }

    /// Creates an empty pane without a process.
    #[must_use]
    pub const fn empty(mut self, enabled: bool) -> Self {
        self.empty = enabled;
        self
    }

    /// Sets pane style.
    #[must_use]
    pub fn style(mut self, value: impl Into<OsString>) -> Self {
        self.style = Some(value.into());
        self
    }

    /// Sets active border style.
    #[must_use]
    pub fn active_border_style(mut self, value: impl Into<OsString>) -> Self {
        self.active_border_style = Some(value.into());
        self
    }

    /// Sets inactive border style.
    #[must_use]
    pub fn inactive_border_style(mut self, value: impl Into<OsString>) -> Self {
        self.inactive_border_style = Some(value.into());
        self
    }

    /// Sets the remain-on-exit message.
    #[must_use]
    pub fn message(mut self, value: impl Into<OsString>) -> Self {
        self.message = Some(value.into());
        self
    }

    /// Keeps the pane after its command exits.
    #[must_use]
    pub const fn keep(mut self, enabled: bool) -> Self {
        self.keep = enabled;
        self
    }
}

impl Default for SplitPane {
    fn default() -> Self {
        Self {
            direction: SplitDirection::Vertical,
            size: None,
            detached: true,
            before: false,
            full_window: false,
            cwd: None,
            shell_command: None,
            environment: BTreeMap::new(),
        }
    }
}

impl SplitPane {
    /// Creates detached split arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the split orientation.
    #[must_use]
    pub const fn direction(mut self, direction: SplitDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Sets an exact cell size.
    pub fn cells(mut self, cells: u32) -> Result<Self> {
        if cells == 0 {
            return Err(Error::InvalidArgument {
                argument: "pane size",
                message: "cell count must be non-zero".to_owned(),
            });
        }
        self.size = Some(PaneSize::Cells(cells));
        Ok(self)
    }

    /// Sets a percentage size.
    pub fn percent(mut self, percent: u8) -> Result<Self> {
        if !(1..=100).contains(&percent) {
            return Err(Error::InvalidArgument {
                argument: "pane percentage",
                message: "must be between 1 and 100".to_owned(),
            });
        }
        self.size = Some(PaneSize::Percent(percent));
        Ok(self)
    }

    /// Chooses whether the new pane remains detached.
    #[must_use]
    pub const fn detached(mut self, detached: bool) -> Self {
        self.detached = detached;
        self
    }

    /// Inserts the new pane before the target.
    #[must_use]
    pub const fn before(mut self, before: bool) -> Self {
        self.before = before;
        self
    }

    /// Splits across the full window instead of only the target pane.
    #[must_use]
    pub const fn full_window(mut self, full_window: bool) -> Self {
        self.full_window = full_window;
        self
    }

    /// Sets the starting directory.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Sets the shell command for the new pane.
    #[must_use]
    pub fn shell_command(mut self, command: impl Into<OsString>) -> Self {
        self.shell_command = Some(command.into());
        self
    }

    /// Adds a pane environment variable. Values are redacted in diagnostics.
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
}

/// Direction for relative pane resizing.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResizeDirection {
    /// Resize the left edge.
    Left,
    /// Resize the right edge.
    Right,
    /// Resize the top edge.
    Up,
    /// Resize the bottom edge.
    Down,
}

/// A pane or window that can receive a moved or joined pane.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PaneDestination {
    /// Position relative to a target pane.
    Pane(PaneId),
    /// Position in a target window.
    Window(WindowTarget),
}

impl std::fmt::Display for PaneDestination {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pane(id) => id.fmt(formatter),
            Self::Window(target) => target.fmt(formatter),
        }
    }
}

impl From<PaneId> for PaneDestination {
    fn from(value: PaneId) -> Self {
        Self::Pane(value)
    }
}

impl From<WindowTarget> for PaneDestination {
    fn from(value: WindowTarget) -> Self {
        Self::Window(value)
    }
}

impl From<WindowId> for PaneDestination {
    fn from(value: WindowId) -> Self {
        Self::Window(value.into())
    }
}

/// Typed placement arguments shared by `move-pane` and `join-pane`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelocatePane {
    destination: PaneDestination,
    direction: SplitDirection,
    detached: bool,
    full_window: bool,
    size: Option<PaneSize>,
    before: bool,
}

/// Relative direction for `swap-pane`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SwapPaneDirection {
    /// Swap with the previous pane (`-U`).
    Up,
    /// Swap with the next pane (`-D`).
    Down,
}

/// Typed arguments for `swap-pane`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SwapPane {
    destination: Option<PaneId>,
    direction: Option<SwapPaneDirection>,
    detached: bool,
    keep_zoom: bool,
}

impl SwapPane {
    /// Creates a default swap request.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Selects an explicit destination pane.
    #[must_use]
    pub const fn destination(mut self, pane: PaneId) -> Self {
        self.destination = Some(pane);
        self
    }

    /// Selects a relative pane.
    #[must_use]
    pub const fn direction(mut self, direction: SwapPaneDirection) -> Self {
        self.direction = Some(direction);
        self
    }

    /// Prevents selection changes.
    #[must_use]
    pub const fn detached(mut self, enabled: bool) -> Self {
        self.detached = enabled;
        self
    }

    /// Preserves zoom.
    #[must_use]
    pub const fn keep_zoom(mut self, enabled: bool) -> Self {
        self.keep_zoom = enabled;
        self
    }
}

impl RelocatePane {
    /// Creates default detached, vertical placement arguments.
    #[must_use]
    pub fn new(destination: impl Into<PaneDestination>) -> Self {
        Self {
            destination: destination.into(),
            direction: SplitDirection::Vertical,
            detached: true,
            full_window: false,
            size: None,
            before: false,
        }
    }

    /// Sets the split direction.
    #[must_use]
    pub const fn direction(mut self, direction: SplitDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Prevents tmux from selecting the destination window.
    #[must_use]
    pub const fn detached(mut self, detached: bool) -> Self {
        self.detached = detached;
        self
    }

    /// Splits across the full destination window.
    #[must_use]
    pub const fn full_window(mut self, enabled: bool) -> Self {
        self.full_window = enabled;
        self
    }

    /// Sets an exact cell size.
    pub fn cells(mut self, cells: u32) -> Result<Self> {
        if cells == 0 {
            return Err(Error::InvalidArgument {
                argument: "relocated pane size",
                message: "cell count must be non-zero".to_owned(),
            });
        }
        self.size = Some(PaneSize::Cells(cells));
        Ok(self)
    }

    /// Sets a percentage size.
    pub fn percent(mut self, percent: u8) -> Result<Self> {
        if !(1..=100).contains(&percent) {
            return Err(Error::InvalidArgument {
                argument: "relocated pane percentage",
                message: "must be between 1 and 100".to_owned(),
            });
        }
        self.size = Some(PaneSize::Percent(percent));
        Ok(self)
    }

    /// Places this pane before the destination.
    #[must_use]
    pub const fn before(mut self, enabled: bool) -> Self {
        self.before = enabled;
        self
    }
}

impl ResizeDirection {
    pub(crate) fn flag(self) -> &'static str {
        match self {
            Self::Left => "-L",
            Self::Right => "-R",
            Self::Up => "-U",
            Self::Down => "-D",
        }
    }
}

/// Typed arguments for byte-preserving pane capture.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapturePane {
    start: Option<CaptureLine>,
    end: Option<CaptureLine>,
    alternate: bool,
    escape_sequences: bool,
    escape_non_printable: bool,
    join_wrapped: bool,
    preserve_trailing: bool,
    trim_trailing: bool,
    quiet: bool,
    mode_screen: bool,
    pending: bool,
    hyperlinks: bool,
    line_numbers: bool,
    line_flags: bool,
}

/// A numbered capture line or tmux's `-` history/screen boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CaptureLine {
    /// A visible-screen or history line number.
    Number(i64),
    /// The beginning of history for `-S`, or end of the visible screen for `-E`.
    Boundary,
}

impl std::fmt::Display for CaptureLine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(line) => line.fmt(formatter),
            Self::Boundary => formatter.write_str("-"),
        }
    }
}

impl CapturePane {
    /// Creates capture arguments for the visible pane.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the first history line. Negative values address history.
    #[must_use]
    pub const fn start(mut self, line: i64) -> Self {
        self.start = Some(CaptureLine::Number(line));
        self
    }

    /// Starts at the beginning of pane history.
    #[must_use]
    pub const fn start_of_history(mut self) -> Self {
        self.start = Some(CaptureLine::Boundary);
        self
    }

    /// Sets the last history line.
    #[must_use]
    pub const fn end(mut self, line: i64) -> Self {
        self.end = Some(CaptureLine::Number(line));
        self
    }

    /// Ends at the bottom of the visible pane.
    #[must_use]
    pub const fn end_of_screen(mut self) -> Self {
        self.end = Some(CaptureLine::Boundary);
        self
    }

    /// Captures the alternate screen.
    #[must_use]
    pub const fn alternate(mut self, enabled: bool) -> Self {
        self.alternate = enabled;
        self
    }

    /// Includes terminal escape sequences.
    #[must_use]
    pub const fn escape_sequences(mut self, enabled: bool) -> Self {
        self.escape_sequences = enabled;
        self
    }

    /// Escapes non-printable bytes using tmux's octal representation (`-C`).
    #[must_use]
    pub const fn escape_non_printable(mut self, enabled: bool) -> Self {
        self.escape_non_printable = enabled;
        self
    }

    /// Joins wrapped lines.
    #[must_use]
    pub const fn join_wrapped(mut self, enabled: bool) -> Self {
        self.join_wrapped = enabled;
        self
    }

    /// Preserves trailing spaces.
    #[must_use]
    pub const fn preserve_trailing(mut self, enabled: bool) -> Self {
        self.preserve_trailing = enabled;
        self
    }

    /// Trims unused trailing cells (`-T`). Requires tmux 3.4+.
    #[must_use]
    pub const fn trim_trailing(mut self, enabled: bool) -> Self {
        self.trim_trailing = enabled;
        self
    }

    /// Suppresses capture errors (`-q`).
    #[must_use]
    pub const fn quiet(mut self, enabled: bool) -> Self {
        self.quiet = enabled;
        self
    }

    /// Captures the active mode screen (`-M`). Requires tmux 3.6+.
    #[must_use]
    pub const fn mode_screen(mut self, enabled: bool) -> Self {
        self.mode_screen = enabled;
        self
    }

    /// Includes pending output.
    #[must_use]
    pub const fn pending(mut self, enabled: bool) -> Self {
        self.pending = enabled;
        self
    }

    /// Captures hyperlink targets (`-H`). Requires tmux 3.7+.
    #[must_use]
    pub const fn hyperlinks(mut self, enabled: bool) -> Self {
        self.hyperlinks = enabled;
        self
    }

    /// Prefixes captured lines with line numbers (`-L`). Requires tmux 3.7+.
    #[must_use]
    pub const fn line_numbers(mut self, enabled: bool) -> Self {
        self.line_numbers = enabled;
        self
    }

    /// Prefixes captured lines with line flags (`-F`). Requires tmux 3.7+.
    #[must_use]
    pub const fn line_flags(mut self, enabled: bool) -> Self {
        self.line_flags = enabled;
        self
    }
}

/// Typed arguments for `send-keys`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SendKeys {
    keys: Vec<OsString>,
    literal: bool,
    hexadecimal: bool,
    reset: bool,
    repeat: Option<u32>,
    enter: bool,
    suppress_history: bool,
    copy_mode_command: Option<OsString>,
    expand_formats: bool,
    target_client: Option<ClientName>,
    key_name: bool,
    sensitive: bool,
}

/// Typed arguments for `select-pane`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectPane {
    direction: Option<ResizeDirection>,
    last: bool,
    keep_zoom: bool,
    mark: bool,
    clear_mark: bool,
    input: Option<bool>,
}

impl SelectPane {
    /// Creates a direct pane-selection request.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Selects the neighboring pane in a direction.
    #[must_use]
    pub const fn direction(mut self, direction: ResizeDirection) -> Self {
        self.direction = Some(direction);
        self
    }

    /// Selects the previously active pane.
    #[must_use]
    pub const fn last(mut self, enabled: bool) -> Self {
        self.last = enabled;
        self
    }

    /// Preserves the window's zoom state.
    #[must_use]
    pub const fn keep_zoom(mut self, enabled: bool) -> Self {
        self.keep_zoom = enabled;
        self
    }

    /// Marks the selected pane.
    #[must_use]
    pub const fn mark(mut self, enabled: bool) -> Self {
        self.mark = enabled;
        self
    }

    /// Clears the marked pane.
    #[must_use]
    pub const fn clear_mark(mut self, enabled: bool) -> Self {
        self.clear_mark = enabled;
        self
    }

    /// Enables (`Some(true)`) or disables (`Some(false)`) pane input.
    #[must_use]
    pub const fn input(mut self, enabled: Option<bool>) -> Self {
        self.input = enabled;
        self
    }
}

/// Typed arguments for `paste-buffer`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PasteBuffer {
    name: Option<BufferName>,
    delete_after: bool,
    linefeed_separator: bool,
    bracketed: bool,
    separator: Option<OsString>,
    raw: bool,
}

impl PasteBuffer {
    /// Creates arguments for tmux's most recent buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Selects a named buffer.
    #[must_use]
    pub fn name(mut self, name: BufferName) -> Self {
        self.name = Some(name);
        self
    }

    /// Deletes the buffer after pasting.
    #[must_use]
    pub const fn delete_after(mut self, enabled: bool) -> Self {
        self.delete_after = enabled;
        self
    }

    /// Uses line feed rather than carriage return between lines.
    #[must_use]
    pub const fn linefeed_separator(mut self, enabled: bool) -> Self {
        self.linefeed_separator = enabled;
        self
    }

    /// Uses bracketed paste mode.
    #[must_use]
    pub const fn bracketed(mut self, enabled: bool) -> Self {
        self.bracketed = enabled;
        self
    }

    /// Sets an explicit line separator.
    #[must_use]
    pub fn separator(mut self, separator: impl Into<OsString>) -> Self {
        self.separator = Some(separator.into());
        self
    }

    /// Disables tmux `vis(3)` escaping. Requires tmux 3.7+.
    #[must_use]
    pub const fn raw(mut self, enabled: bool) -> Self {
        self.raw = enabled;
        self
    }
}

/// Typed arguments for `pipe-pane`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PipePane {
    command: Option<OsString>,
    output_only: bool,
    input_only: bool,
    toggle: bool,
}

impl PipePane {
    /// Creates a request that disables an existing pipe.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the shell command. It is redacted in diagnostics.
    #[must_use]
    pub fn command(mut self, command: impl Into<OsString>) -> Self {
        self.command = Some(command.into());
        self
    }

    /// Pipes pane output only (`-O`).
    #[must_use]
    pub const fn output_only(mut self, enabled: bool) -> Self {
        self.output_only = enabled;
        self
    }

    /// Pipes pane input only (`-I`).
    #[must_use]
    pub const fn input_only(mut self, enabled: bool) -> Self {
        self.input_only = enabled;
        self
    }

    /// Toggles the pipe (`-o`).
    #[must_use]
    pub const fn toggle(mut self, enabled: bool) -> Self {
        self.toggle = enabled;
        self
    }
}

/// Typed arguments for `copy-mode`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CopyMode {
    scroll_up: bool,
    exit_on_bottom: bool,
    mouse_drag: bool,
    cancel: bool,
    page_down: bool,
    source: Option<PaneId>,
}

impl CopyMode {
    /// Creates default copy-mode arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts one page up.
    #[must_use]
    pub const fn scroll_up(mut self, enabled: bool) -> Self {
        self.scroll_up = enabled;
        self
    }

    /// Exits when scrolling reaches the bottom.
    #[must_use]
    pub const fn exit_on_bottom(mut self, enabled: bool) -> Self {
        self.exit_on_bottom = enabled;
        self
    }

    /// Starts mouse drag.
    #[must_use]
    pub const fn mouse_drag(mut self, enabled: bool) -> Self {
        self.mouse_drag = enabled;
        self
    }

    /// Cancels active modes.
    #[must_use]
    pub const fn cancel(mut self, enabled: bool) -> Self {
        self.cancel = enabled;
        self
    }

    /// Scrolls a page down. Requires tmux 3.5+.
    #[must_use]
    pub const fn page_down(mut self, enabled: bool) -> Self {
        self.page_down = enabled;
        self
    }

    /// Displays another pane's history.
    #[must_use]
    pub const fn source(mut self, source: PaneId) -> Self {
        self.source = Some(source);
        self
    }
}

/// Sort field used by the tree chooser.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TreeSort {
    /// Window/session index.
    Index,
    /// Name.
    Name,
    /// Activity time.
    Time,
    /// Size.
    Size,
}

impl TreeSort {
    fn as_str(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Name => "name",
            Self::Time => "time",
            Self::Size => "size",
        }
    }
}

/// Typed arguments for `choose-tree`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChooseTree {
    sessions_collapsed: bool,
    windows_collapsed: bool,
    format: Option<OsString>,
    native_filter: Option<OsString>,
    sort: Option<TreeSort>,
    reverse: bool,
    zoom: bool,
}

impl ChooseTree {
    /// Creates default chooser arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts with sessions collapsed.
    #[must_use]
    pub const fn sessions_collapsed(mut self, enabled: bool) -> Self {
        self.sessions_collapsed = enabled;
        self
    }

    /// Starts with windows collapsed.
    #[must_use]
    pub const fn windows_collapsed(mut self, enabled: bool) -> Self {
        self.windows_collapsed = enabled;
        self
    }

    /// Sets the row format.
    #[must_use]
    pub fn format(mut self, value: impl Into<OsString>) -> Self {
        self.format = Some(value.into());
        self
    }

    /// Sets a raw tmux item filter.
    #[must_use]
    pub fn native_filter(mut self, value: impl Into<OsString>) -> Self {
        self.native_filter = Some(value.into());
        self
    }

    /// Sets the sort field.
    #[must_use]
    pub const fn sort(mut self, sort: TreeSort) -> Self {
        self.sort = Some(sort);
        self
    }

    /// Reverses sorting.
    #[must_use]
    pub const fn reverse(mut self, enabled: bool) -> Self {
        self.reverse = enabled;
        self
    }

    /// Zooms the pane while choosing.
    #[must_use]
    pub const fn zoom(mut self, enabled: bool) -> Self {
        self.zoom = enabled;
        self
    }
}

/// Typed arguments for `find-window`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FindWindow {
    pattern: OsString,
    content: bool,
    case_insensitive: bool,
    name_only: bool,
    regex: bool,
    title: bool,
}

impl FindWindow {
    /// Creates a search request.
    #[must_use]
    pub fn new(pattern: impl Into<OsString>) -> Self {
        Self {
            pattern: pattern.into(),
            content: false,
            case_insensitive: false,
            name_only: false,
            regex: false,
            title: false,
        }
    }

    /// Searches visible pane content.
    #[must_use]
    pub const fn content(mut self, enabled: bool) -> Self {
        self.content = enabled;
        self
    }

    /// Uses case-insensitive matching.
    #[must_use]
    pub const fn case_insensitive(mut self, enabled: bool) -> Self {
        self.case_insensitive = enabled;
        self
    }

    /// Matches window names only.
    #[must_use]
    pub const fn name_only(mut self, enabled: bool) -> Self {
        self.name_only = enabled;
        self
    }

    /// Treats the pattern as a regular expression.
    #[must_use]
    pub const fn regex(mut self, enabled: bool) -> Self {
        self.regex = enabled;
        self
    }

    /// Matches pane titles.
    #[must_use]
    pub const fn title(mut self, enabled: bool) -> Self {
        self.title = enabled;
        self
    }
}

/// Typed arguments for `respawn-pane`.
#[derive(Clone, Debug, Default)]
pub struct RespawnPane {
    shell_command: Option<OsString>,
    cwd: Option<PathBuf>,
    environment: BTreeMap<String, OsString>,
    kill: bool,
}

impl RespawnPane {
    /// Creates default respawn arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the replacement shell command.
    #[must_use]
    pub fn shell_command(mut self, command: impl Into<OsString>) -> Self {
        self.shell_command = Some(command.into());
        self
    }

    /// Sets the replacement working directory.
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

    /// Kills a still-running process before replacement.
    #[must_use]
    pub const fn kill(mut self, enabled: bool) -> Self {
        self.kill = enabled;
        self
    }
}

impl SendKeys {
    /// Creates an empty key sequence.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            keys: Vec::new(),
            literal: false,
            hexadecimal: false,
            reset: false,
            repeat: None,
            enter: false,
            suppress_history: false,
            copy_mode_command: None,
            expand_formats: false,
            target_client: None,
            key_name: false,
            sensitive: false,
        }
    }

    /// Adds one key name or literal string.
    #[must_use]
    pub fn key(mut self, key: impl Into<OsString>) -> Self {
        self.keys.push(key.into());
        self
    }

    /// Adds multiple key names.
    #[must_use]
    pub fn keys<I, S>(mut self, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.keys.extend(keys.into_iter().map(Into::into));
        self
    }

    /// Sends input literally (`-l`).
    #[must_use]
    pub const fn literal(mut self, enabled: bool) -> Self {
        self.literal = enabled;
        self
    }

    /// Treats each key as hexadecimal (`-H`).
    #[must_use]
    pub const fn hexadecimal(mut self, enabled: bool) -> Self {
        self.hexadecimal = enabled;
        self
    }

    /// Resets terminal state before sending input (`-R`).
    #[must_use]
    pub const fn reset(mut self, enabled: bool) -> Self {
        self.reset = enabled;
        self
    }

    /// Repeats the sequence a non-zero number of times.
    pub fn repeat(mut self, count: u32) -> Result<Self> {
        if count == 0 {
            return Err(Error::InvalidArgument {
                argument: "send-keys repeat",
                message: "must be non-zero".to_owned(),
            });
        }
        self.repeat = Some(count);
        Ok(self)
    }

    /// Appends an Enter key after the input sequence.
    #[must_use]
    pub const fn enter(mut self, enabled: bool) -> Self {
        self.enter = enabled;
        self
    }

    /// Sends a leading space before input so shells configured accordingly omit it from history.
    #[must_use]
    pub const fn suppress_history(mut self, enabled: bool) -> Self {
        self.suppress_history = enabled;
        self
    }

    /// Sends a command to copy mode (`-X`) instead of ordinary keys.
    #[must_use]
    pub fn copy_mode_command(mut self, command: impl Into<OsString>) -> Self {
        self.copy_mode_command = Some(command.into());
        self
    }

    /// Expands tmux formats in the supplied keys (`-F`).
    #[must_use]
    pub const fn expand_formats(mut self, enabled: bool) -> Self {
        self.expand_formats = enabled;
        self
    }

    /// Targets an attached client (`-c`). Requires tmux 3.4+.
    #[must_use]
    pub fn target_client(mut self, client: ClientName) -> Self {
        self.target_client = Some(client);
        self
    }

    /// Interprets input as key names for a target client (`-K`). Requires tmux 3.4+.
    #[must_use]
    pub const fn key_name(mut self, enabled: bool) -> Self {
        self.key_name = enabled;
        self
    }

    /// Redacts input keys or the copy-mode command from command diagnostics.
    #[must_use]
    pub const fn sensitive(mut self, enabled: bool) -> Self {
        self.sensitive = enabled;
        self
    }
}

/// A tmux pane handle with its parent identities and owned snapshot.
#[derive(Clone, Debug)]
pub struct Pane {
    server: Server,
    id: PaneId,
    session_id: SessionId,
    window_id: WindowId,
    snapshot: PaneSnapshot,
}

impl Pane {
    pub(crate) fn from_row(server: Server, row: BTreeMap<&'static str, TmuxText>) -> Result<Self> {
        let snapshot = PaneSnapshot::new(row);
        let id = snapshot.id()?.ok_or_else(|| Error::Decode {
            context: "pane row",
            message: "pane_id was empty".to_owned(),
        })?;
        let session_id = parse_parent_id(&snapshot, "session_id", "pane session ID")?;
        let window_id = parse_parent_id(&snapshot, "window_id", "pane window ID")?;
        Ok(Self {
            server,
            id,
            session_id,
            window_id,
            snapshot,
        })
    }

    /// Resolves the pane named by `TMUX_PANE` and the socket in `TMUX`.
    pub async fn from_environment() -> Result<Self> {
        Server::from_environment()?.pane_from_environment().await
    }

    /// Returns the immutable pane ID.
    #[must_use]
    pub const fn id(&self) -> PaneId {
        self.id
    }

    /// Returns the parent session ID captured with this row.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Returns the parent window ID captured with this row.
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.window_id
    }

    /// Returns the owned snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &PaneSnapshot {
        &self.snapshot
    }

    /// Returns the server handle.
    #[must_use]
    pub const fn server(&self) -> &Server {
        &self.server
    }

    /// Replaces this object with a fresh snapshot.
    pub async fn refresh(&mut self) -> Result<()> {
        *self = self.server.pane(self.id).await?;
        Ok(())
    }

    /// Resolves the parent session.
    pub async fn session(&self) -> Result<Session> {
        self.server.session(self.session_id).await
    }

    /// Resolves the parent window link.
    pub async fn window(&self) -> Result<Window> {
        self.server
            .windows()
            .await?
            .into_iter()
            .find(|window| window.id() == self.window_id && window.session_id() == self.session_id)
            .ok_or_else(|| Error::ObjectNotFound {
                kind: ObjectKind::Window,
                target: format!("{} in {}", self.window_id, self.session_id),
            })
    }

    /// Runs an arbitrary tmux command targeted at this pane.
    pub async fn cmd(&self, command: Command) -> Result<crate::CommandResult> {
        self.server.cmd(command.target(self.id)).await
    }

    /// Creates another pane by splitting this pane.
    pub async fn split(&self, options: SplitPane) -> Result<Self> {
        let mut command = Command::new("split-window")
            .arg("-P")
            .arg("-F")
            .arg("#{pane_id}")
            .target(self.id);
        if options.direction == SplitDirection::Horizontal {
            command = command.arg("-h");
        }
        if options.detached {
            command = command.arg("-d");
        }
        if options.before {
            command = command.arg("-b");
        }
        if options.full_window {
            command = command.arg("-f");
        }
        if let Some(size) = options.size {
            match size {
                PaneSize::Cells(cells) => {
                    command = command.arg("-l").arg(cells.to_string());
                }
                PaneSize::Percent(percent) => {
                    command = command.arg("-p").arg(percent.to_string());
                }
            }
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
        let result = self.server.checked("split pane", command).await?;
        let id: PaneId =
            crate::server::single_line(result.stdout(), "split-window pane ID")?.parse()?;
        self.server.pane(id).await
    }

    /// Creates a floating pane. Requires tmux 3.7+.
    pub async fn new_pane(&self, options: NewPane) -> Result<Self> {
        self.server
            .version()
            .await?
            .require("floating panes", ReleaseVersion::new(3, 7, None))?;
        let mut command = Command::new("new-pane")
            .arg("-P")
            .arg("-F")
            .arg("#{pane_id}")
            .target(self.id);
        if let Some(width) = options.width {
            command = command.arg("-x").arg(width.to_string());
        }
        if let Some(height) = options.height {
            command = command.arg("-y").arg(height.to_string());
        }
        if let Some(x) = options.x {
            command = command.arg("-X").arg(x.to_string());
        }
        if let Some(y) = options.y {
            command = command.arg("-Y").arg(y.to_string());
        }
        if options.zoom {
            command = command.arg("-Z");
        }
        if let Some(style) = options.style {
            command = command.arg("-s").arg(style);
        }
        if let Some(style) = options.active_border_style {
            command = command.arg("-S").arg(style);
        }
        if let Some(style) = options.inactive_border_style {
            command = command.arg("-R").arg(style);
        }
        if let Some(message) = options.message {
            command = command.arg("-m").arg(message);
        }
        if options.keep {
            command = command.arg("-k");
        }
        if let Some(cwd) = options.cwd {
            command = command.arg("-c").arg(cwd);
        }
        if !options.attach {
            command = command.arg("-d");
        }
        for (name, value) in options.environment {
            command =
                command
                    .arg("-e")
                    .sensitive_arg(format!("{}={}", name, value.to_string_lossy()));
        }
        if options.empty {
            command = command.arg("-E");
        }
        if let Some(shell_command) = options.shell_command {
            command = command.sensitive_arg(shell_command);
        }
        let result = self.server.checked("create floating pane", command).await?;
        let id: PaneId =
            crate::server::single_line(result.stdout(), "new-pane pane ID")?.parse()?;
        self.server.pane(id).await
    }

    /// Selects this pane.
    pub async fn select(&self, enable_input: bool) -> Result<()> {
        self.select_with(SelectPane::new().input(enable_input.then_some(true)))
            .await
    }

    /// Selects or marks a pane with explicit directional, zoom, and input flags.
    pub async fn select_with(&self, options: SelectPane) -> Result<()> {
        if options.direction.is_some() && options.last {
            return Err(Error::InvalidArgument {
                argument: "pane selection",
                message: "direction and last are mutually exclusive".to_owned(),
            });
        }
        if options.mark && options.clear_mark {
            return Err(Error::InvalidArgument {
                argument: "pane selection",
                message: "mark and clear_mark are mutually exclusive".to_owned(),
            });
        }
        let mut command = Command::new("select-pane");
        if let Some(direction) = options.direction {
            command = command.arg(direction.flag());
        }
        for (enabled, flag) in [
            (options.last, "-l"),
            (options.keep_zoom, "-Z"),
            (options.mark, "-m"),
            (options.clear_mark, "-M"),
        ] {
            if enabled {
                command = command.arg(flag);
            }
        }
        if let Some(input) = options.input {
            command = command.arg(if input { "-e" } else { "-d" });
        }
        self.server
            .checked("select pane", command.target(self.id))
            .await
            .map(|_| ())
    }

    /// Sets the pane title.
    pub async fn set_title(&self, title: impl Into<OsString>) -> Result<()> {
        self.server
            .checked(
                "set pane title",
                Command::new("select-pane")
                    .arg("-T")
                    .arg(title)
                    .target(self.id),
            )
            .await
            .map(|_| ())
    }

    /// Resizes one edge by a non-zero cell count.
    pub async fn resize(&self, direction: ResizeDirection, cells: u32) -> Result<()> {
        if cells == 0 {
            return Err(Error::InvalidArgument {
                argument: "resize cells",
                message: "must be non-zero".to_owned(),
            });
        }
        self.server
            .checked(
                "resize pane",
                Command::new("resize-pane")
                    .arg(direction.flag())
                    .arg(cells.to_string())
                    .target(self.id),
            )
            .await
            .map(|_| ())
    }

    /// Sets an absolute pane width and/or height.
    pub async fn resize_to(&self, width: Option<u32>, height: Option<u32>) -> Result<()> {
        if width.is_none() && height.is_none() {
            return Err(Error::InvalidArgument {
                argument: "absolute pane size",
                message: "width or height is required".to_owned(),
            });
        }
        if width == Some(0) || height == Some(0) {
            return Err(Error::InvalidArgument {
                argument: "absolute pane size",
                message: "dimensions must be non-zero".to_owned(),
            });
        }
        let mut command = Command::new("resize-pane");
        if let Some(width) = width {
            command = command.arg("-x").arg(width.to_string());
        }
        if let Some(height) = height {
            command = command.arg("-y").arg(height.to_string());
        }
        self.server
            .checked("resize pane", command.target(self.id))
            .await
            .map(|_| ())
    }

    /// Sets pane width to a percentage of the window.
    pub async fn resize_width_percent(&self, percent: u8) -> Result<()> {
        self.resize_percent("-x", percent).await
    }

    /// Sets pane height to a percentage of the window.
    pub async fn resize_height_percent(&self, percent: u8) -> Result<()> {
        self.resize_percent("-y", percent).await
    }

    async fn resize_percent(&self, flag: &'static str, percent: u8) -> Result<()> {
        if !(1..=100).contains(&percent) {
            return Err(Error::InvalidArgument {
                argument: "pane size percentage",
                message: "must be between 1 and 100".to_owned(),
            });
        }
        self.server
            .checked(
                "resize pane",
                Command::new("resize-pane")
                    .arg(flag)
                    .arg(format!("{percent}%"))
                    .target(self.id),
            )
            .await
            .map(|_| ())
    }

    /// Resizes using tmux's current mouse event (`-M`).
    pub async fn resize_from_mouse(&self) -> Result<()> {
        self.server
            .checked(
                "resize pane from mouse",
                Command::new("resize-pane").arg("-M").target(self.id),
            )
            .await
            .map(|_| ())
    }

    /// Trims lines below the current cursor (`resize-pane -T`).
    pub async fn trim_below(&self) -> Result<()> {
        self.server
            .checked(
                "trim pane below cursor",
                Command::new("resize-pane").arg("-T").target(self.id),
            )
            .await
            .map(|_| ())
    }

    /// Toggles zoom for this pane's window.
    pub async fn toggle_zoom(&self) -> Result<()> {
        self.server
            .checked(
                "toggle pane zoom",
                Command::new("resize-pane").arg("-Z").target(self.id),
            )
            .await
            .map(|_| ())
    }

    /// Captures pane contents without forcing UTF-8.
    pub async fn capture(&self, options: &CapturePane) -> Result<TmuxText> {
        self.validate_capture(options).await?;
        let command = self.capture_command(options).arg("-p");
        self.server
            .checked("capture pane", command)
            .await
            .map(|result| result.stdout().clone())
    }

    /// Captures pane contents directly into a named tmux buffer.
    pub async fn capture_to_buffer(
        &self,
        options: &CapturePane,
        buffer: &BufferName,
    ) -> Result<()> {
        self.validate_capture(options).await?;
        self.server
            .checked(
                "capture pane to buffer",
                self.capture_command(options)
                    .arg("-b")
                    .arg(buffer.to_os_string()),
            )
            .await
            .map(|_| ())
    }

    fn capture_command(&self, options: &CapturePane) -> Command {
        let mut command = Command::new("capture-pane").target(self.id);
        if let Some(start) = options.start {
            command = command.arg("-S").arg(start.to_string());
        }
        if let Some(end) = options.end {
            command = command.arg("-E").arg(end.to_string());
        }
        if options.alternate {
            command = command.arg("-a");
        }
        if options.escape_sequences {
            command = command.arg("-e");
        }
        if options.escape_non_printable {
            command = command.arg("-C");
        }
        if options.join_wrapped {
            command = command.arg("-J");
        }
        if options.preserve_trailing {
            command = command.arg("-N");
        }
        if options.pending {
            command = command.arg("-P");
        }
        if options.trim_trailing {
            command = command.arg("-T");
        }
        if options.quiet {
            command = command.arg("-q");
        }
        if options.mode_screen {
            command = command.arg("-M");
        }
        if options.hyperlinks {
            command = command.arg("-H");
        }
        if options.line_numbers {
            command = command.arg("-L");
        }
        if options.line_flags {
            command = command.arg("-F");
        }
        command
    }

    async fn validate_capture(&self, options: &CapturePane) -> Result<()> {
        if options.trim_trailing {
            self.server.version().await?.require(
                "trim trailing capture cells",
                ReleaseVersion::new(3, 4, None),
            )?;
        }
        if options.mode_screen {
            self.server
                .version()
                .await?
                .require("capture mode screen", ReleaseVersion::new(3, 6, None))?;
        }
        if options.hyperlinks || options.line_numbers || options.line_flags {
            self.server
                .version()
                .await?
                .require("extended pane capture", ReleaseVersion::new(3, 7, None))?;
        }
        Ok(())
    }

    /// Sends named keys or literal text.
    pub async fn send_keys(&self, options: SendKeys) -> Result<()> {
        let send_enter = options.enter && options.copy_mode_command.is_none();
        if options.keys.is_empty()
            && !options.reset
            && options.repeat.is_none()
            && options.copy_mode_command.is_none()
        {
            return Err(Error::InvalidArgument {
                argument: "send keys",
                message: "keys, reset, repeat, or a copy-mode command is required".to_owned(),
            });
        }
        if options.literal && options.hexadecimal {
            return Err(Error::InvalidArgument {
                argument: "send keys",
                message: "literal and hexadecimal modes are mutually exclusive".to_owned(),
            });
        }
        if options.copy_mode_command.is_some() && !options.keys.is_empty() {
            return Err(Error::InvalidArgument {
                argument: "send keys",
                message: "ordinary keys and a copy-mode command are mutually exclusive".to_owned(),
            });
        }
        if options.target_client.is_some() || options.key_name {
            self.server
                .version()
                .await?
                .require("client-targeted keys", ReleaseVersion::new(3, 4, None))?;
        }
        let mut command = Command::new("send-keys").target(self.id);
        if options.literal {
            command = command.arg("-l");
        }
        if options.hexadecimal {
            command = command.arg("-H");
        }
        if options.reset {
            command = command.arg("-R");
        }
        if options.expand_formats {
            command = command.arg("-F");
        }
        if options.key_name {
            command = command.arg("-K");
        }
        if let Some(client) = options.target_client {
            command = command.arg("-c").arg(client.to_string());
        }
        if let Some(repeat) = options.repeat {
            command = command.arg("-N").arg(repeat.to_string());
        }
        if let Some(copy_mode_command) = options.copy_mode_command {
            command = command.arg("-X");
            command = if options.sensitive {
                command.sensitive_arg(copy_mode_command)
            } else {
                command.arg(copy_mode_command)
            };
        } else {
            if options.suppress_history {
                command = command.arg(" ");
            }
            for key in options.keys {
                command = if options.sensitive {
                    command.sensitive_arg(key)
                } else {
                    command.arg(key)
                };
            }
        }
        self.server.checked("send keys", command).await?;
        if send_enter {
            self.server
                .checked(
                    "send Enter key",
                    Command::new("send-keys").target(self.id).arg("Enter"),
                )
                .await?;
        }
        Ok(())
    }

    /// Clears pane history.
    pub async fn clear_history(&self) -> Result<()> {
        self.server
            .checked(
                "clear pane history",
                Command::new("clear-history").target(self.id),
            )
            .await
            .map(|_| ())
    }

    /// Clears history and optionally resets hyperlinks (tmux 3.4+).
    pub async fn clear_history_with(&self, reset_hyperlinks: bool) -> Result<()> {
        if reset_hyperlinks {
            self.server
                .version()
                .await?
                .require("reset pane hyperlinks", ReleaseVersion::new(3, 4, None))?;
        }
        let mut command = Command::new("clear-history").target(self.id);
        if reset_hyperlinks {
            command = command.arg("-H");
        }
        self.server
            .checked("clear pane history", command)
            .await
            .map(|_| ())
    }

    /// Sends the shell `reset` command followed by Enter, matching Python `Pane.clear()`.
    pub async fn clear(&self) -> Result<()> {
        self.send_keys(SendKeys::new().key("reset").enter(true))
            .await
    }

    /// Resets the pane terminal state.
    pub async fn reset(&self, clear_history: bool) -> Result<()> {
        let mut command = Command::new("send-keys").target(self.id).arg("-R");
        if clear_history {
            command = command
                .arg(";")
                .arg("clear-history")
                .arg("-t")
                .arg(self.id.to_string());
        }
        self.server.checked("reset pane", command).await.map(|_| ())
    }

    /// Enters or manipulates copy mode.
    pub async fn copy_mode(&self, options: CopyMode) -> Result<()> {
        if options.page_down {
            self.server
                .version()
                .await?
                .require("copy-mode page down", ReleaseVersion::new(3, 5, None))?;
        }
        let mut command = Command::new("copy-mode");
        if options.scroll_up {
            command = command.arg("-u");
        }
        if options.exit_on_bottom {
            command = command.arg("-e");
        }
        if options.mouse_drag {
            command = command.arg("-M");
        }
        if options.page_down {
            command = command.arg("-d");
        }
        if let Some(source) = options.source {
            command = command.arg("-s").arg(source.to_string());
        }
        if options.cancel {
            command = command.arg("-q");
        }
        self.server
            .checked("enter copy mode", command.target(self.id))
            .await
            .map(|_| ())
    }

    /// Pastes a tmux buffer into the pane.
    pub async fn paste_buffer(&self, options: PasteBuffer) -> Result<()> {
        if options.raw {
            self.server
                .version()
                .await?
                .require("raw paste-buffer mode", ReleaseVersion::new(3, 7, None))?;
        }
        let mut command = Command::new("paste-buffer").target(self.id);
        if options.delete_after {
            command = command.arg("-d");
        }
        if options.linefeed_separator {
            command = command.arg("-r");
        }
        if options.bracketed {
            command = command.arg("-p");
        }
        if let Some(name) = options.name {
            command = command.arg("-b").arg(name.to_os_string());
        }
        if let Some(separator) = options.separator {
            command = command.arg("-s").arg(separator);
        }
        if options.raw {
            command = command.arg("-S");
        }
        self.server
            .checked("paste buffer", command)
            .await
            .map(|_| ())
    }

    /// Configures or disables a pane pipe.
    pub async fn pipe(&self, options: PipePane) -> Result<()> {
        if options.output_only && options.input_only {
            return Err(Error::InvalidArgument {
                argument: "pane pipe direction",
                message: "output_only and input_only are mutually exclusive".to_owned(),
            });
        }
        let mut command = Command::new("pipe-pane").target(self.id);
        if options.output_only {
            command = command.arg("-O");
        }
        if options.input_only {
            command = command.arg("-I");
        }
        if options.toggle {
            command = command.arg("-o");
        }
        if let Some(shell_command) = options.command {
            command = command.sensitive_arg(shell_command);
        }
        self.server.checked("pipe pane", command).await.map(|_| ())
    }

    /// Enters clock mode.
    pub async fn clock_mode(&self) -> Result<()> {
        self.server
            .checked("clock mode", Command::new("clock-mode").target(self.id))
            .await
            .map(|_| ())
    }

    /// Displays pane numbers for an attached client.
    pub async fn display_panes(&self, duration_millis: Option<u32>, no_select: bool) -> Result<()> {
        let mut command = Command::new("display-panes");
        if let Some(duration) = duration_millis {
            command = command.arg("-d").arg(duration.to_string());
        }
        if no_select {
            command = command.arg("-N");
        }
        self.server
            .checked("display panes", command)
            .await
            .map(|_| ())
    }

    /// Enters buffer chooser mode.
    pub async fn choose_buffer(&self) -> Result<()> {
        self.server
            .checked(
                "choose buffer",
                Command::new("choose-buffer").target(self.id),
            )
            .await
            .map(|_| ())
    }

    /// Enters client chooser mode.
    pub async fn choose_client(&self) -> Result<()> {
        self.server
            .checked(
                "choose client",
                Command::new("choose-client").target(self.id),
            )
            .await
            .map(|_| ())
    }

    /// Enters tree chooser mode.
    pub async fn choose_tree(&self, options: ChooseTree) -> Result<()> {
        let mut command = Command::new("choose-tree").target(self.id);
        for (enabled, flag) in [
            (options.sessions_collapsed, "-s"),
            (options.windows_collapsed, "-w"),
            (options.zoom, "-Z"),
            (options.reverse, "-r"),
        ] {
            if enabled {
                command = command.arg(flag);
            }
        }
        if let Some(format) = options.format {
            command = command.arg("-F").arg(format);
        }
        if let Some(filter) = options.native_filter {
            command = command.arg("-f").arg(filter);
        }
        if let Some(sort) = options.sort {
            command = command.arg("-O").arg(sort.as_str());
        }
        self.server
            .checked("choose tree", command)
            .await
            .map(|_| ())
    }

    /// Enters customize mode.
    pub async fn customize_mode(&self) -> Result<()> {
        self.server
            .checked(
                "customize mode",
                Command::new("customize-mode").target(self.id),
            )
            .await
            .map(|_| ())
    }

    /// Opens a window search chooser.
    pub async fn find_window(&self, options: FindWindow) -> Result<()> {
        let mut command = Command::new("find-window").target(self.id);
        for (enabled, flag) in [
            (options.content, "-C"),
            (options.case_insensitive, "-i"),
            (options.name_only, "-N"),
            (options.regex, "-r"),
            (options.title, "-T"),
        ] {
            if enabled {
                command = command.arg(flag);
            }
        }
        self.server
            .checked("find window", command.arg(options.pattern))
            .await
            .map(|_| ())
    }

    /// Sends the primary or secondary prefix key.
    pub async fn send_prefix(&self, secondary: bool) -> Result<()> {
        let mut command = Command::new("send-prefix").target(self.id);
        if secondary {
            command = command.arg("-2");
        }
        self.server
            .checked("send prefix", command)
            .await
            .map(|_| ())
    }

    /// Swaps this pane with another pane.
    pub async fn swap_with(&self, destination: PaneId, detached: bool) -> Result<()> {
        let mut command = Command::new("swap-pane").arg("-s").arg(self.id.to_string());
        if detached {
            command = command.arg("-d");
        }
        self.server
            .checked("swap pane", command.arg("-t").arg(destination.to_string()))
            .await
            .map(|_| ())
    }

    /// Swaps this pane using explicit or relative typed arguments.
    pub async fn swap(&self, options: SwapPane) -> Result<()> {
        if options.destination.is_some() && options.direction.is_some() {
            return Err(Error::InvalidArgument {
                argument: "pane swap",
                message: "destination and relative direction are mutually exclusive".to_owned(),
            });
        }
        let mut command = Command::new("swap-pane").arg("-s").arg(self.id.to_string());
        if let Some(destination) = options.destination {
            command = command.arg("-t").arg(destination.to_string());
        }
        if let Some(direction) = options.direction {
            command = command.arg(match direction {
                SwapPaneDirection::Up => "-U",
                SwapPaneDirection::Down => "-D",
            });
        }
        if options.detached {
            command = command.arg("-d");
        }
        if options.keep_zoom {
            command = command.arg("-Z");
        }
        self.server.checked("swap pane", command).await.map(|_| ())
    }

    /// Moves this pane into another pane or window.
    pub async fn move_to(&self, options: RelocatePane) -> Result<()> {
        self.relocate("move-pane", "move pane", options).await
    }

    /// Joins this pane into another pane or window.
    pub async fn join(&self, options: RelocatePane) -> Result<()> {
        self.relocate("join-pane", "join pane", options).await
    }

    async fn relocate(
        &self,
        subcommand: &'static str,
        operation: &'static str,
        options: RelocatePane,
    ) -> Result<()> {
        let mut command = Command::new(subcommand)
            .arg(match options.direction {
                SplitDirection::Vertical => "-v",
                SplitDirection::Horizontal => "-h",
            })
            .arg("-s")
            .arg(self.id.to_string())
            .arg("-t")
            .arg(options.destination.to_string());
        if options.detached {
            command = command.arg("-d");
        }
        if options.full_window {
            command = command.arg("-f");
        }
        if let Some(size) = options.size {
            let size = match size {
                PaneSize::Cells(cells) => cells.to_string(),
                PaneSize::Percent(percent) => format!("{percent}%"),
            };
            command = command.arg("-l").arg(size);
        }
        if options.before {
            command = command.arg("-b");
        }
        self.server.checked(operation, command).await.map(|_| ())
    }

    /// Joins this pane into another window.
    pub async fn join_to(
        &self,
        destination: impl Into<WindowTarget>,
        horizontal: bool,
        before: bool,
        percent: Option<u8>,
    ) -> Result<()> {
        if percent.is_some_and(|value| !(1..=100).contains(&value)) {
            return Err(Error::InvalidArgument {
                argument: "join pane percentage",
                message: "must be between 1 and 100".to_owned(),
            });
        }
        let mut command = Command::new("join-pane")
            .arg("-s")
            .arg(self.id.to_string())
            .arg("-t")
            .arg(destination.into().to_string());
        if horizontal {
            command = command.arg("-h");
        }
        if before {
            command = command.arg("-b");
        }
        if let Some(percent) = percent {
            command = command.arg("-l").arg(format!("{percent}%"));
        }
        self.server.checked("join pane", command).await.map(|_| ())
    }

    /// Breaks this pane into a new window and returns the new link.
    pub async fn break_to_window(&self, detached: bool, name: Option<&str>) -> Result<Window> {
        let mut command = Command::new("break-pane")
            .arg("-P")
            .arg("-F")
            .arg("#{window_id}")
            .target(self.id);
        if detached {
            command = command.arg("-d");
        }
        if let Some(name) = name {
            command = command.arg("-n").arg(name);
        }
        let result = self.server.checked("break pane", command).await?;
        let id: WindowId =
            crate::server::single_line(result.stdout(), "break-pane window ID")?.parse()?;
        self.server
            .windows()
            .await?
            .into_iter()
            .find(|window| window.id() == id && window.session_id() == self.session_id)
            .ok_or_else(|| Error::ObjectNotFound {
                kind: ObjectKind::Window,
                target: id.to_string(),
            })
    }

    /// Respawns the pane process.
    pub async fn respawn(&self, options: RespawnPane) -> Result<()> {
        let mut command = Command::new("respawn-pane").target(self.id);
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
        if let Some(shell_command) = options.shell_command {
            command = command.sensitive_arg(shell_command);
        }
        self.server
            .checked("respawn pane", command)
            .await
            .map(|_| ())
    }

    /// Destroys the pane. Dropping the handle is non-destructive.
    pub async fn kill(&self) -> Result<()> {
        self.kill_with(false).await
    }

    /// Destroys this pane, or every other pane in the window when requested.
    pub async fn kill_with(&self, all_except: bool) -> Result<()> {
        let mut command = Command::new("kill-pane").target(self.id);
        if all_except {
            command = command.arg("-a");
        }
        self.server.checked("kill pane", command).await.map(|_| ())
    }

    /// Shows pane-scoped options.
    pub async fn options(&self) -> Result<OptionMap> {
        self.server
            .show_options(OptionScope::Pane, Some(&self.id.to_string()))
            .await
    }

    /// Shows sparse pane options and hooks.
    pub async fn sparse_options(&self) -> Result<SparseOptionMap> {
        self.server
            .show_sparse_options(OptionScope::Pane, Some(&self.id.to_string()))
            .await
    }

    /// Sets a pane option.
    pub async fn set_option(&self, name: &str, value: &OptionValue, append: bool) -> Result<()> {
        self.server
            .set_option(
                OptionScope::Pane,
                Some(&self.id.to_string()),
                name,
                value,
                append,
            )
            .await
    }

    /// Unsets a pane option.
    pub async fn unset_option(&self, name: &str) -> Result<()> {
        self.server
            .unset_option(OptionScope::Pane, Some(&self.id.to_string()), name)
            .await
    }
}

impl PartialEq for Pane {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.server.config() == other.server.config()
    }
}

impl Eq for Pane {}

fn parse_parent_id<T>(
    snapshot: &PaneSnapshot,
    token: &'static str,
    context: &'static str,
) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let value = snapshot.get(token).ok_or_else(|| Error::Decode {
        context,
        message: format!("{token} was absent"),
    })?;
    let value = value.to_str().map_err(|source| Error::Decode {
        context,
        message: source.to_string(),
    })?;
    value.parse::<T>().map_err(|source| Error::Decode {
        context,
        message: source.to_string(),
    })
}
