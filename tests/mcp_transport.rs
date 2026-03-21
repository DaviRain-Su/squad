use anyhow::Result;
use serde_json::json;
use squad::mcp::transport::StdioTransport;
use squad::mcp::McpServer;
use tempfile::tempdir;
use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};

fn frame(value: serde_json::Value) -> Vec<u8> {
    let body = serde_json::to_vec(&value).expect("serialize frame");
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend(body);
    framed
}

async fn read_frame<T>(stream: &mut T) -> Result<serde_json::Value>
where
    T: AsyncReadExt + Unpin,
{
    let mut header = Vec::new();
    let mut buf = [0_u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut buf).await?;
        header.push(buf[0]);
    }

    let header_text = String::from_utf8(header)?;
    let len = header_text
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .expect("content length header")
        .trim()
        .parse::<usize>()?;

    let mut body = vec![0_u8; len];
    stream.read_exact(&mut body).await?;
    Ok(serde_json::from_slice(&body)?)
}

#[tokio::test]
async fn initialize_returns_capabilities_over_mcp_stdio() -> Result<()> {
    let workspace = tempdir()?;
    std::fs::write(workspace.path().join("squad.yaml"), "workflow: {}\n")?;

    let (mut client_side, server_side) = duplex(4096);
    let server = McpServer::for_workspace(workspace.path().to_path_buf());
    let (reader, writer) = tokio::io::split(server_side);

    let task = tokio::spawn(async move {
        let mut transport = StdioTransport::new(reader, writer, server);
        transport.serve_once().await
    });

    client_side
        .write_all(&frame(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {}
            }
        })))
        .await?;

    let response = read_frame(&mut client_side).await?;
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
    assert!(response["result"]["capabilities"]["tools"].is_object());
    assert_eq!(response["result"]["serverInfo"]["name"], "squad-mcp");

    task.await??;
    Ok(())
}

#[tokio::test]
async fn tools_list_returns_three_tools() -> Result<()> {
    let workspace = tempdir()?;
    std::fs::write(workspace.path().join("squad.yaml"), "workflow: {}\n")?;

    let (mut client_side, server_side) = duplex(4096);
    let server = McpServer::for_workspace(workspace.path().to_path_buf());
    let (reader, writer) = tokio::io::split(server_side);

    let task = tokio::spawn(async move {
        let mut transport = StdioTransport::new(reader, writer, server);
        transport.serve_once().await
    });

    client_side
        .write_all(&frame(json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/list"
        })))
        .await?;

    let response = read_frame(&mut client_side).await?;
    let tools = response["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 4);
    assert_eq!(tools[0]["name"], "send_message");
    assert_eq!(tools[1]["name"], "check_inbox");
    assert_eq!(tools[2]["name"], "mark_done");
    assert_eq!(tools[3]["name"], "send_heartbeat");

    task.await??;
    Ok(())
}
