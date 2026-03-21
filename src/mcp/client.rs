use crate::protocol::{AgentStatus, DaemonEnvelope, DaemonRequest, DaemonResponse, InboxMessage};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Clone, Debug)]
pub struct DaemonClient {
    workspace_root: PathBuf,
}

impl DaemonClient {
    pub fn for_workspace(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    pub fn from_cwd() -> Result<Self> {
        let root = Self::discover_workspace_root(std::env::current_dir()?)?;
        Ok(Self::for_workspace(root))
    }

    pub fn discover_workspace_root(start: impl AsRef<Path>) -> Result<PathBuf> {
        let mut current = start.as_ref().to_path_buf();
        loop {
            let candidate = current.join("squad.yaml");
            if candidate.exists() {
                return Ok(current);
            }
            if !current.pop() {
                break;
            }
        }
        bail!("unable to find squad.yaml from current working directory")
    }

    pub fn socket_path(&self) -> PathBuf {
        self.workspace_root.join(".squad/squad.sock")
    }

    pub async fn send_message(&self, to: String, content: String) -> Result<()> {
        match self
            .send(DaemonRequest::SendMessage { to, content })
            .await?
        {
            DaemonResponse::Ack { .. } => Ok(()),
            DaemonResponse::Error { message } => bail!(message),
            other => bail!("unexpected daemon response: {other:?}"),
        }
    }

    pub async fn check_inbox(&self) -> Result<Vec<InboxMessage>> {
        match self.send(DaemonRequest::CheckInbox).await? {
            DaemonResponse::Inbox { messages } => Ok(messages),
            DaemonResponse::Error { message } => bail!(message),
            other => bail!("unexpected daemon response: {other:?}"),
        }
    }

    pub async fn mark_done(&self, summary: String) -> Result<()> {
        match self.send(DaemonRequest::MarkDone { summary }).await? {
            DaemonResponse::Done { .. } => Ok(()),
            DaemonResponse::Error { message } => bail!(message),
            other => bail!("unexpected daemon response: {other:?}"),
        }
    }

    pub async fn send_heartbeat(&self, agent_id: String) -> Result<()> {
        match self.send(DaemonRequest::Heartbeat { agent_id }).await? {
            DaemonResponse::Ack { .. } => Ok(()),
            DaemonResponse::Error { message } => bail!(message),
            other => bail!("unexpected daemon response: {other:?}"),
        }
    }

    pub async fn get_agent_status(&self, agent_id: String) -> Result<AgentStatus> {
        match self.send(DaemonRequest::GetAgentStatus { agent_id }).await? {
            DaemonResponse::AgentStatus { agent_status } => Ok(agent_status),
            DaemonResponse::Error { message } => bail!(message),
            other => bail!("unexpected daemon response: {other:?}"),
        }
    }

    async fn send(&self, request: DaemonRequest) -> Result<DaemonResponse> {
        let stream = UnixStream::connect(self.socket_path())
            .await
            .with_context(|| format!("failed to connect to {}", self.socket_path().display()))?;
        let mut reader = BufReader::new(stream);
        let envelope = DaemonEnvelope {
            id: request_id(),
            body: request,
        };
        let payload = serde_json::to_string(&envelope)? + "\n";
        reader.get_mut().write_all(payload.as_bytes()).await?;

        let mut line = String::new();
        reader.read_line(&mut line).await?;
        if line.is_empty() {
            bail!("daemon closed the socket before sending a response")
        }
        let response: DaemonEnvelope<DaemonResponse> = serde_json::from_str(&line)?;
        Ok(response.body)
    }
}

fn request_id() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos()
        .to_string()
}
