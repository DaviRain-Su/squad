use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct SquadConfig {
    pub project: String,
    pub agents: BTreeMap<String, AgentConfig>,
    pub persistence: PersistenceConfig,
    pub workflow: WorkflowConfig,
    pub recovery: RecoveryConfig,
    pub heartbeat_timeout_seconds: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentAdapterKind {
    #[default]
    Mcp,
    Hook,
    Watch,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct AgentConfig {
    #[serde(default)]
    pub adapter: AgentAdapterKind,
    #[serde(default)]
    pub hook_script: Option<String>,
    #[serde(default)]
    pub watch_file: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PersistenceConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowMode {
    #[default]
    Loop,
    Pipeline,
    Parallel,
}

impl WorkflowMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Loop => "loop",
            Self::Pipeline => "pipeline",
            Self::Parallel => "parallel",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimeoutPolicy {
    #[default]
    Stop,
    Notify,
    Restart,
}

impl TimeoutPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Notify => "notify",
            Self::Restart => "restart",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    #[default]
    Reconnect,
    Restart,
    Notify,
    Ignore,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RecoveryConfig {
    #[serde(default)]
    pub on_agent_offline: RecoveryAction,
    #[serde(default = "default_reconnect_attempts")]
    pub reconnect_attempts: usize,
    #[serde(default = "default_reconnect_interval_seconds")]
    pub reconnect_interval_seconds: u64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            on_agent_offline: RecoveryAction::Reconnect,
            reconnect_attempts: default_reconnect_attempts(),
            reconnect_interval_seconds: default_reconnect_interval_seconds(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct WorkflowConfig {
    #[serde(default)]
    pub start_at: String,
    #[serde(default)]
    pub mode: WorkflowMode,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    #[serde(default)]
    pub on_timeout: TimeoutPolicy,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub steps: Vec<WorkflowStepConfig>,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            start_at: String::new(),
            mode: WorkflowMode::Loop,
            max_iterations: default_max_iterations(),
            on_timeout: TimeoutPolicy::Stop,
            timeout_seconds: default_timeout_seconds(),
            steps: Vec::new(),
        }
    }
}

impl WorkflowConfig {
    pub fn normalize(&mut self) {
        for (index, step) in self.steps.iter_mut().enumerate() {
            if step.id.trim().is_empty() {
                step.id = if step.agent.trim().is_empty() {
                    format!("step_{}", index + 1)
                } else {
                    step.agent.clone()
                };
            }
        }

        if self.start_at.trim().is_empty() {
            self.start_at = self
                .steps
                .first()
                .map(|step| step.id.clone())
                .unwrap_or_default();
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct WorkflowStepConfig {
    #[serde(default)]
    pub id: String,
    pub agent: String,
    #[serde(default)]
    pub action: String,
    #[serde(default, alias = "then")]
    pub next: Option<String>,
    #[serde(default)]
    pub on_pass: Option<String>,
    #[serde(default)]
    pub on_fail: Option<String>,
    #[serde(default)]
    pub on_timeout: Option<String>,
    #[serde(default, alias = "prompt")]
    pub message: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RawSquadConfig {
    #[serde(default = "default_project")]
    project: String,
    #[serde(default)]
    agents: BTreeMap<String, AgentConfig>,
    #[serde(default)]
    persistence: PersistenceConfig,
    #[serde(default)]
    workflow: WorkflowConfig,
    #[serde(default)]
    recovery: RecoveryConfig,
    #[serde(default = "default_heartbeat_timeout_seconds")]
    heartbeat_timeout_seconds: u64,
    #[serde(default)]
    max_iterations: Option<usize>,
}

impl SquadConfig {
    pub fn from_yaml(raw: &str) -> Result<Self> {
        let mut parsed: RawSquadConfig =
            serde_yaml::from_str(raw).context("failed to parse squad.yaml")?;
        if let Some(max_iterations) = parsed.max_iterations {
            if parsed.workflow.max_iterations == default_max_iterations() {
                parsed.workflow.max_iterations = max_iterations;
            }
        }
        parsed.workflow.normalize();
        Ok(Self {
            project: parsed.project,
            agents: parsed.agents,
            persistence: parsed.persistence,
            workflow: parsed.workflow,
            recovery: parsed.recovery,
            heartbeat_timeout_seconds: parsed.heartbeat_timeout_seconds,
        })
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        Self::from_yaml(&raw)
    }
}

fn default_project() -> String {
    "my-project".to_string()
}

fn default_max_iterations() -> usize {
    10
}

fn default_timeout_seconds() -> u64 {
    300
}

fn default_heartbeat_timeout_seconds() -> u64 {
    30
}

fn default_reconnect_attempts() -> usize {
    3
}

fn default_reconnect_interval_seconds() -> u64 {
    5
}

#[cfg(test)]
mod tests {
    use super::{RecoveryAction, SquadConfig};

    #[test]
    fn parses_recovery_config_with_defaults() {
        let config = SquadConfig::from_yaml(
            r#"
project: demo
workflow:
  start_at: builder
  steps:
    - agent: builder
      prompt: "Build it"
recovery:
  on_agent_offline: reconnect
  reconnect_attempts: 3
  reconnect_interval_seconds: 5
"#,
        )
        .expect("parse config");

        assert_eq!(config.recovery.on_agent_offline, RecoveryAction::Reconnect);
        assert_eq!(config.recovery.reconnect_attempts, 3);
        assert_eq!(config.recovery.reconnect_interval_seconds, 5);
        assert_eq!(config.heartbeat_timeout_seconds, 30);
    }

    #[test]
    fn uses_default_recovery_config_when_omitted() {
        let config = SquadConfig::from_yaml(
            r#"
workflow:
  start_at: builder
  steps:
    - agent: builder
      prompt: "Build it"
"#,
        )
        .expect("parse config");

        assert_eq!(config.recovery.on_agent_offline, RecoveryAction::Reconnect);
        assert_eq!(config.recovery.reconnect_attempts, 3);
        assert_eq!(config.recovery.reconnect_interval_seconds, 5);
        assert_eq!(config.heartbeat_timeout_seconds, 30);
    }
}
