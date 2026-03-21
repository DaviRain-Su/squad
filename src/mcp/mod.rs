pub mod client;
pub mod tools;
pub mod transport;

use crate::mcp::tools::ToolRegistry;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct McpServer {
    workspace_root: PathBuf,
}

impl McpServer {
    pub fn for_workspace(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    pub fn from_cwd() -> Result<Self> {
        let workspace_root =
            client::DaemonClient::discover_workspace_root(std::env::current_dir()?)?;
        Ok(Self::for_workspace(workspace_root))
    }

    pub async fn handle_request(&self, request: Value) -> Result<Value> {
        let method = request["method"]
            .as_str()
            .context("missing JSON-RPC method")?;
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
        let tools = ToolRegistry::new(self.workspace_root.clone());

        let result = match method {
            "initialize" => {
                let protocol_version = params
                    .get("protocolVersion")
                    .cloned()
                    .unwrap_or_else(|| json!("2024-11-05"));
                json!({
                    "protocolVersion": protocol_version,
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "squad-mcp",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                })
            }
            "tools/list" => json!({
                "tools": tools.list_tools()
            }),
            "tools/call" => {
                let name = params["name"].as_str().context("missing tool name")?;
                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let text = tools.call(name, arguments).await?;
                json!({
                    "content": [
                        {
                            "type": "text",
                            "text": text,
                        }
                    ],
                    "isError": false,
                })
            }
            other => anyhow::bail!("unsupported JSON-RPC method: {other}"),
        };

        Ok(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }))
    }

    pub fn error_response(id: Value, message: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32603,
                "message": message,
            }
        })
    }
}
