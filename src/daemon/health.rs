use std::path::Path;

use anyhow::Result;

use crate::daemon::registry::{AgentInfo, Registry};
use crate::daemon::{append_watch_message, send_request, DaemonPaths};
use crate::protocol::Request;

#[derive(Clone, Debug)]
pub struct AgentHealthChecker {
    heartbeat_timeout_seconds: u64,
}

impl AgentHealthChecker {
    pub fn new(heartbeat_timeout_seconds: u64) -> Self {
        Self {
            heartbeat_timeout_seconds,
        }
    }

    pub async fn ping_registered_agents(
        &self,
        socket_path: impl AsRef<Path>,
        agents: &[AgentInfo],
    ) -> Result<()> {
        for agent in agents {
            let _ = send_request(
                socket_path.as_ref(),
                &Request::PingAgent {
                    agent_id: agent.agent_id.clone(),
                },
            )
            .await;
        }
        Ok(())
    }

    pub fn mark_stale_agents_offline(
        &self,
        paths: &DaemonPaths,
        registry: &mut Registry,
        now_unix: u64,
    ) -> Result<Vec<AgentInfo>> {
        let stale_agents = registry.stale_agents(self.heartbeat_timeout_seconds, now_unix);
        let mut offline = Vec::new();
        for agent_id in stale_agents {
            if let Some(agent) = registry.mark_offline(&agent_id) {
                append_watch_message(
                    paths,
                    "agent-offline",
                    format!("agent {} marked offline", agent.agent_id),
                )?;
                offline.push(agent);
            }
        }
        Ok(offline)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use crate::daemon::registry::Registry;
    use crate::daemon::DaemonPaths;
    use crate::protocol::HealthState;

    use super::AgentHealthChecker;

    #[test]
    fn marks_stale_agents_offline_and_logs_event() -> Result<()> {
        let workspace = tempdir()?;
        let paths = DaemonPaths::new(workspace.path());
        let checker = AgentHealthChecker::new(30);
        let mut registry = Registry::default();
        let registered = registry.register("agent-1", "implementer");

        let offline = checker.mark_stale_agents_offline(
            &paths,
            &mut registry,
            registered.last_seen_unix + 31,
        )?;
        assert_eq!(offline.len(), 1);
        assert_eq!(offline[0].health, HealthState::Offline);

        let log = std::fs::read_to_string(paths.messages_path())?;
        assert!(log.contains("agent agent-1 marked offline"));
        Ok(())
    }
}
