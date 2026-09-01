//! Create and inspect a detached hierarchy.

use tmux_client::{NewSession, SendKeys, Server, SplitPane};

#[tokio::main(flavor = "current_thread")]
async fn main() -> tmux_client::Result<()> {
    let server = Server::new();
    server
        .with_session(
            NewSession::new().name("tmux-client-example")?,
            |session| async move {
                let window = session.active_window().await?;
                let pane = window.active_pane().await?;
                let second = pane.split(SplitPane::new()).await?;
                second
                    .send_keys(
                        SendKeys::new()
                            .key("printf 'hello from tmux-client'")
                            .literal(true),
                    )
                    .await?;
                second.send_keys(SendKeys::new().key("Enter")).await?;
                Ok(())
            },
        )
        .await
}
