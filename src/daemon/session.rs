use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::daemon::registry::AgentInfo;
use crate::workflow::WorkflowState;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub agents: Vec<AgentInfo>,
    pub workflow_state: WorkflowState,
    pub pending_message_ids: Vec<String>,
    pub saved_at_unix: u64,
}

#[derive(Clone, Debug)]
pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn load(&self) -> Result<Option<SessionState>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let state = serde_json::from_str(&raw).context("failed to parse session state")?;
        Ok(Some(state))
    }

    pub fn save(&self, session: &SessionState) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let payload = serde_json::to_string_pretty(session)?;
        fs::write(&self.path, payload)
            .with_context(|| format!("failed to write {}", self.path.display()))?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => {
                Err(error).with_context(|| format!("failed to remove {}", self.path.display()))
            }
        }
    }
}

impl SessionState {
    pub fn new(
        session_id: String,
        agents: Vec<AgentInfo>,
        workflow_state: WorkflowState,
        pending_message_ids: Vec<String>,
    ) -> Self {
        Self {
            session_id,
            agents,
            workflow_state,
            pending_message_ids,
            saved_at_unix: now_unix_secs(),
        }
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after unix epoch")
        .as_secs()
}
