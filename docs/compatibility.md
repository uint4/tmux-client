# Compatibility

`tmux-client` targets Rust 1.85+, tmux 3.2a+, and Unix environments where tmux itself is supported.

| Platform | Status | Notes |
|---|---|---|
| Linux | Supported | CI covers tmux 3.2a, 3.3a, 3.4, 3.5, 3.6, 3.7a, 3.7b, and master. |
| macOS | Supported | CI covers the current Homebrew tmux release. |
| WSL | Supported | Uses the Linux tmux binary and Unix socket model. |
| Native Windows | Not supported | tmux does not provide the required native process and Unix-socket interface. |

The library detects `tmux -V` once per `Server`. Format variables and optional flags are gated by release. Requesting a feature that is unavailable on the connected release returns `Error::Unsupported`; requested behavior is never silently discarded.

The MSRV job checks Rust 1.85.0. The primary quality job uses current stable Rust. Edition 2024 is intentional.
