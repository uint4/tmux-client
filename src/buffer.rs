//! Paste-buffer discovery and byte-preserving operations.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::path::Path;

use crate::{Command, Error, FormatScope, NativeFilter, ObjectKind, Result, Server, TmuxText};

/// A validated tmux paste-buffer name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BufferName(TmuxText);

impl BufferName {
    /// Creates a non-empty buffer name.
    pub fn new(value: impl Into<TmuxText>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() || value.as_bytes().contains(&b'\0') {
            return Err(Error::InvalidArgument {
                argument: "buffer name",
                message: "must be non-empty and contain no NUL byte".to_owned(),
            });
        }
        Ok(Self(value))
    }

    /// Returns the byte-preserving name.
    #[must_use]
    pub const fn as_text(&self) -> &TmuxText {
        &self.0
    }

    pub(crate) fn to_os_string(&self) -> OsString {
        self.0.to_os_string()
    }
}

impl fmt::Display for BufferName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// An owned snapshot of a tmux paste buffer.
#[derive(Clone, Debug)]
pub struct Buffer {
    server: Server,
    name: BufferName,
    size: u64,
    sample: TmuxText,
}

impl Buffer {
    fn from_row(server: Server, row: &BTreeMap<&'static str, TmuxText>) -> Result<Self> {
        let name = row
            .get("buffer_name")
            .cloned()
            .ok_or_else(|| Error::Decode {
                context: "buffer row",
                message: "buffer_name was absent".to_owned(),
            })?;
        let size = row
            .get("buffer_size")
            .ok_or_else(|| Error::Decode {
                context: "buffer row",
                message: "buffer_size was absent".to_owned(),
            })?
            .to_str()
            .map_err(|source| Error::Decode {
                context: "buffer size",
                message: source.to_string(),
            })?
            .parse()
            .map_err(|source: std::num::ParseIntError| Error::Decode {
                context: "buffer size",
                message: source.to_string(),
            })?;
        let sample = row.get("buffer_sample").cloned().unwrap_or_default();
        Ok(Self {
            server,
            name: BufferName::new(name)?,
            size,
            sample,
        })
    }

    /// Returns the buffer name.
    #[must_use]
    pub const fn name(&self) -> &BufferName {
        &self.name
    }

    /// Returns the byte size reported by tmux.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// Returns tmux's listing sample.
    #[must_use]
    pub const fn sample(&self) -> &TmuxText {
        &self.sample
    }

    /// Returns the server handle.
    #[must_use]
    pub const fn server(&self) -> &Server {
        &self.server
    }

    /// Refreshes this named buffer.
    pub async fn refresh(&mut self) -> Result<()> {
        *self = self.server.buffer(&self.name).await?;
        Ok(())
    }

    /// Reads the complete buffer contents.
    pub async fn show(&self) -> Result<TmuxText> {
        self.server.show_buffer(Some(&self.name)).await
    }

    /// Deletes the named buffer.
    pub async fn delete(&self) -> Result<()> {
        self.server.delete_buffer(Some(&self.name)).await
    }

    /// Saves the buffer to a file through tmux.
    pub async fn save(&self, path: impl AsRef<Path>, append: bool) -> Result<()> {
        self.server.save_buffer(&self.name, path, append).await
    }
}

impl PartialEq for Buffer {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.server.config() == other.server.config()
    }
}

impl Eq for Buffer {}

impl Server {
    /// Lists paste buffers.
    pub async fn buffers(&self) -> Result<Vec<Buffer>> {
        self.list_rows("list-buffers", &[FormatScope::Buffer], [], None)
            .await?
            .into_iter()
            .map(|row| Buffer::from_row(self.clone(), &row))
            .collect()
    }

    /// Lists buffers with a raw output format and optional native filter.
    pub async fn list_buffers(
        &self,
        format: Option<&str>,
        filter: Option<&NativeFilter>,
    ) -> Result<Vec<TmuxText>> {
        let mut command = Command::new("list-buffers");
        if let Some(format) = format {
            command = command.arg("-F").arg(format);
        }
        if let Some(filter) = filter {
            command = command.arg("-f").arg(filter.as_str());
        }
        Ok(self
            .checked("list buffers", command)
            .await?
            .stdout_lines()
            .collect())
    }

    /// Lists buffers, returning an empty collection on failure.
    pub async fn buffers_or_empty(&self) -> Vec<Buffer> {
        self.buffers().await.unwrap_or_default()
    }

    /// Resolves an exact buffer name.
    pub async fn buffer(&self, name: &BufferName) -> Result<Buffer> {
        self.buffers()
            .await?
            .into_iter()
            .find(|buffer| buffer.name == *name)
            .ok_or_else(|| Error::ObjectNotFound {
                kind: ObjectKind::Buffer,
                target: name.to_string(),
            })
    }

    /// Loads byte-preserving data through stdin and returns its resulting buffer.
    pub async fn load_buffer(
        &self,
        name: Option<&BufferName>,
        data: impl Into<TmuxText>,
    ) -> Result<Buffer> {
        let mut command = Command::new("load-buffer");
        if let Some(name) = name {
            command = command.arg("-b").arg(name.to_os_string());
        }
        command = command.arg("-").sensitive_input(data);
        self.checked("load buffer", command).await?;
        if let Some(name) = name {
            return self.buffer(name).await;
        }
        self.buffers()
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::ObjectNotFound {
                kind: ObjectKind::Buffer,
                target: "most recent buffer".to_owned(),
            })
    }

    /// Loads a filesystem path into a named or automatic tmux buffer.
    pub async fn load_buffer_file(
        &self,
        name: Option<&BufferName>,
        path: impl AsRef<Path>,
    ) -> Result<Buffer> {
        let mut command = Command::new("load-buffer");
        if let Some(name) = name {
            command = command.arg("-b").arg(name.to_os_string());
        }
        self.checked("load buffer file", command.arg(path.as_ref()))
            .await?;
        if let Some(name) = name {
            return self.buffer(name).await;
        }
        self.buffers()
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::ObjectNotFound {
                kind: ObjectKind::Buffer,
                target: "most recent buffer".to_owned(),
            })
    }

    /// Sets text using `set-buffer`. Prefer [`Self::load_buffer`] for arbitrary bytes.
    pub async fn set_buffer(
        &self,
        name: Option<&BufferName>,
        data: impl Into<TmuxText>,
        append: bool,
    ) -> Result<Buffer> {
        let data = data.into();
        if data.as_bytes().contains(&b'\0') {
            return self.load_buffer(name, data).await;
        }
        let mut command = Command::new("set-buffer");
        if append {
            command = command.arg("-a");
        }
        if let Some(name) = name {
            command = command.arg("-b").arg(name.to_os_string());
        }
        command = command.sensitive_arg(data.to_os_string());
        self.checked("set buffer", command).await?;
        if let Some(name) = name {
            return self.buffer(name).await;
        }
        self.buffers()
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::ObjectNotFound {
                kind: ObjectKind::Buffer,
                target: "most recent buffer".to_owned(),
            })
    }

    /// Reads a named buffer or tmux's most recent buffer.
    pub async fn show_buffer(&self, name: Option<&BufferName>) -> Result<TmuxText> {
        let mut command = Command::new("show-buffer");
        if let Some(name) = name {
            command = command.arg("-b").arg(name.to_os_string());
        }
        self.checked("show buffer", command)
            .await
            .map(|result| result.stdout().clone())
    }

    /// Deletes a named buffer or tmux's most recent buffer.
    pub async fn delete_buffer(&self, name: Option<&BufferName>) -> Result<()> {
        let mut command = Command::new("delete-buffer");
        if let Some(name) = name {
            command = command.arg("-b").arg(name.to_os_string());
        }
        self.checked("delete buffer", command).await.map(|_| ())
    }

    /// Saves a buffer to a filesystem path through tmux.
    pub async fn save_buffer(
        &self,
        name: &BufferName,
        path: impl AsRef<Path>,
        append: bool,
    ) -> Result<()> {
        let mut command = Command::new("save-buffer")
            .arg("-b")
            .arg(name.to_os_string());
        if append {
            command = command.arg("-a");
        }
        self.checked("save buffer", command.arg(path.as_ref()))
            .await
            .map(|_| ())
    }
}
