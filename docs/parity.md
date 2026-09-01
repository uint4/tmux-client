# Python libtmux parity ledger

Baseline: Python libtmux **0.62.0**, commit [`a7ff6b6cd687e0c96874a839f8cbb2b797623680`](https://github.com/tmux-python/libtmux/commit/a7ff6b6cd687e0c96874a839f8cbb2b797623680).

This ledger tracks public runtime behavior, not Python syntax. `I` means a Rust-native implementation exists; `E` means the item is intentionally excluded by the fixed project scope. A row is not complete merely because `Server::cmd` could spell the underlying tmux command.

Validation references:

- `U`: focused unit tests in `src/**`.
- `R`: real-tmux scenarios in `tests/integration.rs`.
- `M`: version matrix in `.github/workflows/ci.yml`.
- `D`: rustdoc/example compilation.

## Foundation and object model

| Baseline capability | Rust API | Status | Validation |
|---|---|---:|---|
| Server construction: binary, `-L`/`-S`, config, 88/256 colors, environment | `ServerBuilder`, `ServerConfig`, `Socket`, `ColorMode` | I | U, R |
| Detect server from `TMUX` | `Server::from_environment` | I | U |
| Detect current pane/session/window from `TMUX_PANE` | `pane_from_environment`, object `from_environment` methods | I | M |
| Raw command result and non-zero exit status | `Command`, `CommandResult`, `Server::cmd` | I | U, R |
| Checked typed operations | non-exhaustive `Error`, `CommandResult::ensure_success` | I | U, R |
| Raw stdout/stderr lines | `TmuxText`, `stdout_lines`, `stderr_lines` | I | U |
| Version detection and comparisons | `TmuxVersion`, `ReleaseVersion`, cached `Server::version` | I | U, M |
| Post-release feature gating | `TmuxVersion::require`, `Error::Unsupported` | I | U, R, M |
| Object identity and targets | validated IDs, names, targets, `WindowLink` | I | U, R |
| Snapshot refresh | `refresh(&mut self)` on all discovered handles | I | R |
| Context-manager destruction | async `with_session`, `with_window`, `with_pane` | I | R |
| Ordinary object destruction | non-destructive Rust `Drop` | I | U |
| Python dictionary access (`get`, `__getitem__`) | snapshot typed accessors and `get` | E | Fixed exclusion |
| Python equality/repr | identity-aware `Eq` and redacted `Debug` | I | U |

## Formats, discovery, and querying

| Baseline capability | Rust API | Status | Validation |
|---|---|---:|---|
| Runtime format token catalog | `format_catalog`, `FormatDescriptor`, `DecodedType`, `FormatScope` | I | U, M |
| Token scope and minimum-version filtering | listing formatter in `formats` | I | U, R, M |
| Session/window/pane/client/buffer format rows | owned typed snapshots plus raw `get`/`iter` | I | U, R |
| Unknown caller-selected format variables | validated `FormatToken::custom` | I | U |
| Hierarchy discovery | `sessions`, `windows`, `panes`, `clients`, `buffers` | I | R |
| Loud discovery failures | `Result<Vec<T>>` collection methods | I | R |
| Lenient compatibility discovery | explicit `*_or_empty` methods | I | R |
| Active/parent/child traversal | session/window/pane/client traversal methods | I | R |
| Duplicate server-wide linked windows | distinct `WindowId` and `WindowLink`; duplicate rows retained | I | R |
| Exact object lookup and multiple-match errors | typed lookup methods and `ObjectNotFound`/`MultipleObjects` | I | R |
| Python `QueryList` comparisons (`exact`, `ne/noeq`, `<`, `<=`, `>`, `>=`) | `FilterExpr::{equal,not_equal,compare}` | I | U |
| Python substring/prefix/suffix and insensitive variants | `FilterExpr::text` + `TextComparison` | I | U |
| Python membership and negative membership | `FilterExpr::{is_in,not_in}` | I | U |
| Python regex and insensitive regex | `FilterExpr::regex` | I | U |
| Query composition and iteration | `and`, `or`, `!`, `QueryIteratorExt` | I | U |
| Native tmux filtering | explicit `NativeFilter` in listings | I | U, M |

## Server operations

| Python `Server` family | Rust API | Status | Validation |
|---|---|---:|---|
| `is_alive`, `raise_if_dead` | `is_alive`, `ensure_alive` | I | R |
| `attached_sessions`, `has_session` | corresponding async methods | I | R |
| `kill`, `kill_session`, `kill_server` | `kill`, `kill_session`, `Session::kill*` | I | R |
| `new_session` | `NewSession`, `new_session` | I | R |
| `run_shell` | typed `RunShell` | I | M |
| `wait_for` | `WaitAction`, `wait_for` | I | M |
| `bind_key`, `unbind_key`, `list_keys` | `KeyBinding`, `UnbindKey`, server methods | I | M |
| `list_commands` | `list_commands` | I | M |
| `lock_server` | `lock_server` | I | M |
| `server_access` | `ServerAccess`, `ClientPermission` | I | M |
| client refresh/suspend/lock/detach/all-detach | typed server and `Client` methods | I | R, M |
| `confirm_before` | `ConfirmBefore` | I | R, M |
| `command_prompt` | `CommandPrompt`, `PromptType` | I | R, M |
| `display_menu` | `Menu`, `MenuItem` | I | M |
| `start_server` | `start_server` | I | R |
| `show_messages` | `show_messages` | I | M |
| server `display_message` | `DisplayMessage`, `Server::display` | I | R, M |
| prompt history show/clear | `prompt_history`, `clear_prompt_history` | I | M |
| buffer set/show/delete/save/load/list/filter | `Buffer`, `BufferName`, buffer methods | I | R, M |
| `if_shell` | typed `IfShell` | I | M |
| `source_file` flags | `source_file`, `source_file_with` | I | M |
| list/search sessions/windows/panes/clients | typed discovery + `FilterExpr`; native filter variants | I | U, R |
| interactive attach | `Server::attach`, `Session::attach`, `AttachedClient` | I | R |

## Options, hooks, and environments

| Baseline capability | Rust API | Status | Validation |
|---|---|---:|---|
| Server/global-session/session/global-window/window/pane option scopes | `OptionScope` | I | R, M |
| Set, append, unset, show, inherit | `set_option`, `unset_option`, `show_options`, `OptionMap` | I | U, R |
| Sparse array options | `SparseOptionMap` | I | U, R |
| Hook run/set/append/unset/show/indexes | `HookScope` and server hook methods | I | U, R |
| Global and session environment show/get/set/format-expand/hide/unset/remove | server/session environment methods | I | U, R |
| Values containing spaces, `=`, Unicode, or non-UTF-8 bytes | `OptionValue`, `TmuxText`, first-separator parsers | I | U, R |

## Session operations

| Python `Session` capability | Rust API | Status | Validation |
|---|---|---:|---|
| Refresh/from ID/from environment | `refresh`, server lookup, `from_environment` | I | R |
| Windows/panes search and traversal | `windows`, `panes`, query expressions | I | R |
| Targeted raw command | `Session::cmd` | I | U |
| Lock and detach clients | `lock`, `detach_clients`, `detach_clients_with_command` | I | M |
| Last/next/previous/select window | corresponding methods | I | M |
| Active window and pane | `active_window`, `active_pane` | I | R |
| Attach/switch client | `attach`, `select` | I | R |
| Kill one/all-except/clear/group | `kill`, `kill_with` | I | R, M |
| Rename | `rename` | I | R |
| New window: directory/name/index/command/env/placement/replace/select-existing | `NewWindow` | I | R, M |
| Kill a selected window | `kill_window` | I | M |
| Session options/hooks/environment | uniform APIs | I | R |

## Window operations

| Python `Window` capability | Rust API | Status | Validation |
|---|---|---:|---|
| Refresh/from ID/from environment/session | lookup, `refresh`, `from_environment`, `session` | I | R |
| Linked sessions | `linked_sessions` | I | R |
| Pane discovery/search/active/select/last with input and zoom flags | window traversal methods | I | R, M |
| Targeted raw command | `Window::cmd` | I | U |
| Tiled split | `SplitPane`, `split` | I | R |
| Floating new pane | `NewPane`, `new_pane` | I | R, M |
| Relative/absolute/expand/shrink resizing | `WindowResize`, `resize_with` | I | M |
| Layout select/spread/next/previous/custom | `Layout` and layout methods; arbitrary layouts via `Layout::Custom` | I | M |
| Link/unlink/move/swap | typed targets and window mutation methods | I | R, M |
| Rotate and keep zoom | `Rotation`, `rotate`, `rotate_keep_zoom` | I | M |
| Respawn with directory/environment/kill | `RespawnWindow`, `respawn_with` | I | M |
| Window-context display message | `Window::display` | I | M |
| Rename/select/kill/all-except | mutation methods | I | R |
| Create relative window | `new_window` + `WindowPosition` | I | M |
| Window options/hooks | uniform APIs | I | R |

## Pane operations

| Python `Pane` capability | Rust API | Status | Validation |
|---|---|---:|---|
| Refresh/from ID/from environment/window/session | lookup and traversal methods | I | R |
| Targeted raw command | `Pane::cmd` | I | U |
| Relative/absolute/percentage/mouse resize, trim, and zoom | `ResizeDirection`, resize methods | I | R, M |
| Capture history/boundaries/buffers/screens/escapes/wrap/trailing/pending/3.7 metadata | `CapturePane`, `capture`, `capture_to_buffer` | I | R, M |
| Send keys/Enter/history suppression/literal/hex/reset/repeat/copy-mode/formats/client targeting | `SendKeys`, `send_keys` | I | R, M |
| Pane-context display message | `Pane::display` | I | M |
| Kill one/all-except, directional/last/mark/input select, and title | `SelectPane` and pane mutation methods | I | R, M |
| Tiled split and floating new pane | `SplitPane`, `NewPane` | I | R, M |
| Popup | `Popup`, `display_popup` | I | R, M |
| Paste buffer flags | `PasteBuffer` | I | M |
| Pipe input/output/toggle/disable | `PipePane` | I | M |
| Copy mode flags | `CopyMode` | I | M |
| Clock/display-panes/choose-buffer/choose-client/choose-tree/customize/find-window | typed mode methods | I | M |
| Send prefix | `send_prefix` | I | M |
| Respawn with directory/environment/kill | `RespawnPane` | I | M |
| Move/join | `PaneDestination`, `RelocatePane` | I | M |
| Break pane | `break_to_window` | I | M |
| Swap target/up/down/detach/keep-zoom | `SwapPane` | I | M |
| Clear history/hyperlinks, clear, atomic reset | clear/reset methods | I | M |
| Pane options/hooks | uniform APIs | I | R |

## Client operations

| Python `Client` capability | Rust API | Status | Validation |
|---|---|---:|---|
| Refresh/from name | discovery and `refresh` | I | R |
| Attached session/window/pane | `session`, `window`, `pane` | I | R |
| Detach/suspend/lock/switch/resize/refresh | client and server client methods | I | R, M |
| Interactive attachment lifecycle | `AttachedClient` | I | R |
| Control-mode test attachment | `test_support::ControlClient` | I | R |

## Rust-native test support

| Capability | Rust API | Status | Validation |
|---|---|---:|---|
| Isolated socket and temporary directory | `TestServer` | I | R |
| Temporary hierarchy | `TestHierarchy` | I | R |
| Async retries | `retry_until` | I | R |
| Attached control-mode client | `ControlClient` | I | R |
| Abandoned daemon cleanup | `TestServer::drop` fallback and async `shutdown` | I | R |
| pytest fixture/plugin injection | none | E | Fixed exclusion |

## Explicit exclusions

| Python surface | Status | Reason |
|---|---:|---|
| `__version__`, author/package dunder metadata | E | Python packaging metadata has no runtime parity value. |
| Deprecated aliases (`kill_server`, `attach_session`, `split_window`, `resize_pane`, legacy list/find/where/children methods) | E | Replaced by canonical typed methods. |
| `get`, `__getitem__`, arbitrary attribute compatibility | E | Replaced by typed snapshots and explicit raw field access. |
| pytest plugin, fixture injection, Python random/environment helpers | E | Replaced by the `test-support` feature. |
| Python exception class identity and warning behavior | E | Replaced by the non-exhaustive Rust `Error` hierarchy and explicit capability errors. |
| A blocking facade or implicit executor | E | The public API is Tokio async-only by design. |

## Scenario coverage derived from Python tests

The real-tmux suite currently covers isolated creation and cleanup, hierarchy discovery, splits, key input, capture-to-buffer and history boundaries, Unicode and non-UTF-8 buffer bytes, environment values/removals, options, sparse hooks, linked-window duplication, stale snapshots, dead panes, unavailable sockets, lenient listings, raw non-zero statuses, cancellation timeout, attached control clients, attachment resolution, refresh, popup dispatch, confirmation prompts, command prompts, cleanup failures, and the tmux 3.7 floating-pane gate. The CI matrix repeats applicable scenarios against every supported tmux line and master.
