//! tmux format descriptors and structured row decoding.

use std::collections::BTreeMap;

use crate::{Error, ReleaseVersion, Result, TmuxText, TmuxVersion};

pub(crate) const FORMAT_SEPARATOR: &str = "\u{241e}";

/// The tmux context needed to resolve a format token.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FormatScope {
    /// Server-global data available to every listing.
    Universal,
    /// Session data.
    Session,
    /// Window and window-link data.
    Window,
    /// Pane data.
    Pane,
    /// Attached-client data.
    Client,
    /// Paste-buffer data.
    Buffer,
    /// Runtime event data that is not safe in ordinary listings.
    Event,
    /// A command-specific context.
    Context,
}

/// The intended decoded representation of a format token.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DecodedType {
    /// Byte-preserving text.
    Text,
    /// A tmux `0` or `1` flag.
    Bool,
    /// A signed integer.
    Signed,
    /// An unsigned integer.
    Unsigned,
    /// A Unix timestamp.
    Timestamp,
    /// A validated tmux identity.
    Id,
}

/// A value decoded according to a [`FormatDescriptor`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormatValue<'a> {
    /// Byte-preserving text.
    Text(&'a TmuxText),
    /// A boolean flag.
    Bool(bool),
    /// A signed integer.
    Signed(i64),
    /// An unsigned integer.
    Unsigned(u64),
    /// A Unix timestamp.
    Timestamp(u64),
    /// An identity in its validated textual representation.
    Id(&'a str),
}

/// Metadata for one tmux format variable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FormatDescriptor {
    token: &'static str,
    scope: FormatScope,
    decoded: DecodedType,
    minimum: ReleaseVersion,
}

impl FormatDescriptor {
    const fn new(
        token: &'static str,
        scope: FormatScope,
        decoded: DecodedType,
        minimum: ReleaseVersion,
    ) -> Self {
        Self {
            token,
            scope,
            decoded,
            minimum,
        }
    }

    /// Returns the tmux token without `#{}` delimiters.
    #[must_use]
    pub const fn token(self) -> &'static str {
        self.token
    }

    /// Returns the context required by this token.
    #[must_use]
    pub const fn scope(self) -> FormatScope {
        self.scope
    }

    /// Returns the intended decoded representation.
    #[must_use]
    pub const fn decoded_type(self) -> DecodedType {
        self.decoded
    }

    /// Returns the oldest tmux release that supplies the token.
    #[must_use]
    pub const fn minimum_version(self) -> ReleaseVersion {
        self.minimum
    }

    /// Decodes a non-empty value using this descriptor's declared type.
    pub fn decode(self, value: &TmuxText) -> Result<Option<FormatValue<'_>>> {
        if value.is_empty() {
            return Ok(None);
        }
        let parsed = match self.decoded {
            DecodedType::Text => FormatValue::Text(value),
            DecodedType::Bool => match value.as_bytes() {
                b"0" => FormatValue::Bool(false),
                b"1" => FormatValue::Bool(true),
                _ => {
                    return Err(Error::Decode {
                        context: "format boolean",
                        message: format!("{} was not 0 or 1", self.token),
                    });
                }
            },
            DecodedType::Signed => FormatValue::Signed(parse_number(value, self.token)?),
            DecodedType::Unsigned => FormatValue::Unsigned(parse_number(value, self.token)?),
            DecodedType::Timestamp => FormatValue::Timestamp(parse_number(value, self.token)?),
            DecodedType::Id => {
                let value = value.to_str().map_err(|source| Error::Decode {
                    context: "format identity",
                    message: format!("{}: {source}", self.token),
                })?;
                FormatValue::Id(value)
            }
        };
        Ok(Some(parsed))
    }
}

const BASE: ReleaseVersion = ReleaseVersion::MINIMUM;
const V33: ReleaseVersion = ReleaseVersion::new(3, 3, None);
const V37: ReleaseVersion = ReleaseVersion::new(3, 7, None);

macro_rules! descriptors {
    ($(($token:literal, $scope:ident, $decoded:ident, $version:expr)),+ $(,)?) => {
        &[
            $(FormatDescriptor::new(
                $token,
                FormatScope::$scope,
                DecodedType::$decoded,
                $version,
            )),+
        ]
    };
}

static FORMAT_CATALOG: &[FormatDescriptor] = descriptors![
    ("pid", Universal, Unsigned, BASE),
    ("config_files", Universal, Text, BASE),
    ("host", Universal, Text, BASE),
    ("host_short", Universal, Text, BASE),
    ("line", Universal, Unsigned, BASE),
    ("next_session_id", Universal, Unsigned, BASE),
    ("socket_path", Universal, Text, BASE),
    ("start_time", Universal, Timestamp, BASE),
    ("uid", Universal, Unsigned, BASE),
    ("user", Universal, Text, BASE),
    ("version", Universal, Text, BASE),
    ("session_id", Session, Id, BASE),
    ("session_name", Session, Text, BASE),
    ("session_windows", Session, Unsigned, BASE),
    ("session_width", Session, Unsigned, BASE),
    ("session_height", Session, Unsigned, BASE),
    ("session_created", Session, Timestamp, BASE),
    ("session_created_string", Session, Text, BASE),
    ("session_attached", Session, Unsigned, BASE),
    ("session_activity", Session, Timestamp, BASE),
    ("session_alerts", Session, Text, BASE),
    ("session_attached_list", Session, Text, BASE),
    ("session_format", Session, Text, BASE),
    ("session_group", Session, Text, BASE),
    ("session_group_attached", Session, Unsigned, BASE),
    ("session_group_attached_list", Session, Text, BASE),
    ("session_group_list", Session, Text, BASE),
    ("session_group_many_attached", Session, Bool, BASE),
    ("session_group_size", Session, Unsigned, BASE),
    ("session_grouped", Session, Bool, BASE),
    ("session_last_attached", Session, Timestamp, BASE),
    ("session_many_attached", Session, Bool, BASE),
    ("session_marked", Session, Bool, BASE),
    ("session_path", Session, Text, BASE),
    ("session_stack", Session, Text, BASE),
    ("active_window_index", Session, Unsigned, BASE),
    ("last_window_index", Session, Unsigned, BASE),
    ("window_id", Window, Id, BASE),
    ("window_index", Window, Unsigned, BASE),
    ("window_name", Window, Text, BASE),
    ("window_width", Window, Unsigned, BASE),
    ("window_height", Window, Unsigned, BASE),
    ("window_layout", Window, Text, BASE),
    ("window_visible_layout", Window, Text, BASE),
    ("window_panes", Window, Unsigned, BASE),
    ("window_flags", Window, Text, BASE),
    ("window_active", Window, Bool, BASE),
    ("window_bell_flag", Window, Bool, BASE),
    ("window_activity_flag", Window, Bool, BASE),
    ("window_silence_flag", Window, Bool, BASE),
    ("window_zoomed_flag", Window, Bool, BASE),
    ("window_active_clients", Window, Unsigned, BASE),
    ("window_active_clients_list", Window, Text, BASE),
    ("window_active_sessions", Window, Unsigned, BASE),
    ("window_active_sessions_list", Window, Text, BASE),
    ("window_activity", Window, Timestamp, BASE),
    ("window_bigger", Window, Bool, BASE),
    ("window_cell_height", Window, Unsigned, BASE),
    ("window_cell_width", Window, Unsigned, BASE),
    ("window_end_flag", Window, Bool, BASE),
    ("window_format", Window, Text, BASE),
    ("window_last_flag", Window, Bool, BASE),
    ("window_linked", Window, Bool, BASE),
    ("window_linked_sessions", Window, Unsigned, BASE),
    ("window_linked_sessions_list", Window, Text, BASE),
    ("window_marked_flag", Window, Bool, BASE),
    ("window_offset_x", Window, Signed, BASE),
    ("window_offset_y", Window, Signed, BASE),
    ("window_raw_flags", Window, Text, BASE),
    ("window_stack_index", Window, Unsigned, BASE),
    ("window_start_flag", Window, Bool, BASE),
    ("pane_id", Pane, Id, BASE),
    ("pane_index", Pane, Unsigned, BASE),
    ("pane_width", Pane, Unsigned, BASE),
    ("pane_height", Pane, Unsigned, BASE),
    ("pane_title", Pane, Text, BASE),
    ("pane_active", Pane, Bool, BASE),
    ("pane_dead", Pane, Bool, BASE),
    ("pane_dead_status", Pane, Signed, BASE),
    ("pane_dead_signal", Pane, Signed, V33),
    ("pane_dead_time", Pane, Timestamp, V33),
    ("pane_in_mode", Pane, Bool, BASE),
    ("pane_synchronized", Pane, Bool, BASE),
    ("pane_tty", Pane, Text, BASE),
    ("pane_pid", Pane, Unsigned, BASE),
    ("pane_start_command", Pane, Text, BASE),
    ("pane_start_path", Pane, Text, BASE),
    ("pane_current_path", Pane, Text, BASE),
    ("pane_current_command", Pane, Text, BASE),
    ("pane_input_off", Pane, Bool, BASE),
    ("pane_unseen_changes", Pane, Bool, BASE),
    ("pane_at_left", Pane, Bool, BASE),
    ("pane_at_right", Pane, Bool, BASE),
    ("pane_at_top", Pane, Bool, BASE),
    ("pane_at_bottom", Pane, Bool, BASE),
    ("history_size", Pane, Unsigned, BASE),
    ("history_limit", Pane, Unsigned, BASE),
    ("history_bytes", Pane, Unsigned, BASE),
    ("alternate_saved_x", Pane, Unsigned, BASE),
    ("alternate_saved_y", Pane, Unsigned, BASE),
    ("cursor_x", Pane, Unsigned, BASE),
    ("cursor_y", Pane, Unsigned, BASE),
    ("scroll_region_upper", Pane, Unsigned, BASE),
    ("scroll_region_lower", Pane, Unsigned, BASE),
    ("alternate_on", Pane, Bool, BASE),
    ("cursor_flag", Pane, Bool, BASE),
    ("insert_flag", Pane, Bool, BASE),
    ("keypad_cursor_flag", Pane, Bool, BASE),
    ("keypad_flag", Pane, Bool, BASE),
    ("wrap_flag", Pane, Bool, BASE),
    ("mouse_standard_flag", Pane, Bool, BASE),
    ("mouse_button_flag", Pane, Bool, BASE),
    ("mouse_any_flag", Pane, Bool, BASE),
    ("mouse_utf8_flag", Pane, Bool, BASE),
    ("cursor_character", Pane, Text, BASE),
    ("mouse_all_flag", Pane, Bool, BASE),
    ("mouse_sgr_flag", Pane, Bool, BASE),
    ("origin_flag", Pane, Bool, BASE),
    ("pane_bg", Pane, Text, BASE),
    ("pane_bottom", Pane, Signed, BASE),
    ("pane_fg", Pane, Text, BASE),
    ("pane_format", Pane, Text, BASE),
    ("pane_last", Pane, Bool, BASE),
    ("pane_left", Pane, Signed, BASE),
    ("pane_marked", Pane, Bool, BASE),
    ("pane_marked_set", Pane, Bool, BASE),
    ("pane_mode", Pane, Text, BASE),
    ("pane_path", Pane, Text, BASE),
    ("pane_pipe", Pane, Bool, BASE),
    ("pane_right", Pane, Signed, BASE),
    ("pane_search_string", Pane, Text, BASE),
    ("pane_tabs", Pane, Text, BASE),
    ("pane_top", Pane, Signed, BASE),
    ("pane_flags", Pane, Text, V37),
    ("pane_floating_flag", Pane, Bool, V37),
    ("pane_x", Pane, Signed, V37),
    ("pane_y", Pane, Signed, V37),
    ("pane_z", Pane, Signed, V37),
    ("pane_zoomed_flag", Pane, Bool, V37),
    ("pane_pb_progress", Pane, Unsigned, V37),
    ("pane_pb_state", Pane, Text, V37),
    ("pane_pipe_pid", Pane, Unsigned, V37),
    ("bracket_paste_flag", Pane, Bool, V37),
    ("synchronized_output_flag", Pane, Bool, V37),
    ("client_name", Client, Text, BASE),
    ("client_tty", Client, Text, BASE),
    ("client_termname", Client, Text, BASE),
    ("client_session", Client, Text, BASE),
    ("client_last_session", Client, Text, BASE),
    ("client_created", Client, Timestamp, BASE),
    ("client_activity", Client, Timestamp, BASE),
    ("client_width", Client, Unsigned, BASE),
    ("client_height", Client, Unsigned, BASE),
    ("client_pid", Client, Unsigned, BASE),
    ("client_prefix", Client, Bool, BASE),
    ("client_utf8", Client, Bool, BASE),
    ("client_readonly", Client, Bool, BASE),
    ("client_control_mode", Client, Bool, BASE),
    ("client_flags", Client, Text, BASE),
    ("client_cell_height", Client, Unsigned, BASE),
    ("client_cell_width", Client, Unsigned, BASE),
    ("client_discarded", Client, Unsigned, BASE),
    ("client_key_table", Client, Text, BASE),
    ("client_mode_format", Client, Text, BASE),
    ("client_termfeatures", Client, Text, BASE),
    ("client_termtype", Client, Text, BASE),
    ("client_uid", Client, Unsigned, BASE),
    ("client_user", Client, Text, BASE),
    ("client_written", Client, Unsigned, BASE),
    ("buffer_name", Buffer, Text, BASE),
    ("buffer_size", Buffer, Unsigned, BASE),
    ("buffer_sample", Buffer, Text, BASE),
    ("copy_cursor_line", Event, Unsigned, BASE),
    ("copy_cursor_word", Event, Text, BASE),
    ("copy_cursor_x", Event, Unsigned, BASE),
    ("copy_cursor_y", Event, Unsigned, BASE),
    ("scroll_position", Event, Signed, BASE),
    ("selection_end_x", Event, Signed, BASE),
    ("selection_end_y", Event, Signed, BASE),
    ("selection_start_x", Event, Signed, BASE),
    ("selection_start_y", Event, Signed, BASE),
    ("command_list_alias", Context, Text, BASE),
    ("command_list_name", Context, Text, BASE),
    ("command_list_usage", Context, Text, BASE),
    ("current_file", Context, Text, BASE),
    ("search_match", Context, Text, BASE),
];

/// Returns the single catalog used for queries and snapshot accessors.
#[must_use]
pub const fn format_catalog() -> &'static [FormatDescriptor] {
    FORMAT_CATALOG
}

/// Looks up a known descriptor by exact token name.
#[must_use]
pub fn format_descriptor(token: &str) -> Option<&'static FormatDescriptor> {
    FORMAT_CATALOG
        .iter()
        .find(|descriptor| descriptor.token == token)
}

/// A known catalog token or an explicitly requested custom tmux token.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum FormatToken {
    /// A catalog entry.
    Known(&'static FormatDescriptor),
    /// A caller-defined token, without `#{}`.
    Custom(String),
}

impl FormatToken {
    /// Validates a custom tmux format token.
    pub fn custom(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty()
            || value.contains(['{', '}', FORMAT_SEPARATOR.chars().next().unwrap_or('\0')])
        {
            return Err(Error::InvalidArgument {
                argument: "format token",
                message: "must be non-empty and contain no braces or field separator".to_owned(),
            });
        }
        Ok(Self::Custom(value))
    }

    /// Returns the token without delimiters.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Known(descriptor) => descriptor.token(),
            Self::Custom(value) => value,
        }
    }
}

/// A raw tmux native filter expression passed through via `-f`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NativeFilter(String);

impl NativeFilter {
    /// Creates a non-empty native filter expression.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(Error::InvalidArgument {
                argument: "native filter",
                message: "must not be empty".to_owned(),
            });
        }
        Ok(Self(value))
    }

    /// Returns the tmux expression.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) fn fields_for_listing(
    scopes: &[FormatScope],
    version: &TmuxVersion,
) -> Vec<&'static FormatDescriptor> {
    FORMAT_CATALOG
        .iter()
        .filter(|field| scopes.contains(&field.scope) && version.meets(field.minimum))
        .collect()
}

pub(crate) fn render_format(fields: &[&FormatDescriptor]) -> String {
    fields
        .iter()
        .map(|field| format!("#{{{}}}", field.token))
        .collect::<Vec<_>>()
        .join(FORMAT_SEPARATOR)
}

pub(crate) fn parse_rows(
    bytes: &[u8],
    fields: &[&FormatDescriptor],
) -> Result<Vec<BTreeMap<&'static str, TmuxText>>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let separator = FORMAT_SEPARATOR.as_bytes();
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let values = split_fields(line, separator);
            if values.len() != fields.len() {
                return Err(Error::Decode {
                    context: "tmux format row",
                    message: format!(
                        "expected {} fields, received {}",
                        fields.len(),
                        values.len()
                    ),
                });
            }
            Ok(fields
                .iter()
                .zip(values)
                .map(|(field, value)| (field.token, TmuxText::new(value.to_vec())))
                .collect())
        })
        .collect()
}

fn split_fields<'a>(line: &'a [u8], separator: &[u8]) -> Vec<&'a [u8]> {
    let mut fields = Vec::new();
    let mut start = 0;
    while let Some(offset) = line[start..]
        .windows(separator.len())
        .position(|window| window == separator)
    {
        let end = start + offset;
        fields.push(&line[start..end]);
        start = end + separator.len();
    }
    fields.push(&line[start..]);
    fields
}

fn parse_number<T>(value: &TmuxText, token: &'static str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let value = value.to_str().map_err(|source| Error::Decode {
        context: "numeric format",
        message: format!("{token}: {source}"),
    })?;
    value.parse().map_err(|source| Error::Decode {
        context: "numeric format",
        message: format!("{token}: {source}"),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        FORMAT_SEPARATOR, FormatScope, fields_for_listing, format_descriptor, parse_rows,
        render_format,
    };
    use crate::{TmuxText, TmuxVersion};

    #[test]
    fn version_gates_new_fields() {
        let Ok(version) = TmuxVersion::parse("3.6") else {
            return;
        };
        let fields = fields_for_listing(&[FormatScope::Pane], &version);
        assert!(!fields.iter().any(|field| field.token() == "pane_flags"));
    }

    #[test]
    fn format_rows_round_trip() {
        let Ok(version) = TmuxVersion::parse("3.2a") else {
            return;
        };
        let fields = fields_for_listing(&[FormatScope::Session], &version);
        let fields = &fields[..2];
        assert!(render_format(fields).contains("#{session_id}"));
        let output = format!("$1{FORMAT_SEPARATOR}name\n");
        let Ok(rows) = parse_rows(output.as_bytes(), fields) else {
            return;
        };
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn malformed_rows_and_values_are_rejected() {
        let Ok(version) = TmuxVersion::parse("3.2a") else {
            return;
        };
        let fields = fields_for_listing(&[FormatScope::Session], &version);
        assert!(parse_rows(b"only-one-field\n", &fields[..2]).is_err());

        let Some(boolean) = format_descriptor("pane_active") else {
            return;
        };
        assert!(boolean.decode(&TmuxText::from("not-a-boolean")).is_err());
    }
}
