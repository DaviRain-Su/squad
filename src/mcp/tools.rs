use crate::mcp::client::DaemonClient;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;

/// Map a daemon connection error to a user-friendly message that an AI agent
/// can act on. All other errors are passed through unchanged.
fn daemon_unreachable(err: anyhow::Error) -> anyhow::Error {
    let msg = err.to_string();
    if msg.contains("failed to connect")
        || msg.contains("No such file")
        || msg.contains("Connection refused")
        || msg.contains("os error 2")
    {
        anyhow::anyhow!("Squad daemon is not running. Please run squad start.")
    } else {
        err
    }
}

#[derive(Clone, Debug)]
pub struct ToolRegistry {
    workspace_root: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl ToolRegistry {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    pub fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "send_message".into(),
                description: "Send a message to another collaborating agent.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "to": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["to", "content"]
                }),
            },
            ToolDefinition {
                name: "check_inbox".into(),
                description: "Call this after completing any task to check for messages from collaborating agents. Required for squad workflow.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "mark_done".into(),
                description: "Mark the current task as done and record a summary for workflow routing.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "summary": { "type": "string" }
                    },
                    "required": ["summary"]
                }),
            },
            ToolDefinition {
                name: "send_heartbeat".into(),
                description: "Send a heartbeat to notify squad daemon you are active".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "start_workflow".into(),
                description: "Start the squad workflow with a goal. The first agent will receive the goal as its initial task message.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "goal": { "type": "string", "description": "The goal or task description for the workflow" }
                    },
                    "required": ["goal"]
                }),
            },
        ]
    }

    pub async fn call(&self, name: &str, arguments: Value) -> Result<String> {
        let client = DaemonClient::for_workspace(self.workspace_root.clone());
        let agent_id = std::env::var("SQUAD_AGENT_ID").unwrap_or_else(|_| "assistant".to_string());
        match name {
            "send_message" => {
                let to = arguments["to"]
                    .as_str()
                    .context("send_message requires 'to'")?
                    .to_string();
                let content = arguments["content"]
                    .as_str()
                    .context("send_message requires 'content'")?
                    .to_string();
                client
                    .send_message(to.clone(), content)
                    .await
                    .map_err(daemon_unreachable)?;
                Ok(format!(
                    "Message sent to {to}. They will process it on their next check."
                ))
            }
            "check_inbox" => {
                client
                    .send_heartbeat(agent_id)
                    .await
                    .map_err(daemon_unreachable)?;
                let messages = client.check_inbox().await.map_err(daemon_unreachable)?;
                if messages.is_empty() {
                    Ok("No new messages".into())
                } else {
                    Ok(messages
                        .into_iter()
                        .map(|message| format!("From {}: {}", message.from, message.content))
                        .collect::<Vec<_>>()
                        .join("\n"))
                }
            }
            "mark_done" => {
                let summary = arguments["summary"]
                    .as_str()
                    .context("mark_done requires 'summary'")?
                    .to_string();
                client
                    .mark_done(summary)
                    .await
                    .map_err(daemon_unreachable)?;
                Ok("Task marked as done. Summary recorded.".into())
            }
            "send_heartbeat" => {
                client
                    .send_heartbeat(agent_id)
                    .await
                    .map_err(daemon_unreachable)?;
                Ok("Heartbeat sent to squad daemon.".into())
            }
            "start_workflow" => {
                let goal = arguments["goal"]
                    .as_str()
                    .context("start_workflow requires 'goal'")?
                    .to_string();
                client
                    .start_workflow(goal)
                    .await
                    .map_err(daemon_unreachable)?;
                Ok("Workflow started. The first agent will receive the task.".into())
            }
            other => anyhow::bail!("unknown tool: {other}"),
        }
    }
}
