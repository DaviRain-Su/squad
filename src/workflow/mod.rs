pub mod engine;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowState {
    pub current_step: Option<String>,
    pub iteration: usize,
    #[serde(default = "now_unix_secs")]
    pub started_at_unix: u64,
    #[serde(default)]
    pub previous_output: Option<String>,
    #[serde(default)]
    pub active_steps: Vec<String>,
    #[serde(default)]
    pub parallel_outputs: Vec<String>,
}

impl WorkflowState {
    pub fn new(start_at: String) -> Self {
        let current_step = if start_at.trim().is_empty() {
            None
        } else {
            Some(start_at.clone())
        };
        let active_steps = current_step.iter().cloned().collect();
        Self {
            current_step,
            iteration: 0,
            started_at_unix: now_unix_secs(),
            previous_output: None,
            active_steps,
            parallel_outputs: Vec::new(),
        }
    }

    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let payload = serde_json::to_string_pretty(self)?;
        fs::write(path, payload).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    pub fn load_from_path(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&raw).context("failed to parse workflow state")
    }

    pub fn reset(&mut self, start_at: &str) {
        self.current_step = if start_at.trim().is_empty() {
            None
        } else {
            Some(start_at.to_string())
        };
        self.iteration = 0;
        self.previous_output = None;
        self.active_steps = self.current_step.iter().cloned().collect();
        self.parallel_outputs.clear();
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowMessage {
    pub step_id: String,
    pub content: String,
    pub iteration: usize,
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after unix epoch")
        .as_secs()
}
