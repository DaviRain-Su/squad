use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::protocol::{AgentStatus, HealthState};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentInfo {
    pub agent_id: String,
    pub role: String,
    pub status: String,
    pub health: HealthState,
    pub registered_at: u64,
    pub last_seen_unix: u64,
}

#[derive(Debug, Default)]
pub struct Registry {
    agents: HashMap<String, AgentInfo>,
}

impl Registry {
    pub fn register(&mut self, agent_id: impl Into<String>, role: impl Into<String>) -> AgentInfo {
        let agent_id = agent_id.into();
        let now = now_unix_secs();
        let info = AgentInfo {
            agent_id: agent_id.clone(),
            role: role.into(),
            status: "idle".to_string(),
            health: HealthState::Online,
            registered_at: now,
            last_seen_unix: now,
        };
        self.agents.insert(agent_id, info.clone());
        info
    }

    pub fn mark_done(&mut self, agent_id: &str) -> Option<AgentInfo> {
        let agent = self.agents.get_mut(agent_id)?;
        agent.status = "done".to_string();
        agent.health = HealthState::Online;
        agent.last_seen_unix = now_unix_secs();
        Some(agent.clone())
    }

    pub fn set_status(&mut self, agent_id: &str, status: impl Into<String>) -> Option<AgentInfo> {
        let agent = self.agents.get_mut(agent_id)?;
        agent.status = status.into();
        Some(agent.clone())
    }

    pub fn heartbeat(&mut self, agent_id: &str) -> Option<AgentInfo> {
        let agent = self.agents.get_mut(agent_id)?;
        agent.health = HealthState::Online;
        agent.last_seen_unix = now_unix_secs();
        Some(agent.clone())
    }

    pub fn mark_offline(&mut self, agent_id: &str) -> Option<AgentInfo> {
        let agent = self.agents.get_mut(agent_id)?;
        agent.health = HealthState::Offline;
        agent.status = "offline".to_string();
        Some(agent.clone())
    }

    pub fn get(&self, agent_id: &str) -> Option<&AgentInfo> {
        self.agents.get(agent_id)
    }

    pub fn get_status(&self, agent_id: &str) -> Option<AgentStatus> {
        self.agents.get(agent_id).map(|agent| AgentStatus {
            agent_id: agent.agent_id.clone(),
            status: agent.status.clone(),
            health: agent.health.clone(),
            last_seen_unix: agent.last_seen_unix,
        })
    }

    pub fn stale_agents(&self, heartbeat_timeout_seconds: u64, now_unix: u64) -> Vec<String> {
        self.agents
            .values()
            .filter(|agent| {
                now_unix.saturating_sub(agent.last_seen_unix) > heartbeat_timeout_seconds
                    && agent.health != HealthState::Offline
            })
            .map(|agent| agent.agent_id.clone())
            .collect()
    }

    pub fn list(&self) -> Vec<AgentInfo> {
        let mut agents: Vec<_> = self.agents.values().cloned().collect();
        agents.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
        agents
    }

    pub fn restore(&mut self, agents: Vec<AgentInfo>) {
        self.agents.clear();
        for agent in agents {
            self.agents.insert(agent.agent_id.clone(), agent);
        }
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after unix epoch")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::Registry;
    use crate::protocol::HealthState;

    #[test]
    fn registers_agents_and_marks_them_done() {
        let mut registry = Registry::default();
        let registered = registry.register("codex", "reviewer");
        assert_eq!(registered.status, "idle");
        assert_eq!(registered.health, HealthState::Online);

        let updated = registry.mark_done("codex").expect("registered agent");
        assert_eq!(updated.status, "done");
        assert_eq!(registry.list().len(), 1);
    }

    #[test]
    fn marks_agent_offline_when_stale() {
        let mut registry = Registry::default();
        let registered = registry.register("codex", "reviewer");
        let stale_now = registered.last_seen_unix + 31;

        let stale = registry.stale_agents(30, stale_now);
        assert_eq!(stale, vec!["codex".to_string()]);

        let updated = registry.mark_offline("codex").expect("offline agent");
        assert_eq!(updated.health, HealthState::Offline);
        assert_eq!(updated.status, "offline");
    }
}
