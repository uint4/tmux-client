# tmux-client

`tmux-client` is a typed, asynchronous Rust client for the tmux terminal multiplexer. It models tmux's server → session → window → pane hierarchy, attached clients, format snapshots, options, hooks, environments, buffers, interactive commands, and the raw command escape hatch.

The crate is an independent Rust port of Python libtmux 0.62.0 and currently carries an alpha version while parity work is completed.

```rust,no_run
use tmux_client::{NewSession, Server};

#[tokio::main]
async fn main() -> Result<(), tmux_client::Error> {
    let server = Server::new();
    let session = server
        .new_session(NewSession::new().name("work")?)
        .await?;
    if let Some(name) = session.name() {
        println!("{name}");
    }
    session.kill().await?;
    Ok(())
}
```

The default `query` feature supplies typed in-memory filtering and regular expressions. `tracing` adds redacted command events. `test-support` provides isolated sockets, hierarchy fixtures, retry helpers, control-mode clients, and abandoned-daemon cleanup. The crate never creates a Tokio runtime internally.

See [compatibility](docs/compatibility.md), [Python migration](docs/migration.md), [parity](docs/parity.md), [PLAN.md](PLAN.md), and [CODEX.md](CODEX.md).
