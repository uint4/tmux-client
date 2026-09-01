//! Rust-native utilities for isolated, real-tmux integration tests.
//!
//! This module is available with the `test-support` feature.

use std::future::Future;
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command as ProcessCommand};

use crate::{
    ColorMode, Command, Error, NewSession, Pane, Result, Server, Session, Socket, TmuxText, Window,
};

static NEXT_NAME: AtomicU64 = AtomicU64::new(1);

/// An isolated tmux socket whose abandoned daemon is cleaned up on drop.
#[derive(Debug)]
pub struct TestServer {
    server: Server,
    directory: TempDir,
    shutdown: bool,
}

impl TestServer {
    /// Allocates an isolated socket path without starting tmux yet.
    pub fn new() -> Result<Self> {
        let directory = tempfile::tempdir().map_err(|source| Error::Io {
            operation: "create tmux test directory",
            source,
        })?;
        let socket = directory.path().join("tmux.sock");
        let server = Server::builder()
            .socket_path(socket)
            .timeout(Duration::from_secs(10))?
            .build();
        Ok(Self {
            server,
            directory,
            shutdown: false,
        })
    }

    /// Returns the isolated server handle.
    #[must_use]
    pub const fn server(&self) -> &Server {
        &self.server
    }

    /// Returns the directory containing the socket.
    #[must_use]
    pub fn directory(&self) -> &std::path::Path {
        self.directory.path()
    }

    /// Starts the tmux daemon without creating a session.
    pub async fn start(&self) -> Result<()> {
        self.server
            .cmd(Command::new("start-server"))
            .await?
            .ensure_success("start test server")
            .map(|_| ())
    }

    /// Creates a deterministic session/window/pane hierarchy.
    pub async fn hierarchy(&self) -> Result<TestHierarchy> {
        let sequence = NEXT_NAME.fetch_add(1, Ordering::Relaxed);
        let name = format!("tmux_client_test_{}_{}", std::process::id(), sequence);
        let session = self
            .server
            .new_session(NewSession::new().name(name)?)
            .await?;
        let window = session.active_window().await?;
        let pane = window.active_pane().await?;
        Ok(TestHierarchy {
            session,
            window,
            pane,
        })
    }

    /// Attaches a test client using tmux control mode and piped I/O.
    pub fn attach_control_mode(&self, session: &Session) -> Result<ControlClient> {
        ControlClient::attach(&self.server, session)
    }

    /// Kills the daemon and consumes the guard.
    pub async fn shutdown(mut self) -> Result<()> {
        let result = self.server.kill().await;
        self.shutdown = true;
        result
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if self.shutdown {
            return;
        }
        let mut process = configured_std_process(&self.server);
        process
            .arg("kill-server")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let _ignored = process.status();
    }
}

/// A small hierarchy returned by [`TestServer::hierarchy`].
#[derive(Clone, Debug)]
pub struct TestHierarchy {
    /// The isolated session.
    pub session: Session,
    /// The session's initial window.
    pub window: Window,
    /// The window's initial pane.
    pub pane: Pane,
}

/// A tmux control-mode client suitable for attachment tests.
#[derive(Debug)]
pub struct ControlClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl ControlClient {
    fn attach(server: &Server, session: &Session) -> Result<Self> {
        let mut process = configured_process(server);
        process
            .kill_on_drop(true)
            .arg("-C")
            .arg("attach-session")
            .arg("-t")
            .arg(session.id().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = process.spawn().map_err(|source| Error::Io {
            operation: "spawn tmux control client",
            source,
        })?;
        let stdin = child.stdin.take().ok_or_else(|| Error::Io {
            operation: "open control client stdin",
            source: std::io::Error::other("Tokio did not provide piped stdin"),
        })?;
        let stdout = child.stdout.take().ok_or_else(|| Error::Io {
            operation: "open control client stdout",
            source: std::io::Error::other("Tokio did not provide piped stdout"),
        })?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    /// Sends one control-mode command followed by a newline.
    pub async fn send(&mut self, command: &[u8]) -> Result<()> {
        self.stdin
            .write_all(command)
            .await
            .map_err(|source| Error::Io {
                operation: "write control command",
                source,
            })?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|source| Error::Io {
                operation: "write control newline",
                source,
            })?;
        self.stdin.flush().await.map_err(|source| Error::Io {
            operation: "flush control command",
            source,
        })
    }

    /// Reads one byte-preserving control-mode line, including its newline.
    pub async fn read_line(&mut self) -> Result<TmuxText> {
        let mut bytes = Vec::new();
        self.stdout
            .read_until(b'\n', &mut bytes)
            .await
            .map_err(|source| Error::Io {
                operation: "read control output",
                source,
            })?;
        Ok(TmuxText::new(bytes))
    }

    /// Returns the child process ID when available.
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Terminates and reaps the attached client process.
    pub async fn terminate(&mut self) -> Result<ExitStatus> {
        self.child.kill().await.map_err(|source| Error::Io {
            operation: "terminate control client",
            source,
        })?;
        self.child.wait().await.map_err(|source| Error::Io {
            operation: "wait for control client",
            source,
        })
    }
}

/// Retries an async probe until it yields a value or a deadline expires.
pub async fn retry_until<F, Fut, T>(
    timeout: Duration,
    interval: Duration,
    mut probe: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<T>>>,
{
    if timeout.is_zero() || interval.is_zero() {
        return Err(Error::InvalidArgument {
            argument: "retry duration",
            message: "timeout and interval must be non-zero".to_owned(),
        });
    }
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(value) = probe().await? {
            return Ok(value);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(Error::WaitTimeout {
                condition: "integration-test retry probe".to_owned(),
            });
        }
        tokio::time::sleep(interval).await;
    }
}

fn configured_std_process(server: &Server) -> std::process::Command {
    let config = server.config();
    let mut process = std::process::Command::new(config.executable());
    if config.clears_environment() {
        process.env_clear();
    }
    process.envs(config.environment());
    match config.socket() {
        Socket::Default => {}
        Socket::Name(name) => {
            process.arg("-L").arg(name);
        }
        Socket::Path(path) => {
            process.arg("-S").arg(path);
        }
    }
    if let Some(path) = config.config_file() {
        process.arg("-f").arg(path);
    }
    match config.color_mode() {
        ColorMode::Default => {}
        ColorMode::Colors88 => {
            process.arg("-8");
        }
        ColorMode::Colors256 => {
            process.arg("-2");
        }
    }
    process
}

fn configured_process(server: &Server) -> ProcessCommand {
    let standard = configured_std_process(server);
    ProcessCommand::from(standard)
}
