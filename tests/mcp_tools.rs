use anyhow::Result;
use serde_json::json;
use squad::mcp::McpServer;
use squad::protocol::{DaemonEnvelope, DaemonRequest, DaemonResponse, InboxMessage};
use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

async fn setup_daemon<F>(
    handler: F,
) -> Result<(tempfile::TempDir, tokio::task::JoinHandle<Result<()>>)>
where
    F: FnOnce(DaemonEnvelope) -> DaemonResponse + Send + 'static,
{
    let workspace = tempdir()?;
    std::fs::create_dir_all(workspace.path().join(".squad"))?;
    std::fs::write(
        workspace.path().join("squad.yaml"),
        "workflow:\n  start_at: writer\n  steps: []\n",
    )?;

    let socket_path = workspace.path().join(".squad/squad.sock");
    let listener = UnixListener::bind(&socket_path)?;
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let envelope: DaemonEnvelope = serde_json::from_str(&line)?;
        let response = DaemonEnvelope {
            id: envelope.id.clone(),
            body: handler(envelope),
        };
        let payload = serde_json::to_string(&response)? + "\n";
        reader.get_mut().write_all(payload.as_bytes()).await?;
        Ok::<(), anyhow::Error>(())
    });

    Ok((workspace, task))
}

#[tokio::test]
async fn send_message_tool_calls_daemon_and_returns_user_facing_text() -> Result<()> {
    let (workspace, daemon_task) = setup_daemon(|envelope| match envelope.body {
        DaemonRequest::SendMessage { to, content } => {
            assert_eq!(to, "agent-2");
            assert_eq!(content, "Please review this diff");
            DaemonResponse::Ack {
                message: "queued".into(),
            }
        }
        other => panic!("unexpected request: {other:?}"),
    })
    .await?;

    let server = McpServer::for_workspace(workspace.path().to_path_buf());
    let response = server
        .handle_request(json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": {
                "name": "send_message",
                "arguments": {
                    "to": "agent-2",
                    "content": "Please review this diff"
                }
            }
        }))
        .await?;

    assert_eq!(
        response["result"]["content"][0]["text"],
        "Message sent to agent-2. They will process it on their next check."
    );
    daemon_task.await??;
    Ok(())
}

#[tokio::test]
async fn check_inbox_tool_returns_messages_from_daemon() -> Result<()> {
    let workspace = tempdir()?;
    std::fs::create_dir_all(workspace.path().join(".squad"))?;
    std::fs::write(
        workspace.path().join("squad.yaml"),
        "workflow:
  start_at: writer
  steps: []
",
    )?;

    let socket_path = workspace.path().join(".squad/squad.sock");
    let listener = UnixListener::bind(&socket_path)?;
    let daemon_task = tokio::spawn(async move {
        let (heartbeat_stream, _) = listener.accept().await?;
        let mut heartbeat_reader = BufReader::new(heartbeat_stream);
        let mut heartbeat_line = String::new();
        heartbeat_reader.read_line(&mut heartbeat_line).await?;
        let heartbeat: DaemonEnvelope = serde_json::from_str(&heartbeat_line)?;
        match heartbeat.body {
            DaemonRequest::Heartbeat { agent_id } => assert_eq!(agent_id, "assistant"),
            other => panic!("unexpected request: {other:?}"),
        }
        let heartbeat_response = DaemonEnvelope {
            id: heartbeat.id.clone(),
            body: DaemonResponse::Ack {
                message: "heartbeat recorded".into(),
            },
        };
        let heartbeat_payload = serde_json::to_string(&heartbeat_response)? + "
";
        heartbeat_reader
            .get_mut()
            .write_all(heartbeat_payload.as_bytes())
            .await?;

        let (inbox_stream, _) = listener.accept().await?;
        let mut inbox_reader = BufReader::new(inbox_stream);
        let mut inbox_line = String::new();
        inbox_reader.read_line(&mut inbox_line).await?;
        let inbox: DaemonEnvelope = serde_json::from_str(&inbox_line)?;
        match inbox.body {
            DaemonRequest::CheckInbox => {}
            other => panic!("unexpected request: {other:?}"),
        }
        let inbox_response = DaemonEnvelope {
            id: inbox.id.clone(),
            body: DaemonResponse::Inbox {
                messages: vec![InboxMessage {
                    from: "agent-1".into(),
                    content: "Please pick up workflow step 2".into(),
                }],
            },
        };
        let inbox_payload = serde_json::to_string(&inbox_response)? + "
";
        inbox_reader
            .get_mut()
            .write_all(inbox_payload.as_bytes())
            .await?;
        Ok::<(), anyhow::Error>(())
    });

    let server = McpServer::for_workspace(workspace.path().to_path_buf());
    let response = server
        .handle_request(json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "tools/call",
            "params": {
                "name": "check_inbox",
                "arguments": {}
            }
        }))
        .await?;

    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text response");
    assert!(text.contains("agent-1"));
    assert!(text.contains("Please pick up workflow step 2"));
    daemon_task.await??;
    Ok(())
}

#[tokio::test]
async fn send_heartbeat_tool_forwards_agent_id_and_returns_success_text() -> Result<()> {
    let (workspace, daemon_task) = setup_daemon(|envelope| match envelope.body {
        DaemonRequest::Heartbeat { agent_id } => {
            assert_eq!(agent_id, "assistant");
            DaemonResponse::Ack {
                message: "heartbeat recorded".into(),
            }
        }
        other => panic!("unexpected request: {other:?}"),
    })
    .await?;

    let server = McpServer::for_workspace(workspace.path().to_path_buf());
    let response = server
        .handle_request(json!({
            "jsonrpc": "2.0",
            "id": 14,
            "method": "tools/call",
            "params": {
                "name": "send_heartbeat",
                "arguments": {}
            }
        }))
        .await?;

    assert_eq!(
        response["result"]["content"][0]["text"],
        "Heartbeat sent to squad daemon."
    );
    daemon_task.await??;
    Ok(())
}

#[tokio::test]
async fn mark_done_tool_forwards_summary_and_returns_success_text() -> Result<()> {
    let (workspace, daemon_task) = setup_daemon(|envelope| match envelope.body {
        DaemonRequest::MarkDone { summary } => {
            assert_eq!(summary, "review pass: LGTM");
            DaemonResponse::Done {
                message: "recorded".into(),
            }
        }
        other => panic!("unexpected request: {other:?}"),
    })
    .await?;

    let server = McpServer::for_workspace(workspace.path().to_path_buf());
    let response = server
        .handle_request(json!({
            "jsonrpc": "2.0",
            "id": 13,
            "method": "tools/call",
            "params": {
                "name": "mark_done",
                "arguments": {
                    "summary": "review pass: LGTM"
                }
            }
        }))
        .await?;

    assert_eq!(
        response["result"]["content"][0]["text"],
        "Task marked as done. Summary recorded."
    );
    daemon_task.await??;
    Ok(())
}
