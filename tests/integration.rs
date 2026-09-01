//! Real-tmux smoke tests for the public hierarchy.

#![cfg(feature = "test-support")]

use std::time::Duration;

use tmux_client::test_support::{TestServer, retry_until};
use tmux_client::{
    BufferName, CapturePane, ClientName, CommandPrompt, ConfirmBefore, Error, HookScope, NewPane,
    NewSession, NewWindow, OptionScope, OptionValue, Popup, ReleaseVersion, SendKeys, Server,
    SplitDirection, SplitPane, TmuxText,
};

#[tokio::test]
async fn discovers_mutates_and_cleans_up_an_isolated_hierarchy() -> tmux_client::Result<()> {
    let test_server = TestServer::new()?;
    let hierarchy = test_server.hierarchy().await?;

    assert_eq!(test_server.server().sessions().await?.len(), 1);
    assert_eq!(hierarchy.session.windows().await?.len(), 1);
    assert_eq!(hierarchy.window.panes().await?.len(), 1);

    let pane = hierarchy
        .pane
        .split(SplitPane::new().direction(SplitDirection::Horizontal))
        .await?;
    pane.send_keys(
        SendKeys::new()
            .key("printf tmux-client")
            .literal(true)
            .enter(true)
            .sensitive(true),
    )
    .await?;
    let captured = retry_until(
        Duration::from_secs(3),
        Duration::from_millis(25),
        || async {
            let captured = pane.capture(&CapturePane::new()).await?;
            Ok(captured
                .to_string_lossy()
                .contains("tmux-client")
                .then_some(captured))
        },
    )
    .await?;
    assert!(captured.to_string_lossy().contains("tmux-client"));

    let capture_buffer = BufferName::new("capture")?;
    pane.capture_to_buffer(
        &CapturePane::new().start_of_history().end_of_screen(),
        &capture_buffer,
    )
    .await?;
    assert!(
        test_server
            .server()
            .show_buffer(Some(&capture_buffer))
            .await?
            .to_string_lossy()
            .contains("tmux-client")
    );

    test_server.shutdown().await
}

#[tokio::test]
async fn preserves_link_rows_and_reports_stale_objects() -> tmux_client::Result<()> {
    let test_server = TestServer::new()?;
    let hierarchy = test_server.hierarchy().await?;
    let second = test_server
        .server()
        .new_session(NewSession::new().name("linked_destination")?)
        .await?;

    let linked = hierarchy
        .window
        .link_to(second.id(), Some(5), false)
        .await?;
    let matching = test_server
        .server()
        .windows()
        .await?
        .into_iter()
        .filter(|window| window.id() == hierarchy.window.id())
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 2);
    assert_ne!(matching[0].link(), matching[1].link());

    linked.unlink(false).await?;
    let mut stale = hierarchy.window.clone();
    hierarchy.window.kill().await?;
    assert!(matches!(
        stale.refresh().await,
        Err(Error::ObjectNotFound { .. })
    ));

    test_server.shutdown().await
}

#[tokio::test]
async fn handles_buffers_options_hooks_and_unicode() -> tmux_client::Result<()> {
    let test_server = TestServer::new()?;
    let session = test_server
        .server()
        .new_session(NewSession::new().name("unicode_雪")?)
        .await?;
    let mut window = session
        .new_window(NewWindow::new().name("λ-window"))
        .await?;
    assert_eq!(
        window
            .snapshot()
            .name()
            .map(TmuxText::to_string_lossy)
            .as_deref(),
        Some("λ-window")
    );

    let buffer_name = BufferName::new("named")?;
    let buffer = test_server
        .server()
        .load_buffer(Some(&buffer_name), TmuxText::from("snowman ☃\n"))
        .await?;
    assert_eq!(buffer.show().await?.as_bytes(), "snowman ☃\n".as_bytes());

    let binary_name = BufferName::new("binary")?;
    let binary = TmuxText::new(vec![0xff, b'a', b'\n']);
    test_server
        .server()
        .load_buffer(Some(&binary_name), binary.clone())
        .await?;
    assert_eq!(
        test_server
            .server()
            .show_buffer(Some(&binary_name))
            .await?
            .as_bytes(),
        binary.as_bytes()
    );

    session
        .set_option("@tmux-client-test", &OptionValue::from("enabled"), false)
        .await?;
    assert!(session.options().await?.get("@tmux-client-test").is_some());

    test_server
        .server()
        .set_hook(
            HookScope::Global,
            None,
            "after-new-window",
            Some(17),
            &OptionValue::from("display-message hook"),
            false,
        )
        .await?;
    assert!(
        test_server
            .server()
            .show_hooks(HookScope::Global, None)
            .await?
            .get("after-new-window")
            .is_some()
    );

    window.rename("renamed-雪").await?;
    assert_eq!(
        window
            .snapshot()
            .name()
            .map(TmuxText::to_string_lossy)
            .as_deref(),
        Some("renamed-雪")
    );
    test_server.shutdown().await
}

#[tokio::test]
async fn handles_global_and_session_environment_entries() -> tmux_client::Result<()> {
    let test_server = TestServer::new()?;
    let hierarchy = test_server.hierarchy().await?;
    test_server
        .server()
        .set_environment("TMUX_CLIENT_TEST", "välue")
        .await?;
    let environment = test_server.server().environment().await?;
    assert_eq!(
        environment
            .get("TMUX_CLIENT_TEST")
            .and_then(Option::as_ref)
            .map(TmuxText::to_string_lossy)
            .as_deref(),
        Some("välue")
    );
    assert_eq!(
        test_server
            .server()
            .environment_entry("TMUX_CLIENT_TEST")
            .await?
            .flatten()
            .map(|value| value.to_string_lossy().into_owned()),
        Some("välue".to_owned())
    );

    hierarchy
        .session
        .set_environment_with("TMUX_CLIENT_SESSION_TEST", "session-value", false, false)
        .await?;
    assert_eq!(
        hierarchy
            .session
            .environment_entry("TMUX_CLIENT_SESSION_TEST")
            .await?
            .flatten()
            .map(|value| value.to_string_lossy().into_owned()),
        Some("session-value".to_owned())
    );
    hierarchy
        .session
        .remove_environment("TMUX_CLIENT_SESSION_TEST")
        .await?;
    assert_eq!(
        hierarchy
            .session
            .environment_entry("TMUX_CLIENT_SESSION_TEST")
            .await?,
        Some(None)
    );
    test_server.shutdown().await
}

#[tokio::test]
async fn raw_status_timeout_and_lenient_discovery_are_distinct() -> tmux_client::Result<()> {
    let false_server = Server::builder().executable("false").build();
    let raw = false_server
        .cmd(tmux_client::Command::new("ignored"))
        .await?;
    assert!(!raw.success());

    let sleeping = Server::builder()
        .executable("sleep")
        .timeout(Duration::from_millis(20))?
        .build();
    assert!(matches!(
        sleeping.cmd(tmux_client::Command::new("10")).await,
        Err(Error::Timeout { .. })
    ));

    let unavailable = TestServer::new()?;
    assert!(unavailable.server().sessions().await.is_err());
    assert!(unavailable.server().sessions_or_empty().await.is_empty());
    unavailable.shutdown().await
}

#[tokio::test]
async fn resolves_attached_clients_and_interactive_commands() -> tmux_client::Result<()> {
    let test_server = TestServer::new()?;
    let hierarchy = test_server.hierarchy().await?;
    let mut control = test_server.attach_control_mode(&hierarchy.session)?;

    let client = retry_until(
        Duration::from_secs(3),
        Duration::from_millis(25),
        || async { Ok(test_server.server().clients().await?.into_iter().next()) },
    )
    .await?;
    assert_eq!(
        client
            .session()
            .await?
            .as_ref()
            .map(tmux_client::Session::id),
        Some(hierarchy.session.id())
    );
    assert_eq!(
        client.window().await?.as_ref().map(tmux_client::Window::id),
        Some(hierarchy.window.id())
    );
    assert_eq!(
        client.pane().await?.as_ref().map(tmux_client::Pane::id),
        Some(hierarchy.pane.id())
    );

    test_server
        .server()
        .refresh_client(Some(client.name()), false)
        .await?;
    hierarchy
        .pane
        .display_popup(
            Popup::new()
                .close_existing(true)
                .target_client(client.name().clone()),
        )
        .await?;

    let version = test_server.server().version().await?;
    if version.meets(ReleaseVersion::new(3, 4, None)) {
        exercise_background_prompts(test_server.server(), client.name()).await?;
    } else {
        assert!(matches!(
            test_server
                .server()
                .send_client_key(client.name(), "y")
                .await,
            Err(Error::Unsupported { .. })
        ));
        if !version.meets(ReleaseVersion::new(3, 3, None)) {
            assert!(matches!(
                test_server
                    .server()
                    .confirm_before(
                        ConfirmBefore::new("display-message unsupported")
                            .target_client(client.name().clone()),
                    )
                    .await,
                Err(Error::Unsupported { .. })
            ));
        }
    }

    control.terminate().await?;
    test_server.shutdown().await
}

async fn exercise_background_prompts(
    server: &Server,
    client: &ClientName,
) -> tmux_client::Result<()> {
    server
        .confirm_before(
            ConfirmBefore::new("set-option -g @tmux-client-confirmed yes")
                .target_client(client.clone()),
        )
        .await?;
    retry_until(
        Duration::from_secs(3),
        Duration::from_millis(25),
        || async {
            server.send_client_key(client, "y").await?;
            let options = server
                .show_options(OptionScope::GlobalSession, None)
                .await?;
            Ok(options
                .get("@tmux-client-confirmed")
                .is_some()
                .then_some(()))
        },
    )
    .await?;

    server
        .command_prompt(
            CommandPrompt::new("set-option -g @tmux-client-prompt '%1'")
                .target_client(client.clone()),
        )
        .await?;
    retry_until(
        Duration::from_secs(3),
        Duration::from_millis(25),
        || async {
            for key in ["h", "i", "Enter"] {
                server.send_client_key(client, key).await?;
            }
            let options = server
                .show_options(OptionScope::GlobalSession, None)
                .await?;
            Ok(options.get("@tmux-client-prompt").is_some().then_some(()))
        },
    )
    .await
}

#[tokio::test]
async fn floating_panes_are_gated_or_created() -> tmux_client::Result<()> {
    let test_server = TestServer::new()?;
    let hierarchy = test_server.hierarchy().await?;
    if test_server
        .server()
        .version()
        .await?
        .meets(ReleaseVersion::new(3, 7, None))
    {
        let pane = hierarchy
            .pane
            .new_pane(
                NewPane::new()
                    .width(40)?
                    .height(10)?
                    .shell_command("sleep 30"),
            )
            .await?;
        assert_eq!(pane.snapshot().floating()?, Some(true));
        pane.kill().await?;
    } else {
        assert!(matches!(
            hierarchy.pane.new_pane(NewPane::new()).await,
            Err(Error::Unsupported { .. })
        ));
    }
    test_server.shutdown().await
}

#[tokio::test]
async fn discovers_dead_panes_without_losing_their_snapshot() -> tmux_client::Result<()> {
    let test_server = TestServer::new()?;
    let hierarchy = test_server.hierarchy().await?;
    hierarchy
        .window
        .set_option("remain-on-exit", &OptionValue::from("on"), false)
        .await?;
    let dead = hierarchy
        .pane
        .split(SplitPane::new().shell_command("false"))
        .await?;
    let server = test_server.server().clone();
    let pane_id = dead.id();
    let dead = retry_until(Duration::from_secs(3), Duration::from_millis(25), || {
        let server = server.clone();
        async move {
            let pane = server.pane(pane_id).await?;
            Ok(pane.snapshot().dead()?.unwrap_or(false).then_some(pane))
        }
    })
    .await?;
    assert!(dead.snapshot().dead()?.unwrap_or(false));
    assert!(dead.snapshot().dead_status()?.is_some());
    test_server.shutdown().await
}

#[tokio::test]
async fn scoped_helpers_cleanup_and_surface_cleanup_failures() -> tmux_client::Result<()> {
    let test_server = TestServer::new()?;
    let server = test_server.server().clone();
    let id = server
        .with_session(NewSession::new().name("scoped")?, |session| async move {
            let window = session.active_window().await?;
            window
                .with_pane(SplitPane::new(), |pane| async move { Ok(pane.id()) })
                .await?;
            Ok(session.id())
        })
        .await?;
    assert!(
        !server
            .sessions_or_empty()
            .await
            .iter()
            .any(|session| session.id() == id)
    );

    let cleanup_failure = server
        .with_session(
            NewSession::new().name("cleanup_failure")?,
            |session| async move {
                session.kill().await?;
                Ok(())
            },
        )
        .await;
    assert!(matches!(cleanup_failure, Err(Error::Cleanup { .. })));
    test_server.shutdown().await
}
