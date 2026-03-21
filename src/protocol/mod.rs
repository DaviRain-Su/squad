use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonEnvelope<T = DaemonRequest> {
    pub id: String,
    pub body: T,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DaemonRequest {
    SendMessage { to: String, content: String },
    CheckInbox,
    MarkDone { summary: String },
    Heartbeat { agent_id: String },
    GetAgentStatus { agent_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DaemonResponse {
    Ack { message: String },
    Inbox { messages: Vec<InboxMessage> },
    Done { message: String },
    AgentStatus { agent_status: AgentStatus },
    Error { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxMessage {
    pub from: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub from: String,
    pub content: String,
}

impl Message {
    pub fn new(from: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            content: content.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Online,
    Offline,
}

impl HealthState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStatus {
    pub agent_id: String,
    pub status: String,
    pub health: HealthState,
    pub last_seen_unix: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    Register { agent_id: String, role: String },
    SendMessage {
        from: String,
        to: String,
        content: String,
    },
    CheckInbox { agent_id: String },
    MarkDone { agent_id: String, message: String },
    Heartbeat { agent_id: String },
    GetAgentStatus { agent_id: String },
    PingAgent { agent_id: String },
    Status,
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Ok(Value),
    Error { message: String },
}

impl Response {
    pub fn ok(payload: Value) -> Self {
        Self::Ok(payload)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }
}
