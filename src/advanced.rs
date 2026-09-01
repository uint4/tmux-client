//! Typed builders for advanced tmux command families.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use crate::{
    ClientName, Command, Error, OptionValue, Pane, PaneId, ReleaseVersion, Result, Server,
    SparseOptionMap, TmuxText, Window,
};

const V33: ReleaseVersion = ReleaseVersion::new(3, 3, None);
const V34: ReleaseVersion = ReleaseVersion::new(3, 4, None);
const V35: ReleaseVersion = ReleaseVersion::new(3, 5, None);
const V36: ReleaseVersion = ReleaseVersion::new(3, 6, None);
const V37: ReleaseVersion = ReleaseVersion::new(3, 7, None);

/// Typed arguments for `run-shell`.
#[derive(Clone, Debug)]
pub struct RunShell {
    command: OsString,
    background: bool,
    delay: Option<Duration>,
    as_tmux_command: bool,
    target: Option<PaneId>,
    cwd: Option<PathBuf>,
    show_stderr: bool,
    arguments: Vec<OsString>,
}

impl RunShell {
    /// Creates a shell job.
    #[must_use]
    pub fn new(command: impl Into<OsString>) -> Self {
        Self {
            command: command.into(),
            background: false,
            delay: None,
            as_tmux_command: false,
            target: None,
            cwd: None,
            show_stderr: false,
            arguments: Vec::new(),
        }
    }

    /// Runs the job without waiting for output.
    #[must_use]
    pub const fn background(mut self, enabled: bool) -> Self {
        self.background = enabled;
        self
    }

    /// Delays execution by a non-zero duration.
    pub fn delay(mut self, delay: Duration) -> Result<Self> {
        if delay.is_zero() {
            return Err(Error::InvalidArgument {
                argument: "run-shell delay",
                message: "must be non-zero".to_owned(),
            });
        }
        self.delay = Some(delay);
        Ok(self)
    }

    /// Parses the command as tmux commands instead of a shell command.
    #[must_use]
    pub const fn as_tmux_command(mut self, enabled: bool) -> Self {
        self.as_tmux_command = enabled;
        self
    }

    /// Selects a pane for formats and output.
    #[must_use]
    pub const fn target(mut self, target: PaneId) -> Self {
        self.target = Some(target);
        self
    }

    /// Sets the starting directory. Requires tmux 3.4+.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Includes job standard error. Requires tmux 3.6+.
    #[must_use]
    pub const fn show_stderr(mut self, enabled: bool) -> Self {
        self.show_stderr = enabled;
        self
    }

    /// Adds positional template arguments. Requires tmux 3.7+.
    #[must_use]
    pub fn arguments<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.arguments.extend(arguments.into_iter().map(Into::into));
        self
    }
}

/// An operation supported by `wait-for`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum WaitAction {
    /// Wait until another command signals the channel.
    #[default]
    Wait,
    /// Signal the channel (`-S`).
    Signal,
    /// Lock the channel (`-L`).
    Lock,
    /// Unlock the channel (`-U`).
    Unlock,
}

impl WaitAction {
    fn flag(self) -> Option<&'static str> {
        match self {
            Self::Wait => None,
            Self::Signal => Some("-S"),
            Self::Lock => Some("-L"),
            Self::Unlock => Some("-U"),
        }
    }
}

/// Typed arguments for `bind-key`.
#[derive(Clone, Debug)]
pub struct KeyBinding {
    key: OsString,
    command: OsString,
    table: Option<OsString>,
    note: Option<OsString>,
    repeat: bool,
    root: bool,
}

impl KeyBinding {
    /// Creates a key binding.
    #[must_use]
    pub fn new(key: impl Into<OsString>, command: impl Into<OsString>) -> Self {
        Self {
            key: key.into(),
            command: command.into(),
            table: None,
            note: None,
            repeat: false,
            root: false,
        }
    }

    /// Selects a key table.
    #[must_use]
    pub fn table(mut self, table: impl Into<OsString>) -> Self {
        self.table = Some(table.into());
        self
    }

    /// Adds a user-facing binding note.
    #[must_use]
    pub fn note(mut self, note: impl Into<OsString>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Makes the binding repeatable.
    #[must_use]
    pub const fn repeat(mut self, enabled: bool) -> Self {
        self.repeat = enabled;
        self
    }

    /// Binds without the prefix key (`-n`).
    #[must_use]
    pub const fn root(mut self, enabled: bool) -> Self {
        self.root = enabled;
        self
    }
}

/// Typed arguments for `unbind-key`.
#[derive(Clone, Debug, Default)]
pub struct UnbindKey {
    key: Option<OsString>,
    table: Option<OsString>,
    all: bool,
    quiet: bool,
}

impl UnbindKey {
    /// Creates an empty unbind request.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Selects a key.
    #[must_use]
    pub fn key(mut self, key: impl Into<OsString>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Selects a key table.
    #[must_use]
    pub fn table(mut self, table: impl Into<OsString>) -> Self {
        self.table = Some(table.into());
        self
    }

    /// Removes all bindings in the selected table.
    #[must_use]
    pub const fn all(mut self, enabled: bool) -> Self {
        self.all = enabled;
        self
    }

    /// Suppresses errors for missing keys.
    #[must_use]
    pub const fn quiet(mut self, enabled: bool) -> Self {
        self.quiet = enabled;
        self
    }
}

/// A server ACL permission.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ClientPermission {
    /// Force read-only attachment.
    ReadOnly,
    /// Permit read-write attachment.
    ReadWrite,
}

/// A typed `server-access` action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerAccess {
    /// Allow a user with an explicit permission.
    Allow {
        /// Operating-system username.
        user: String,
        /// Attachment permission.
        permission: ClientPermission,
    },
    /// Deny a user.
    Deny {
        /// Operating-system username.
        user: String,
    },
    /// List access rules.
    List,
}

/// Typed arguments for `confirm-before`.
#[derive(Clone, Debug)]
pub struct ConfirmBefore {
    command: OsString,
    prompt: Option<OsString>,
    confirm_key: Option<OsString>,
    default_yes: bool,
    target_client: Option<ClientName>,
}

impl ConfirmBefore {
    /// Creates a background confirmation request.
    #[must_use]
    pub fn new(command: impl Into<OsString>) -> Self {
        Self {
            command: command.into(),
            prompt: None,
            confirm_key: None,
            default_yes: false,
            target_client: None,
        }
    }

    /// Sets prompt text.
    #[must_use]
    pub fn prompt(mut self, prompt: impl Into<OsString>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    /// Sets the acceptance key. Requires tmux 3.4+.
    #[must_use]
    pub fn confirm_key(mut self, key: impl Into<OsString>) -> Self {
        self.confirm_key = Some(key.into());
        self
    }

    /// Makes Enter confirm by default. Requires tmux 3.4+.
    #[must_use]
    pub const fn default_yes(mut self, enabled: bool) -> Self {
        self.default_yes = enabled;
        self
    }

    /// Targets an attached client.
    #[must_use]
    pub fn target_client(mut self, client: ClientName) -> Self {
        self.target_client = Some(client);
        self
    }
}

/// A tmux prompt-history and completion namespace.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PromptType {
    /// Command history.
    Command,
    /// Search history.
    Search,
    /// General target history.
    Target,
    /// Window-target history.
    WindowTarget,
}

impl PromptType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Search => "search",
            Self::Target => "target",
            Self::WindowTarget => "window-target",
        }
    }
}

/// Typed arguments for `command-prompt`.
#[derive(Clone, Debug)]
pub struct CommandPrompt {
    template: OsString,
    prompt: Option<OsString>,
    inputs: Option<OsString>,
    target_client: Option<ClientName>,
    one_key: bool,
    key_only: bool,
    on_input_change: bool,
    numeric: bool,
    prompt_type: Option<PromptType>,
    expand_format: bool,
    literal: bool,
    backspace_exit: bool,
    no_freeze: bool,
}

impl CommandPrompt {
    /// Creates a background prompt with a tmux command template.
    #[must_use]
    pub fn new(template: impl Into<OsString>) -> Self {
        Self {
            template: template.into(),
            prompt: None,
            inputs: None,
            target_client: None,
            one_key: false,
            key_only: false,
            on_input_change: false,
            numeric: false,
            prompt_type: None,
            expand_format: false,
            literal: false,
            backspace_exit: false,
            no_freeze: false,
        }
    }

    /// Sets prompt text.
    #[must_use]
    pub fn prompt(mut self, prompt: impl Into<OsString>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    /// Sets prefilled inputs.
    #[must_use]
    pub fn inputs(mut self, inputs: impl Into<OsString>) -> Self {
        self.inputs = Some(inputs.into());
        self
    }

    /// Targets an attached client.
    #[must_use]
    pub fn target_client(mut self, client: ClientName) -> Self {
        self.target_client = Some(client);
        self
    }

    /// Accepts one key press.
    #[must_use]
    pub const fn one_key(mut self, enabled: bool) -> Self {
        self.one_key = enabled;
        self
    }

    /// Accepts keys but no text input.
    #[must_use]
    pub const fn key_only(mut self, enabled: bool) -> Self {
        self.key_only = enabled;
        self
    }

    /// Runs the template after every input change.
    #[must_use]
    pub const fn on_input_change(mut self, enabled: bool) -> Self {
        self.on_input_change = enabled;
        self
    }

    /// Restricts input to numbers.
    #[must_use]
    pub const fn numeric(mut self, enabled: bool) -> Self {
        self.numeric = enabled;
        self
    }

    /// Selects the history namespace.
    #[must_use]
    pub const fn prompt_type(mut self, prompt_type: PromptType) -> Self {
        self.prompt_type = Some(prompt_type);
        self
    }

    /// Expands formats in the command template.
    #[must_use]
    pub const fn expand_format(mut self, enabled: bool) -> Self {
        self.expand_format = enabled;
        self
    }

    /// Treats a comma-containing prompt literally. Requires tmux 3.6+.
    #[must_use]
    pub const fn literal(mut self, enabled: bool) -> Self {
        self.literal = enabled;
        self
    }

    /// Exits when backspace empties input. Requires tmux 3.7+.
    #[must_use]
    pub const fn backspace_exit(mut self, enabled: bool) -> Self {
        self.backspace_exit = enabled;
        self
    }

    /// Keeps panes unfrozen. Requires tmux 3.7+.
    #[must_use]
    pub const fn no_freeze(mut self, enabled: bool) -> Self {
        self.no_freeze = enabled;
        self
    }
}

/// One entry in a popup menu.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MenuItem {
    /// A selectable name/key/command triple.
    Entry {
        /// Displayed label.
        label: OsString,
        /// Shortcut key.
        key: OsString,
        /// tmux command executed on selection.
        command: OsString,
    },
    /// A visual separator.
    Separator,
}

impl MenuItem {
    /// Creates a selectable entry.
    #[must_use]
    pub fn entry(
        label: impl Into<OsString>,
        key: impl Into<OsString>,
        command: impl Into<OsString>,
    ) -> Self {
        Self::Entry {
            label: label.into(),
            key: key.into(),
            command: command.into(),
        }
    }
}

/// Typed arguments for `display-menu`.
#[derive(Clone, Debug, Default)]
pub struct Menu {
    items: Vec<MenuItem>,
    title: Option<OsString>,
    target_pane: Option<PaneId>,
    target_client: Option<ClientName>,
    x: Option<OsString>,
    y: Option<OsString>,
    starting_choice: Option<OsString>,
    border_lines: Option<OsString>,
    style: Option<OsString>,
    border_style: Option<OsString>,
    selected_style: Option<OsString>,
    mouse: bool,
    stay_open: bool,
}

impl Menu {
    /// Creates an empty menu.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a menu item.
    #[must_use]
    pub fn item(mut self, item: MenuItem) -> Self {
        self.items.push(item);
        self
    }

    /// Sets the menu title.
    #[must_use]
    pub fn title(mut self, title: impl Into<OsString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Targets a pane.
    #[must_use]
    pub const fn target_pane(mut self, pane: PaneId) -> Self {
        self.target_pane = Some(pane);
        self
    }

    /// Targets a client.
    #[must_use]
    pub fn target_client(mut self, client: ClientName) -> Self {
        self.target_client = Some(client);
        self
    }

    /// Sets an x position or tmux position expression.
    #[must_use]
    pub fn x(mut self, value: impl Into<OsString>) -> Self {
        self.x = Some(value.into());
        self
    }

    /// Sets a y position or tmux position expression.
    #[must_use]
    pub fn y(mut self, value: impl Into<OsString>) -> Self {
        self.y = Some(value.into());
        self
    }

    /// Sets the initially selected row. Requires tmux 3.4+.
    #[must_use]
    pub fn starting_choice(mut self, value: impl Into<OsString>) -> Self {
        self.starting_choice = Some(value.into());
        self
    }

    /// Sets the border line style. Requires tmux 3.4+.
    #[must_use]
    pub fn border_lines(mut self, value: impl Into<OsString>) -> Self {
        self.border_lines = Some(value.into());
        self
    }

    /// Sets menu style. Requires tmux 3.4+.
    #[must_use]
    pub fn style(mut self, value: impl Into<OsString>) -> Self {
        self.style = Some(value.into());
        self
    }

    /// Sets border style. Requires tmux 3.4+.
    #[must_use]
    pub fn border_style(mut self, value: impl Into<OsString>) -> Self {
        self.border_style = Some(value.into());
        self
    }

    /// Sets selected-entry style. Requires tmux 3.4+.
    #[must_use]
    pub fn selected_style(mut self, value: impl Into<OsString>) -> Self {
        self.selected_style = Some(value.into());
        self
    }

    /// Enables mouse handling. Requires tmux 3.5+.
    #[must_use]
    pub const fn mouse(mut self, enabled: bool) -> Self {
        self.mouse = enabled;
        self
    }

    /// Keeps the menu open after mouse release.
    #[must_use]
    pub const fn stay_open(mut self, enabled: bool) -> Self {
        self.stay_open = enabled;
        self
    }
}

/// Typed arguments for `display-popup`.
#[derive(Clone, Debug, Default)]
pub struct Popup {
    command: Option<OsString>,
    close_on_exit: bool,
    close_on_success: bool,
    close_existing: bool,
    target_client: Option<ClientName>,
    width: Option<OsString>,
    height: Option<OsString>,
    x: Option<OsString>,
    y: Option<OsString>,
    cwd: Option<PathBuf>,
    title: Option<OsString>,
    border_lines: Option<OsString>,
    style: Option<OsString>,
    border_style: Option<OsString>,
    environment: BTreeMap<String, OsString>,
    no_border: bool,
    close_on_any_key: bool,
    no_keys: bool,
}

impl Popup {
    /// Creates popup arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the popup shell command.
    #[must_use]
    pub fn command(mut self, command: impl Into<OsString>) -> Self {
        self.command = Some(command.into());
        self
    }

    /// Closes after any exit status.
    #[must_use]
    pub const fn close_on_exit(mut self, enabled: bool) -> Self {
        self.close_on_exit = enabled;
        self
    }

    /// Closes only after success.
    #[must_use]
    pub const fn close_on_success(mut self, enabled: bool) -> Self {
        self.close_on_success = enabled;
        self
    }

    /// Closes an existing popup.
    #[must_use]
    pub const fn close_existing(mut self, enabled: bool) -> Self {
        self.close_existing = enabled;
        self
    }

    /// Targets a client.
    #[must_use]
    pub fn target_client(mut self, client: ClientName) -> Self {
        self.target_client = Some(client);
        self
    }

    /// Sets width in cells, percent, or a tmux expression.
    #[must_use]
    pub fn width(mut self, value: impl Into<OsString>) -> Self {
        self.width = Some(value.into());
        self
    }

    /// Sets height in cells, percent, or a tmux expression.
    #[must_use]
    pub fn height(mut self, value: impl Into<OsString>) -> Self {
        self.height = Some(value.into());
        self
    }

    /// Sets x position.
    #[must_use]
    pub fn x(mut self, value: impl Into<OsString>) -> Self {
        self.x = Some(value.into());
        self
    }

    /// Sets y position.
    #[must_use]
    pub fn y(mut self, value: impl Into<OsString>) -> Self {
        self.y = Some(value.into());
        self
    }

    /// Sets the starting directory.
    #[must_use]
    pub fn cwd(mut self, value: impl Into<PathBuf>) -> Self {
        self.cwd = Some(value.into());
        self
    }

    /// Sets the title. Requires tmux 3.3+.
    #[must_use]
    pub fn title(mut self, value: impl Into<OsString>) -> Self {
        self.title = Some(value.into());
        self
    }

    /// Sets border line style. Requires tmux 3.3+.
    #[must_use]
    pub fn border_lines(mut self, value: impl Into<OsString>) -> Self {
        self.border_lines = Some(value.into());
        self
    }

    /// Sets popup style. Requires tmux 3.3+.
    #[must_use]
    pub fn style(mut self, value: impl Into<OsString>) -> Self {
        self.style = Some(value.into());
        self
    }

    /// Sets border style. Requires tmux 3.3+.
    #[must_use]
    pub fn border_style(mut self, value: impl Into<OsString>) -> Self {
        self.border_style = Some(value.into());
        self
    }

    /// Adds an environment entry. Requires tmux 3.3+.
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

    /// Removes the popup border. Requires tmux 3.3+.
    #[must_use]
    pub const fn no_border(mut self, enabled: bool) -> Self {
        self.no_border = enabled;
        self
    }

    /// Closes on any key after process exit. Requires tmux 3.6+.
    #[must_use]
    pub const fn close_on_any_key(mut self, enabled: bool) -> Self {
        self.close_on_any_key = enabled;
        self
    }

    /// Disables automatic close keys. Requires tmux 3.6+.
    #[must_use]
    pub const fn no_keys(mut self, enabled: bool) -> Self {
        self.no_keys = enabled;
        self
    }
}

/// Typed arguments for the broad `display-message` flag family.
#[derive(Clone, Debug, Default)]
pub struct DisplayMessage {
    message: Option<OsString>,
    print: bool,
    all_formats: bool,
    verbose: bool,
    literal: bool,
    notify: bool,
    client: Option<ClientName>,
    delay_millis: Option<u32>,
    format: Option<OsString>,
}

/// Typed arguments for conditional tmux command execution.
#[derive(Clone, Debug)]
pub struct IfShell {
    condition: OsString,
    if_command: OsString,
    else_command: Option<OsString>,
    background: bool,
    format: bool,
    target: Option<PaneId>,
}

impl IfShell {
    /// Creates a shell condition and success command.
    #[must_use]
    pub fn new(condition: impl Into<OsString>, if_command: impl Into<OsString>) -> Self {
        Self {
            condition: condition.into(),
            if_command: if_command.into(),
            else_command: None,
            background: false,
            format: false,
            target: None,
        }
    }

    /// Sets the command run when the condition is false.
    #[must_use]
    pub fn else_command(mut self, command: impl Into<OsString>) -> Self {
        self.else_command = Some(command.into());
        self
    }

    /// Runs without blocking the tmux command queue.
    #[must_use]
    pub const fn background(mut self, enabled: bool) -> Self {
        self.background = enabled;
        self
    }

    /// Treats the condition as a tmux format expression.
    #[must_use]
    pub const fn format(mut self, enabled: bool) -> Self {
        self.format = enabled;
        self
    }

    /// Selects a target pane.
    #[must_use]
    pub const fn target(mut self, target: PaneId) -> Self {
        self.target = Some(target);
        self
    }
}

impl DisplayMessage {
    /// Creates empty display arguments.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the message or format expression.
    #[must_use]
    pub fn message(mut self, message: impl Into<OsString>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Prints expanded output rather than rendering to the status line.
    #[must_use]
    pub const fn print(mut self, enabled: bool) -> Self {
        self.print = enabled;
        self
    }

    /// Lists all format variables.
    #[must_use]
    pub const fn all_formats(mut self, enabled: bool) -> Self {
        self.all_formats = enabled;
        self
    }

    /// Includes verbose format type details.
    #[must_use]
    pub const fn verbose(mut self, enabled: bool) -> Self {
        self.verbose = enabled;
        self
    }

    /// Prevents format expansion. Requires tmux 3.4+.
    #[must_use]
    pub const fn literal(mut self, enabled: bool) -> Self {
        self.literal = enabled;
        self
    }

    /// Does not wait for input.
    #[must_use]
    pub const fn notify(mut self, enabled: bool) -> Self {
        self.notify = enabled;
        self
    }

    /// Targets a client.
    #[must_use]
    pub fn client(mut self, client: ClientName) -> Self {
        self.client = Some(client);
        self
    }

    /// Sets the status-line display duration.
    #[must_use]
    pub const fn delay_millis(mut self, delay: u32) -> Self {
        self.delay_millis = Some(delay);
        self
    }

    /// Sets an alternative output format.
    #[must_use]
    pub fn format(mut self, format: impl Into<OsString>) -> Self {
        self.format = Some(format.into());
        self
    }
}

/// Scope for hook commands.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HookScope {
    /// Global hooks (`-g`).
    Global,
    /// Session hooks (no scope flag).
    Session,
    /// Window hooks (`-w`).
    Window,
    /// Pane hooks (`-p`).
    Pane,
}

impl HookScope {
    fn flag(self) -> Option<&'static str> {
        match self {
            Self::Global => Some("-g"),
            Self::Session => None,
            Self::Window => Some("-w"),
            Self::Pane => Some("-p"),
        }
    }
}

impl Server {
    /// Runs a shell or tmux command job.
    pub async fn run_shell(&self, options: RunShell) -> Result<Option<TmuxText>> {
        let version = self.version().await?;
        if options.cwd.is_some() {
            version.require("run-shell working directory", V34)?;
        }
        if options.show_stderr {
            version.require("run-shell stderr forwarding", V36)?;
        }
        if !options.arguments.is_empty() {
            version.require("run-shell positional arguments", V37)?;
        }
        let mut command = Command::new("run-shell");
        if options.background {
            command = command.arg("-b");
        }
        if let Some(delay) = options.delay {
            command = command.arg("-d").arg(format_duration(delay));
        }
        if options.as_tmux_command {
            command = command.arg("-C");
        }
        if let Some(target) = options.target {
            command = command.target(target);
        }
        if let Some(cwd) = options.cwd {
            command = command.arg("-c").arg(cwd);
        }
        if options.show_stderr {
            command = command.arg("-E");
        }
        command = command
            .sensitive_arg(options.command)
            .args(options.arguments);
        let result = self.checked("run shell", command).await?;
        Ok((!options.background).then(|| result.stdout().clone()))
    }

    /// Waits, signals, locks, or unlocks a named channel.
    pub async fn wait_for(&self, channel: &str, action: WaitAction) -> Result<()> {
        crate::server::validate_name("wait channel", channel)?;
        let mut command = Command::new("wait-for");
        if let Some(flag) = action.flag() {
            command = command.arg(flag);
        }
        self.checked("wait for channel", command.arg(channel))
            .await
            .map(|_| ())
    }

    /// Creates or replaces a key binding.
    pub async fn bind_key(&self, binding: KeyBinding) -> Result<()> {
        let mut command = Command::new("bind-key");
        if binding.repeat {
            command = command.arg("-r");
        }
        if binding.root {
            command = command.arg("-n");
        }
        if let Some(note) = binding.note {
            command = command.arg("-N").arg(note);
        }
        if let Some(table) = binding.table {
            command = command.arg("-T").arg(table);
        }
        self.checked("bind key", command.arg(binding.key).arg(binding.command))
            .await
            .map(|_| ())
    }

    /// Removes one or more key bindings.
    pub async fn unbind_key(&self, options: UnbindKey) -> Result<()> {
        if options.key.is_none() && !options.all {
            return Err(Error::InvalidArgument {
                argument: "unbind key",
                message: "a key or all=true is required".to_owned(),
            });
        }
        let mut command = Command::new("unbind-key");
        if options.all {
            command = command.arg("-a");
        }
        if options.quiet {
            command = command.arg("-q");
        }
        if let Some(table) = options.table {
            command = command.arg("-T").arg(table);
        }
        if let Some(key) = options.key {
            command = command.arg(key);
        }
        self.checked("unbind key", command).await.map(|_| ())
    }

    /// Lists key bindings as byte-preserving lines.
    pub async fn list_keys(
        &self,
        table: Option<&str>,
        format: Option<&str>,
    ) -> Result<Vec<TmuxText>> {
        let mut command = Command::new("list-keys");
        if let Some(table) = table {
            command = command.arg("-T").arg(table);
        }
        if let Some(format) = format {
            command = command.arg("-F").arg(format);
        }
        Ok(self
            .checked("list keys", command)
            .await?
            .stdout_lines()
            .collect())
    }

    /// Lists tmux commands, optionally narrowed by name.
    pub async fn list_commands(&self, name: Option<&str>) -> Result<Vec<TmuxText>> {
        let mut command = Command::new("list-commands");
        if let Some(name) = name {
            command = command.arg(name);
        }
        Ok(self
            .checked("list commands", command)
            .await?
            .stdout_lines()
            .collect())
    }

    /// Locks every attached client.
    pub async fn lock_server(&self) -> Result<()> {
        self.checked("lock server", Command::new("lock-server"))
            .await
            .map(|_| ())
    }

    /// Applies or lists server ACL rules. Requires tmux 3.3+.
    pub async fn server_access(&self, action: ServerAccess) -> Result<Option<Vec<TmuxText>>> {
        self.version()
            .await?
            .require("server access control", V33)?;
        let mut command = Command::new("server-access");
        let listing = matches!(action, ServerAccess::List);
        match action {
            ServerAccess::Allow { user, permission } => {
                command = command.arg("-a").arg(user).arg(match permission {
                    ClientPermission::ReadOnly => "-r",
                    ClientPermission::ReadWrite => "-w",
                });
            }
            ServerAccess::Deny { user } => {
                command = command.arg("-d").arg(user);
            }
            ServerAccess::List => {
                command = command.arg("-l");
            }
        }
        let result = self.checked("server access", command).await?;
        Ok(listing.then(|| result.stdout_lines().collect()))
    }

    /// Opens a background confirmation prompt.
    pub async fn confirm_before(&self, options: ConfirmBefore) -> Result<()> {
        let version = self.version().await?;
        version.require("background confirmation", V33)?;
        if options.confirm_key.is_some() || options.default_yes {
            version.require("confirmation key/default", V34)?;
        }
        let mut command = Command::new("confirm-before").arg("-b");
        if let Some(prompt) = options.prompt {
            command = command.arg("-p").arg(prompt);
        }
        if let Some(key) = options.confirm_key {
            command = command.arg("-c").arg(key);
        }
        if options.default_yes {
            command = command.arg("-y");
        }
        if let Some(client) = options.target_client {
            command = command.arg("-t").arg(client.to_os_string());
        }
        self.checked("confirm before", command.arg(options.command))
            .await
            .map(|_| ())
    }

    /// Opens a background command prompt.
    pub async fn command_prompt(&self, options: CommandPrompt) -> Result<()> {
        let version = self.version().await?;
        version.require("background command prompt", V33)?;
        if options.literal {
            version.require("literal prompt", V36)?;
        }
        if options.backspace_exit || options.no_freeze {
            version.require("prompt backspace/no-freeze flags", V37)?;
        }
        let mut command = Command::new("command-prompt").arg("-b");
        for (enabled, flag) in [
            (options.one_key, "-1"),
            (options.key_only, "-k"),
            (options.on_input_change, "-i"),
            (options.numeric, "-N"),
            (options.expand_format, "-F"),
            (options.literal, "-l"),
            (options.backspace_exit, "-e"),
            (options.no_freeze, "-C"),
        ] {
            if enabled {
                command = command.arg(flag);
            }
        }
        if let Some(prompt) = options.prompt {
            command = command.arg("-p").arg(prompt);
        }
        if let Some(inputs) = options.inputs {
            command = command.arg("-I").arg(inputs);
        }
        if let Some(prompt_type) = options.prompt_type {
            command = command.arg("-T").arg(prompt_type.as_str());
        }
        if let Some(client) = options.target_client {
            command = command.arg("-t").arg(client.to_os_string());
        }
        self.checked("command prompt", command.arg(options.template))
            .await
            .map(|_| ())
    }

    /// Displays an attached-client menu.
    pub async fn display_menu(&self, options: Menu) -> Result<()> {
        let version = self.version().await?;
        if options.starting_choice.is_some()
            || options.border_lines.is_some()
            || options.style.is_some()
            || options.border_style.is_some()
            || options.selected_style.is_some()
        {
            version.require("advanced menu styling", V34)?;
        }
        if options.mouse {
            version.require("menu mouse mode", V35)?;
        }
        let mut command = Command::new("display-menu");
        push_optional(&mut command, "-T", options.title);
        if let Some(client) = options.target_client {
            command = command.arg("-c").arg(client.to_os_string());
        }
        if let Some(pane) = options.target_pane {
            command = command.target(pane);
        }
        push_optional(&mut command, "-x", options.x);
        push_optional(&mut command, "-y", options.y);
        push_optional(&mut command, "-C", options.starting_choice);
        push_optional(&mut command, "-b", options.border_lines);
        push_optional(&mut command, "-s", options.style);
        push_optional(&mut command, "-S", options.border_style);
        push_optional(&mut command, "-H", options.selected_style);
        if options.mouse {
            command = command.arg("-M");
        }
        if options.stay_open {
            command = command.arg("-O");
        }
        for item in options.items {
            match item {
                MenuItem::Entry {
                    label,
                    key,
                    command: item_command,
                } => {
                    command = command.arg(label).arg(key).arg(item_command);
                }
                MenuItem::Separator => {
                    command = command.arg("");
                }
            }
        }
        self.checked("display menu", command).await.map(|_| ())
    }

    /// Runs the broad server-scoped display-message operation.
    pub async fn display(&self, options: DisplayMessage) -> Result<Option<TmuxText>> {
        if options.literal {
            self.version()
                .await?
                .require("literal display message", V34)?;
        }
        let print = options.print;
        let command = display_command(options, None);
        let result = self.checked("display message", command).await?;
        Ok(print.then(|| result.stdout().clone()))
    }

    /// Starts the tmux daemon without creating a session.
    pub async fn start_server(&self) -> Result<()> {
        self.checked("start server", Command::new("start-server"))
            .await
            .map(|_| ())
    }

    /// Shows server messages, terminal capabilities, or jobs.
    pub async fn show_messages(
        &self,
        client: Option<&ClientName>,
        terminals: bool,
        jobs: bool,
    ) -> Result<Vec<TmuxText>> {
        if terminals && jobs {
            return Err(Error::InvalidArgument {
                argument: "show-messages mode",
                message: "terminals and jobs are mutually exclusive".to_owned(),
            });
        }
        let mut command = Command::new("show-messages");
        if terminals {
            command = command.arg("-T");
        }
        if jobs {
            command = command.arg("-J");
        }
        if let Some(client) = client {
            command = command.arg("-t").arg(client.to_os_string());
        }
        Ok(self
            .checked("show messages", command)
            .await?
            .stdout_lines()
            .collect())
    }

    /// Refreshes a client display and optionally requests the clipboard.
    pub async fn refresh_client(
        &self,
        client: Option<&ClientName>,
        request_clipboard: bool,
    ) -> Result<()> {
        if request_clipboard {
            self.version()
                .await?
                .require("client clipboard request", V37)?;
        }
        let mut command = Command::new("refresh-client");
        if request_clipboard {
            command = command.arg("-l");
        }
        if let Some(client) = client {
            command = command.arg("-t").arg(client.to_os_string());
        }
        self.checked("refresh client", command).await.map(|_| ())
    }

    /// Sends one key to an attached client's key handling state. Requires tmux 3.4+.
    pub async fn send_client_key(
        &self,
        client: &ClientName,
        key: impl Into<OsString>,
    ) -> Result<()> {
        self.version().await?.require("client-targeted keys", V34)?;
        self.checked(
            "send client key",
            Command::new("send-keys")
                .arg("-K")
                .arg("-c")
                .arg(client.to_os_string())
                .arg(key),
        )
        .await
        .map(|_| ())
    }

    /// Suspends an explicit or current client.
    pub async fn suspend_client(&self, client: Option<&ClientName>) -> Result<()> {
        let mut command = Command::new("suspend-client");
        if let Some(client) = client {
            command = command.arg("-t").arg(client.to_os_string());
        }
        self.checked("suspend client", command).await.map(|_| ())
    }

    /// Locks an explicit or current client.
    pub async fn lock_client(&self, client: Option<&ClientName>) -> Result<()> {
        let mut command = Command::new("lock-client");
        if let Some(client) = client {
            command = command.arg("-t").arg(client.to_os_string());
        }
        self.checked("lock client", command).await.map(|_| ())
    }

    /// Detaches an explicit or current client, optionally running a shell command afterward.
    pub async fn detach_client(
        &self,
        client: Option<&ClientName>,
        shell_command: Option<OsString>,
    ) -> Result<()> {
        let mut command = Command::new("detach-client");
        if let Some(shell_command) = shell_command {
            command = command.arg("-E").sensitive_arg(shell_command);
        }
        if let Some(client) = client {
            command = command.arg("-t").arg(client.to_os_string());
        }
        self.checked("detach client", command).await.map(|_| ())
    }

    /// Detaches all clients except an optional retained client.
    pub async fn detach_all_clients(
        &self,
        keep: Option<&ClientName>,
        shell_command: Option<OsString>,
    ) -> Result<()> {
        let mut command = Command::new("detach-client").arg("-a");
        if let Some(shell_command) = shell_command {
            command = command.arg("-E").sensitive_arg(shell_command);
        }
        if let Some(client) = keep {
            command = command.arg("-t").arg(client.to_os_string());
        }
        self.checked("detach all clients", command)
            .await
            .map(|_| ())
    }

    /// Executes one of two tmux commands based on a shell or format condition.
    pub async fn if_shell(&self, options: IfShell) -> Result<()> {
        let mut command = Command::new("if-shell");
        if options.background {
            command = command.arg("-b");
        }
        if options.format {
            command = command.arg("-F");
        }
        if let Some(target) = options.target {
            command = command.target(target);
        }
        command = command
            .sensitive_arg(options.condition)
            .sensitive_arg(options.if_command);
        if let Some(else_command) = options.else_command {
            command = command.sensitive_arg(else_command);
        }
        self.checked("if shell", command).await.map(|_| ())
    }

    /// Shows prompt history. Requires tmux 3.3+.
    pub async fn prompt_history(&self, prompt_type: Option<PromptType>) -> Result<Vec<TmuxText>> {
        self.version().await?.require("prompt history", V33)?;
        let mut command = Command::new("show-prompt-history");
        if let Some(prompt_type) = prompt_type {
            command = command.arg("-T").arg(prompt_type.as_str());
        }
        Ok(self
            .checked("show prompt history", command)
            .await?
            .stdout_lines()
            .collect())
    }

    /// Clears prompt history. Requires tmux 3.3+.
    pub async fn clear_prompt_history(&self, prompt_type: Option<PromptType>) -> Result<()> {
        self.version().await?.require("clear prompt history", V33)?;
        let mut command = Command::new("clear-prompt-history");
        if let Some(prompt_type) = prompt_type {
            command = command.arg("-T").arg(prompt_type.as_str());
        }
        self.checked("clear prompt history", command)
            .await
            .map(|_| ())
    }

    /// Runs a named hook immediately.
    pub async fn run_hook(&self, name: &str, target: Option<&str>) -> Result<()> {
        crate::server::validate_name("hook name", name)?;
        let mut command = Command::new("run-hook");
        if let Some(target) = target {
            command = command.arg("-t").arg(target);
        }
        self.checked("run hook", command.arg(name))
            .await
            .map(|_| ())
    }

    /// Sets one hook command, optionally at a sparse index.
    pub async fn set_hook(
        &self,
        scope: HookScope,
        target: Option<&str>,
        name: &str,
        index: Option<u32>,
        value: &OptionValue,
        append: bool,
    ) -> Result<()> {
        crate::server::validate_name("hook name", name)?;
        let mut command = Command::new("set-hook");
        if let Some(flag) = scope.flag() {
            command = command.arg(flag);
        }
        if append {
            command = command.arg("-a");
        }
        if let Some(target) = target {
            command = command.arg("-t").arg(target);
        }
        let name = index.map_or_else(|| name.to_owned(), |index| format!("{name}[{index}]"));
        self.checked("set hook", command.arg(name).arg(value.to_os_string()))
            .await
            .map(|_| ())
    }

    /// Unsets one hook or sparse hook index.
    pub async fn unset_hook(
        &self,
        scope: HookScope,
        target: Option<&str>,
        name: &str,
        index: Option<u32>,
    ) -> Result<()> {
        crate::server::validate_name("hook name", name)?;
        let mut command = Command::new("set-hook").arg("-u");
        if let Some(flag) = scope.flag() {
            command = command.arg(flag);
        }
        if let Some(target) = target {
            command = command.arg("-t").arg(target);
        }
        let name = index.map_or_else(|| name.to_owned(), |index| format!("{name}[{index}]"));
        self.checked("unset hook", command.arg(name))
            .await
            .map(|_| ())
    }

    /// Shows hooks as sparse option arrays.
    pub async fn show_hooks(
        &self,
        scope: HookScope,
        target: Option<&str>,
    ) -> Result<SparseOptionMap> {
        let mut command = Command::new("show-hooks");
        if let Some(flag) = scope.flag() {
            command = command.arg(flag);
        }
        if let Some(target) = target {
            command = command.arg("-t").arg(target);
        }
        let result = self.checked("show hooks", command).await?;
        SparseOptionMap::parse(result.stdout().as_bytes())
    }
}

impl Pane {
    /// Displays a popup associated with this pane.
    pub async fn display_popup(&self, options: Popup) -> Result<()> {
        if options.close_on_exit && options.close_on_success {
            return Err(Error::InvalidArgument {
                argument: "popup close behavior",
                message: "close_on_exit and close_on_success are mutually exclusive".to_owned(),
            });
        }
        let version = self.server().version().await?;
        if options.title.is_some()
            || options.border_lines.is_some()
            || options.style.is_some()
            || options.border_style.is_some()
            || !options.environment.is_empty()
            || options.no_border
        {
            version.require("popup styling and environment", V33)?;
        }
        if options.close_on_any_key || options.no_keys {
            version.require("popup close-key flags", V36)?;
        }
        let mut command = Command::new("display-popup").target(self.id());
        if options.close_existing {
            command = command.arg("-C");
        }
        if let Some(client) = options.target_client {
            command = command.arg("-c").arg(client.to_os_string());
        }
        if options.close_on_exit {
            command = command.arg("-E");
        }
        if options.close_on_success {
            command = command.arg("-E").arg("-E");
        }
        push_optional(&mut command, "-w", options.width);
        push_optional(&mut command, "-h", options.height);
        push_optional(&mut command, "-x", options.x);
        push_optional(&mut command, "-y", options.y);
        if let Some(cwd) = options.cwd {
            command = command.arg("-d").arg(cwd);
        }
        push_optional(&mut command, "-T", options.title);
        push_optional(&mut command, "-b", options.border_lines);
        push_optional(&mut command, "-s", options.style);
        push_optional(&mut command, "-S", options.border_style);
        for (name, value) in options.environment {
            command =
                command
                    .arg("-e")
                    .sensitive_arg(format!("{}={}", name, value.to_string_lossy()));
        }
        if options.no_border {
            command = command.arg("-B");
        }
        if options.close_on_any_key {
            command = command.arg("-k");
        }
        if options.no_keys {
            command = command.arg("-N");
        }
        if let Some(popup_command) = options.command {
            command = command.sensitive_arg(popup_command);
        }
        self.server()
            .checked("display popup", command)
            .await
            .map(|_| ())
    }

    /// Displays or evaluates a message in this pane's context.
    pub async fn display(&self, options: DisplayMessage) -> Result<Option<TmuxText>> {
        if options.literal {
            self.server()
                .version()
                .await?
                .require("literal display message", V34)?;
        }
        let print = options.print;
        let command = display_command(options, Some(self.id().to_string()));
        let result = self
            .server()
            .checked("display pane message", command)
            .await?;
        Ok(print.then(|| result.stdout().clone()))
    }
}

impl Window {
    /// Displays or evaluates a message in this window's context.
    pub async fn display(&self, options: DisplayMessage) -> Result<Option<TmuxText>> {
        if options.literal {
            self.server()
                .version()
                .await?
                .require("literal display message", V34)?;
        }
        let print = options.print;
        let command = display_command(options, Some(self.id().to_string()));
        let result = self
            .server()
            .checked("display window message", command)
            .await?;
        Ok(print.then(|| result.stdout().clone()))
    }
}

fn display_command(options: DisplayMessage, target: Option<String>) -> Command {
    let mut command = Command::new("display-message");
    for (enabled, flag) in [
        (options.print, "-p"),
        (options.all_formats, "-a"),
        (options.verbose, "-v"),
        (options.literal, "-l"),
        (options.notify, "-N"),
    ] {
        if enabled {
            command = command.arg(flag);
        }
    }
    if let Some(client) = options.client {
        command = command.arg("-c").arg(client.to_os_string());
    }
    if let Some(target) = target {
        command = command.arg("-t").arg(target);
    }
    if let Some(delay) = options.delay_millis {
        command = command.arg("-d").arg(delay.to_string());
    }
    push_optional(&mut command, "-F", options.format);
    if let Some(message) = options.message {
        command = command.arg(message);
    }
    command
}

fn push_optional(command: &mut Command, flag: &'static str, value: Option<OsString>) {
    if let Some(value) = value {
        *command = command.clone().arg(flag).arg(value);
    }
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs_f64();
    format!("{seconds:.6}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}
