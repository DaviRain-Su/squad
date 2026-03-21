use anyhow::Result;

use crate::config::{RecoveryAction, RecoveryConfig};
use crate::daemon::{append_watch_message, DaemonPaths};

#[derive(Clone, Debug)]
pub struct RecoveryPolicy {
    config: RecoveryConfig,
}

impl RecoveryPolicy {
    pub fn new(config: RecoveryConfig) -> Self {
        Self { config }
    }

    pub fn action(&self) -> &RecoveryAction {
        &self.config.on_agent_offline
    }

    pub fn reconnect_attempts(&self) -> usize {
        self.config.reconnect_attempts
    }

    pub fn reconnect_interval_seconds(&self) -> u64 {
        self.config.reconnect_interval_seconds
    }

    pub fn handle_agent_offline(&self, paths: &DaemonPaths, agent_id: &str) -> Result<()> {
        match self.config.on_agent_offline {
            RecoveryAction::Reconnect => append_watch_message(
                paths,
                "recovery",
                format!(
                    "agent {agent_id} offline; waiting for reconnect ({} attempts, {}s interval)",
                    self.config.reconnect_attempts, self.config.reconnect_interval_seconds
                ),
            ),
            RecoveryAction::Restart => append_watch_message(
                paths,
                "recovery",
                format!(
                    "agent {agent_id} offline; restart requested ({} attempts, {}s interval)",
                    self.config.reconnect_attempts, self.config.reconnect_interval_seconds
                ),
            ),
            RecoveryAction::Notify => append_watch_message(
                paths,
                "recovery",
                format!("agent {agent_id} offline; notify only"),
            ),
            RecoveryAction::Ignore => append_watch_message(
                paths,
                "recovery",
                format!("agent {agent_id} offline; ignoring"),
            ),
        }
    }
}
