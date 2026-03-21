use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Pending,
    Delivered,
    Read,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub content: String,
    pub timestamp: u64,
    pub status: MessageStatus,
}

#[derive(Clone, Debug)]
pub struct MessageStore {
    path: PathBuf,
}

impl MessageStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn append_message(
        &mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<StoredMessage> {
        let message = StoredMessage {
            id: Ulid::new().to_string(),
            from: from.into(),
            to: to.into(),
            content: content.into(),
            timestamp: now_unix_secs(),
            status: MessageStatus::Pending,
        };
        self.append_record(&message)?;
        Ok(message)
    }

    pub fn mark_delivered_for_agent(&mut self, agent_id: &str) -> Result<Option<StoredMessage>> {
        let mut message = match self
            .pending_messages_for_agent(agent_id)?
            .into_iter()
            .next()
        {
            Some(message) => message,
            None => return Ok(None),
        };
        message.status = MessageStatus::Delivered;
        self.append_record(&message)?;
        Ok(Some(message))
    }

    pub fn pending_messages_for_agent(&self, agent_id: &str) -> Result<Vec<StoredMessage>> {
        let mut messages: Vec<_> = self
            .pending_messages()?
            .into_iter()
            .filter(|message| message.to == agent_id)
            .collect();
        messages.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then(left.id.cmp(&right.id))
        });
        Ok(messages)
    }

    pub fn pending_messages(&self) -> Result<Vec<StoredMessage>> {
        let mut messages: Vec<_> = self
            .latest_messages()?
            .into_values()
            .filter(|message| message.status == MessageStatus::Pending)
            .collect();
        messages.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then(left.id.cmp(&right.id))
        });
        Ok(messages)
    }

    pub fn pending_message_ids(&self) -> Result<Vec<String>> {
        let mut ids: Vec<_> = self
            .latest_messages()?
            .into_values()
            .filter(|message| message.status == MessageStatus::Pending)
            .map(|message| message.id)
            .collect();
        ids.sort();
        Ok(ids)
    }

    fn append_record(&self, message: &StoredMessage) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("failed to open {}", self.path.display()))?;
        let payload = serde_json::to_string(message)?;
        writeln!(file, "{payload}")?;
        Ok(())
    }

    fn latest_messages(&self) -> Result<BTreeMap<String, StoredMessage>> {
        if !self.path.exists() {
            return Ok(BTreeMap::new());
        }
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let mut messages = BTreeMap::new();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let message: StoredMessage = serde_json::from_str(line).with_context(|| {
                format!("failed to parse message record in {}", self.path.display())
            })?;
            messages.insert(message.id.clone(), message);
        }
        Ok(messages)
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after unix epoch")
        .as_secs()
}
