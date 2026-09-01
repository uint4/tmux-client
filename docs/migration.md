# Migrating from Python libtmux

The behavioral reference is Python libtmux 0.62.0 at commit `a7ff6b6cd687e0c96874a839f8cbb2b797623680`. This is a Rust API, not a transliteration.

| Python pattern | Rust replacement |
|---|---|
| `Server(...)` keyword configuration | `Server::builder()` |
| synchronous methods | Tokio `async` methods |
| string IDs and targets | `SessionId`, `WindowId`, `PaneId`, and target enums |
| mutable object attributes | owned `*Snapshot`; update with `refresh(&mut self)` |
| decoded `str` output | byte-preserving `TmuxText`; choose strict or lossy UTF-8 explicitly |
| `QueryList.filter(...)` | `FilterExpr<T>` plus `QueryIteratorExt` |
| raw tmux `-f` | explicit `NativeFilter` |
| implicit empty list after lookup errors | `Result<Vec<T>>`; opt into `*_or_empty` |
| context-manager cleanup | `with_session`, `with_window`, and `with_pane` |
| attaching in the current process | async `AttachedClient` child handle |
| option and hook dictionaries | `OptionMap`, `OptionValue`, and `SparseOptionMap` |

`Server::cmd(Command)` deliberately returns a successful Rust `Result` when tmux itself exits non-zero. Inspect `CommandResult::success`, or call `ensure_success`. Typed operations convert a non-zero status into `Error::CommandFailed`.

Deprecated Python aliases, dictionary indexing, package dunder metadata, pytest fixture injection, and Python-only internal attributes have no Rust equivalent. Ordinary Rust handle drops never kill tmux objects.
