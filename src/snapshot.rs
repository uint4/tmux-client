//! Owned snapshots hydrated from tmux format rows.

use std::collections::BTreeMap;
use std::str::FromStr;

use crate::{
    ClientName, Error, FormatDescriptor, FormatValue, PaneId, Result, SessionId, SessionName,
    TmuxText, WindowId,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Snapshot {
    fields: BTreeMap<&'static str, TmuxText>,
}

impl Snapshot {
    fn new(fields: BTreeMap<&'static str, TmuxText>) -> Self {
        Self { fields }
    }

    fn get(&self, token: &str) -> Option<&TmuxText> {
        self.fields.get(token)
    }

    fn parse<T>(&self, token: &'static str) -> Result<Option<T>>
    where
        T: FromStr,
        T::Err: std::fmt::Display,
    {
        let Some(value) = self.get(token) else {
            return Ok(None);
        };
        if value.is_empty() {
            return Ok(None);
        }
        let value = value.to_str().map_err(|source| Error::Decode {
            context: "snapshot UTF-8 field",
            message: format!("{token}: {source}"),
        })?;
        value.parse().map(Some).map_err(|source| Error::Decode {
            context: "snapshot typed field",
            message: format!("{token}: {source}"),
        })
    }

    fn bool(&self, token: &'static str) -> Result<Option<bool>> {
        match self.get(token).map(TmuxText::as_bytes) {
            None | Some(b"") => Ok(None),
            Some(b"0") => Ok(Some(false)),
            Some(b"1") => Ok(Some(true)),
            Some(_) => Err(Error::Decode {
                context: "snapshot boolean field",
                message: format!("{token}: expected 0 or 1"),
            }),
        }
    }
}

macro_rules! snapshot_type {
    ($name:ident) => {
        #[doc = concat!("An owned tmux ", stringify!($name), " field snapshot.")]
        #[derive(Clone, Debug, Default, Eq, PartialEq)]
        pub struct $name(Snapshot);

        impl $name {
            pub(crate) fn new(fields: BTreeMap<&'static str, TmuxText>) -> Self {
                Self(Snapshot::new(fields))
            }

            /// Returns a byte-preserving field by tmux token name.
            #[must_use]
            pub fn get(&self, token: &str) -> Option<&TmuxText> {
                self.0.get(token)
            }

            /// Iterates over all fields selected for the connected tmux version.
            pub fn iter(&self) -> impl Iterator<Item = (&'static str, &TmuxText)> {
                self.0.fields.iter().map(|(token, value)| (*token, value))
            }
        }
    };
}

snapshot_type!(SessionSnapshot);
snapshot_type!(WindowSnapshot);
snapshot_type!(PaneSnapshot);
snapshot_type!(ClientSnapshot);

/// Catalog-driven field access shared by every snapshot type.
pub trait SnapshotFields {
    /// Returns the raw value for a known descriptor.
    fn raw_field(&self, descriptor: &FormatDescriptor) -> Option<&TmuxText>;

    /// Decodes the value using the descriptor catalog's declared type.
    fn decoded_field(&self, descriptor: &FormatDescriptor) -> Result<Option<FormatValue<'_>>> {
        let Some(value) = self.raw_field(descriptor) else {
            return Ok(None);
        };
        descriptor.decode(value)
    }
}

macro_rules! snapshot_fields {
    ($($name:ident),+ $(,)?) => {
        $(
            impl SnapshotFields for $name {
                fn raw_field(&self, descriptor: &FormatDescriptor) -> Option<&TmuxText> {
                    self.get(descriptor.token())
                }
            }
        )+
    };
}

snapshot_fields!(
    SessionSnapshot,
    WindowSnapshot,
    PaneSnapshot,
    ClientSnapshot
);

macro_rules! unsigned_accessors {
    ($(($name:ident, $token:literal, $docs:literal)),+ $(,)?) => {
        $(
            #[doc = $docs]
            pub fn $name(&self) -> Result<Option<u64>> {
                self.0.parse($token)
            }
        )+
    };
}

macro_rules! signed_accessors {
    ($(($name:ident, $token:literal, $docs:literal)),+ $(,)?) => {
        $(
            #[doc = $docs]
            pub fn $name(&self) -> Result<Option<i64>> {
                self.0.parse($token)
            }
        )+
    };
}

macro_rules! bool_accessors {
    ($(($name:ident, $token:literal, $docs:literal)),+ $(,)?) => {
        $(
            #[doc = $docs]
            pub fn $name(&self) -> Result<Option<bool>> {
                self.0.bool($token)
            }
        )+
    };
}

macro_rules! text_accessors {
    ($(($name:ident, $token:literal, $docs:literal)),+ $(,)?) => {
        $(
            #[doc = $docs]
            #[must_use]
            pub fn $name(&self) -> Option<&TmuxText> {
                self.get($token)
            }
        )+
    };
}

impl SessionSnapshot {
    /// Returns the immutable session ID.
    pub fn id(&self) -> Result<Option<SessionId>> {
        self.0.parse("session_id")
    }

    /// Returns the session name.
    pub fn name(&self) -> Result<Option<SessionName>> {
        let Some(value) = self.get("session_name") else {
            return Ok(None);
        };
        if value.is_empty() {
            return Ok(None);
        }
        SessionName::new(value.to_string_lossy().into_owned()).map(Some)
    }

    /// Returns the number of attached clients.
    pub fn attached_clients(&self) -> Result<Option<u32>> {
        self.0.parse("session_attached")
    }

    /// Returns the number of linked windows.
    pub fn window_count(&self) -> Result<Option<u32>> {
        self.0.parse("session_windows")
    }

    unsigned_accessors![
        (width, "session_width", "Returns the session width."),
        (height, "session_height", "Returns the session height."),
        (
            created,
            "session_created",
            "Returns the creation Unix timestamp."
        ),
        (
            active_window_index,
            "active_window_index",
            "Returns the active window index."
        ),
        (
            last_window_index,
            "last_window_index",
            "Returns the previous window index."
        ),
    ];

    text_accessors![
        (
            created_string,
            "session_created_string",
            "Returns tmux's formatted creation time."
        ),
        (group, "session_group", "Returns the session group name."),
    ];
}

impl WindowSnapshot {
    /// Returns the immutable window ID.
    pub fn id(&self) -> Result<Option<WindowId>> {
        self.0.parse("window_id")
    }

    /// Returns the index of this link in its session.
    pub fn index(&self) -> Result<Option<u32>> {
        self.0.parse("window_index")
    }

    /// Returns the window name.
    #[must_use]
    pub fn name(&self) -> Option<&TmuxText> {
        self.get("window_name")
    }

    /// Returns whether this link is the active window.
    pub fn active(&self) -> Result<Option<bool>> {
        self.0.bool("window_active")
    }

    unsigned_accessors![
        (width, "window_width", "Returns the window width."),
        (height, "window_height", "Returns the window height."),
        (pane_count, "window_panes", "Returns the pane count."),
    ];

    text_accessors![
        (
            layout,
            "window_layout",
            "Returns the serialized window layout."
        ),
        (
            visible_layout,
            "window_visible_layout",
            "Returns the visible serialized layout."
        ),
        (flags, "window_flags", "Returns the window-link flags."),
    ];

    bool_accessors![
        (
            bell,
            "window_bell_flag",
            "Returns whether a bell alert is set."
        ),
        (
            activity,
            "window_activity_flag",
            "Returns whether an activity alert is set."
        ),
        (
            silence,
            "window_silence_flag",
            "Returns whether a silence alert is set."
        ),
        (
            zoomed,
            "window_zoomed_flag",
            "Returns whether the window is zoomed."
        ),
    ];
}

impl PaneSnapshot {
    /// Returns the immutable pane ID.
    pub fn id(&self) -> Result<Option<PaneId>> {
        self.0.parse("pane_id")
    }

    /// Returns the pane index in its window.
    pub fn index(&self) -> Result<Option<u32>> {
        self.0.parse("pane_index")
    }

    /// Returns whether this is the active pane.
    pub fn active(&self) -> Result<Option<bool>> {
        self.0.bool("pane_active")
    }

    /// Returns whether the process running in the pane is dead.
    pub fn dead(&self) -> Result<Option<bool>> {
        self.0.bool("pane_dead")
    }

    /// Returns the pane's current path.
    #[must_use]
    pub fn current_path(&self) -> Option<&TmuxText> {
        self.get("pane_current_path")
    }

    /// Returns the pane's current command.
    #[must_use]
    pub fn current_command(&self) -> Option<&TmuxText> {
        self.get("pane_current_command")
    }

    unsigned_accessors![
        (width, "pane_width", "Returns the pane width."),
        (height, "pane_height", "Returns the pane height."),
        (pid, "pane_pid", "Returns the pane process ID."),
        (
            history_size,
            "history_size",
            "Returns populated history lines."
        ),
        (history_limit, "history_limit", "Returns the history limit."),
        (history_bytes, "history_bytes", "Returns history bytes."),
        (cursor_x, "cursor_x", "Returns the cursor x coordinate."),
        (cursor_y, "cursor_y", "Returns the cursor y coordinate."),
        (
            scroll_region_upper,
            "scroll_region_upper",
            "Returns the upper scroll boundary."
        ),
        (
            scroll_region_lower,
            "scroll_region_lower",
            "Returns the lower scroll boundary."
        ),
        (
            paste_buffer_progress,
            "pane_pb_progress",
            "Returns paste-buffer progress on tmux 3.7+."
        ),
        (
            pipe_pid,
            "pane_pipe_pid",
            "Returns the pipe process ID on tmux 3.7+."
        ),
    ];

    signed_accessors![
        (
            dead_status,
            "pane_dead_status",
            "Returns the dead process status."
        ),
        (
            dead_signal,
            "pane_dead_signal",
            "Returns the dead process signal on tmux 3.3+."
        ),
        (
            dead_time,
            "pane_dead_time",
            "Returns the death Unix timestamp on tmux 3.3+."
        ),
        (
            x,
            "pane_x",
            "Returns the floating x coordinate on tmux 3.7+."
        ),
        (
            y,
            "pane_y",
            "Returns the floating y coordinate on tmux 3.7+."
        ),
        (
            z,
            "pane_z",
            "Returns the floating z coordinate on tmux 3.7+."
        ),
    ];

    text_accessors![
        (title, "pane_title", "Returns the pane title."),
        (tty, "pane_tty", "Returns the pane TTY."),
        (
            start_command,
            "pane_start_command",
            "Returns the initial command."
        ),
        (start_path, "pane_start_path", "Returns the initial path."),
        (flags, "pane_flags", "Returns pane flags on tmux 3.7+."),
        (
            paste_buffer_state,
            "pane_pb_state",
            "Returns paste-buffer state on tmux 3.7+."
        ),
    ];

    bool_accessors![
        (in_mode, "pane_in_mode", "Returns whether a mode is active."),
        (
            synchronized,
            "pane_synchronized",
            "Returns synchronized-input state."
        ),
        (
            input_off,
            "pane_input_off",
            "Returns whether pane input is disabled."
        ),
        (
            unseen_changes,
            "pane_unseen_changes",
            "Returns whether changes are unseen."
        ),
        (
            at_left,
            "pane_at_left",
            "Returns whether the pane touches the left edge."
        ),
        (
            at_right,
            "pane_at_right",
            "Returns whether the pane touches the right edge."
        ),
        (
            at_top,
            "pane_at_top",
            "Returns whether the pane touches the top edge."
        ),
        (
            at_bottom,
            "pane_at_bottom",
            "Returns whether the pane touches the bottom edge."
        ),
        (
            alternate_on,
            "alternate_on",
            "Returns alternate-screen state."
        ),
        (cursor_visible, "cursor_flag", "Returns cursor visibility."),
        (insert_mode, "insert_flag", "Returns insert mode."),
        (
            keypad_cursor,
            "keypad_cursor_flag",
            "Returns keypad cursor mode."
        ),
        (keypad, "keypad_flag", "Returns keypad mode."),
        (wrap, "wrap_flag", "Returns line-wrap mode."),
        (
            mouse_standard,
            "mouse_standard_flag",
            "Returns standard mouse mode."
        ),
        (
            mouse_button,
            "mouse_button_flag",
            "Returns button mouse mode."
        ),
        (mouse_any, "mouse_any_flag", "Returns any-event mouse mode."),
        (mouse_utf8, "mouse_utf8_flag", "Returns UTF-8 mouse mode."),
        (
            floating,
            "pane_floating_flag",
            "Returns floating-pane state on tmux 3.7+."
        ),
        (
            zoomed,
            "pane_zoomed_flag",
            "Returns pane zoom state on tmux 3.7+."
        ),
        (
            bracketed_paste,
            "bracket_paste_flag",
            "Returns bracketed-paste state on tmux 3.7+."
        ),
        (
            synchronized_output,
            "synchronized_output_flag",
            "Returns synchronized-output state on tmux 3.7+."
        ),
    ];
}

impl ClientSnapshot {
    /// Returns the tmux client name.
    pub fn name(&self) -> Result<Option<ClientName>> {
        let Some(value) = self.get("client_name") else {
            return Ok(None);
        };
        if value.is_empty() {
            return Ok(None);
        }
        ClientName::new(value.clone()).map(Some)
    }

    /// Returns the attached session name, if any.
    pub fn session_name(&self) -> Result<Option<SessionName>> {
        let Some(value) = self.get("client_session") else {
            return Ok(None);
        };
        if value.is_empty() {
            return Ok(None);
        }
        SessionName::new(value.to_string_lossy().into_owned()).map(Some)
    }

    /// Returns whether this client is read-only.
    pub fn read_only(&self) -> Result<Option<bool>> {
        self.0.bool("client_readonly")
    }

    unsigned_accessors![
        (
            created,
            "client_created",
            "Returns the creation Unix timestamp."
        ),
        (
            activity,
            "client_activity",
            "Returns the last-activity Unix timestamp."
        ),
        (width, "client_width", "Returns the terminal width."),
        (height, "client_height", "Returns the terminal height."),
        (pid, "client_pid", "Returns the client process ID."),
    ];

    text_accessors![
        (tty, "client_tty", "Returns the client TTY."),
        (
            terminal_name,
            "client_termname",
            "Returns the terminal name."
        ),
        (
            last_session,
            "client_last_session",
            "Returns the previous session name."
        ),
        (flags, "client_flags", "Returns client flags."),
    ];

    bool_accessors![
        (
            prefix,
            "client_prefix",
            "Returns whether the prefix is active."
        ),
        (utf8, "client_utf8", "Returns UTF-8 support."),
        (
            control_mode,
            "client_control_mode",
            "Returns control-mode state."
        ),
    ];
}
