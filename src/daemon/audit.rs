use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventKind {
    AgentRegistered,
    MessageSent,
    MessageDelivered,
    WorkflowAdvanced,
    AgentOffline,
}

impl AuditEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentRegistered => "AgentRegistered",
            Self::MessageSent => "MessageSent",
            Self::MessageDelivered => "MessageDelivered",
            Self::WorkflowAdvanced => "WorkflowAdvanced",
            Self::AgentOffline => "AgentOffline",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub session_id: String,
    pub timestamp: u64,
    pub event: AuditEventKind,
    pub agent: Option<String>,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditFilter {
    key: String,
    value: String,
}

#[derive(Clone, Debug)]
pub struct AuditLog {
    path: PathBuf,
}

impl AuditLog {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn append(
        &mut self,
        session_id: impl Into<String>,
        event: AuditEventKind,
        agent: Option<&str>,
        detail: impl Into<String>,
    ) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let entry = AuditEntry {
            session_id: session_id.into(),
            timestamp: now_unix_secs(),
            event,
            agent: agent.map(|value| value.to_string()),
            detail: detail.into(),
        };
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("failed to open {}", self.path.display()))?;
        let payload = serde_json::to_string(&entry)?;
        writeln!(file, "{payload}")?;
        Ok(())
    }

    pub fn read_entries(
        &self,
        tail: Option<usize>,
        filter: Option<&AuditFilter>,
    ) -> Result<Vec<AuditEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let mut entries = Vec::new();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry: AuditEntry = serde_json::from_str(line).with_context(|| {
                format!("failed to parse audit entry in {}", self.path.display())
            })?;
            if filter.map(|rule| rule.matches(&entry)).unwrap_or(true) {
                entries.push(entry);
            }
        }
        if let Some(limit) = tail {
            if entries.len() > limit {
                entries = entries.split_off(entries.len() - limit);
            }
        }
        Ok(entries)
    }

    pub fn render(&self, tail: Option<usize>, filter: Option<&AuditFilter>) -> Result<String> {
        let entries = self.read_entries(tail, filter)?;
        if entries.is_empty() {
            return Ok("no audit events\n".to_string());
        }
        let lines: Vec<String> = entries
            .into_iter()
            .map(|entry| {
                format!(
                    "[{}] {} session={} agent={} {}",
                    entry.timestamp,
                    entry.event.as_str(),
                    entry.session_id,
                    entry.agent.as_deref().unwrap_or("-"),
                    entry.detail
                )
            })
            .collect();
        Ok(lines.join("\n") + "\n")
    }

    pub fn history_summary(&self) -> Result<String> {
        let entries = self.read_entries(None, None)?;
        if entries.is_empty() {
            return Ok("no session history\n".to_string());
        }
        let mut sessions: BTreeMap<String, (BTreeSet<String>, usize)> = BTreeMap::new();
        for entry in entries {
            let item = sessions
                .entry(entry.session_id.clone())
                .or_insert_with(|| (BTreeSet::new(), 0));
            if let Some(agent) = entry.agent {
                item.0.insert(agent);
            }
            if entry.event == AuditEventKind::MessageSent {
                item.1 += 1;
            }
        }

        let mut lines = Vec::new();
        for (session_id, (agents, messages)) in sessions {
            let agents = if agents.is_empty() {
                "-".to_string()
            } else {
                agents.into_iter().collect::<Vec<_>>().join(",")
            };
            lines.push(format!(
                "session={} agents={} messages={}",
                session_id, agents, messages
            ));
        }
        Ok(lines.join("\n") + "\n")
    }
}

impl AuditFilter {
    pub fn parse(raw: &str) -> Result<Self> {
        let Some((key, value)) = raw.split_once('=') else {
            bail!("filter must use key=value syntax");
        };
        Ok(Self {
            key: key.trim().to_string(),
            value: value.trim().to_string(),
        })
    }

    fn matches(&self, entry: &AuditEntry) -> bool {
        match self.key.as_str() {
            "agent" => entry.agent.as_deref() == Some(self.value.as_str()),
            "event" => entry.event.as_str() == self.value,
            "session" => entry.session_id == self.value,
            _ => false,
        }
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after unix epoch")
        .as_secs()
}
