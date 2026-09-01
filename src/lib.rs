//! A typed, asynchronous client for tmux.
//!
//! `tmux-client` invokes the tmux command-line protocol directly. It does not
//! start a runtime, invoke a shell, or make dropping an object destructive.
//!
//! ```no_run
//! use tmux_client::{NewSession, Server};
//!
//! # async fn example() -> tmux_client::Result<()> {
//! let server = Server::new();
//! let session = server
//!     .new_session(NewSession::new().name("example")?.detached(true))
//!     .await?;
//! assert_eq!(session.name().map(|name| name.as_str()), Some("example"));
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs, rustdoc::broken_intra_doc_links)]

mod advanced;
mod buffer;
mod client;
mod command;
mod error;
mod formats;
mod options;
mod pane;
#[cfg(feature = "query")]
pub mod query;
mod server;
mod session;
mod snapshot;
mod target;
#[cfg(feature = "test-support")]
pub mod test_support;
mod text;
mod version;
mod window;

pub use advanced::{
    ClientPermission, CommandPrompt, ConfirmBefore, DisplayMessage, HookScope, IfShell, KeyBinding,
    Menu, MenuItem, Popup, PromptType, RunShell, ServerAccess, UnbindKey, WaitAction,
};
pub use buffer::{Buffer, BufferName};
pub use client::{AttachedClient, Client};
pub use command::{Command, CommandResult, CommandSummary};
pub use error::{Error, ObjectKind, OptionErrorKind, Result};
pub use formats::{
    DecodedType, FormatDescriptor, FormatScope, FormatToken, FormatValue, NativeFilter,
    format_catalog, format_descriptor,
};
pub use options::{OptionMap, OptionScope, OptionValue, SparseOptionMap};
pub use pane::{
    CaptureLine, CapturePane, ChooseTree, CopyMode, FindWindow, NewPane, Pane, PaneDestination,
    PaneSize, PasteBuffer, PipePane, RelocatePane, ResizeDirection, RespawnPane, SelectPane,
    SendKeys, SplitDirection, SplitPane, SwapPane, SwapPaneDirection, TreeSort,
};
pub use server::{ColorMode, NewSession, Server, ServerBuilder, ServerConfig, Socket};
pub use session::Session;
pub use snapshot::{ClientSnapshot, PaneSnapshot, SessionSnapshot, SnapshotFields, WindowSnapshot};
pub use target::{
    ClientName, PaneId, PaneTarget, SessionId, SessionName, SessionTarget, WindowId, WindowLink,
    WindowTarget,
};
pub use text::TmuxText;
pub use version::{ReleaseVersion, TmuxVersion};
pub use window::{
    Layout, NewWindow, RespawnWindow, Rotation, Window, WindowPosition, WindowResize,
};
