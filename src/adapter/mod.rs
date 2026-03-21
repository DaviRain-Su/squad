use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::config::{AgentAdapterKind, AgentConfig, SquadConfig};
use crate::mcp::client::DaemonClient;

mod hook;
mod watcher;

pub use hook::HookAdapter;
pub use watcher::WatchAdapter;

pub trait AgentAdapter {
    fn send(&self, message: &str) -> Result<()>;
    fn poll_output(&self) -> Result<Option<String>>;
}

#[derive(Clone, Debug)]
pub struct McpAdapter {
    workspace_root: PathBuf,
    target: String,
}

impl McpAdapter {
    pub fn new(workspace_root: impl AsRef<Path>, target: impl Into<String>) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
            target: target.into(),
        }
    }
}

impl AgentAdapter for McpAdapter {
    fn send(&self, message: &str) -> Result<()> {
        let target = self.target.clone();
        let content = message.to_string();
        let workspace_root = self.workspace_root.clone();
        let runtime = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
        runtime.block_on(async move {
            DaemonClient::for_workspace(workspace_root)
                .send_message(target, content)
                .await
        })
    }

    fn poll_output(&self) -> Result<Option<String>> {
        let workspace_root = self.workspace_root.clone();
        let runtime = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
        runtime.block_on(async move {
            let messages = DaemonClient::for_workspace(workspace_root).check_inbox().await?;
            Ok(messages.into_iter().next().map(|message| message.content))
        })
    }
}

pub fn build_adapter(
    workspace_root: impl AsRef<Path>,
    agent_name: &str,
    config: &SquadConfig,
) -> Result<Box<dyn AgentAdapter>> {
    let agent = config
        .agents
        .get(agent_name)
        .cloned()
        .unwrap_or_else(AgentConfig::default);

    match agent.adapter {
        AgentAdapterKind::Mcp => Ok(Box::new(McpAdapter::new(workspace_root, agent_name))),
        AgentAdapterKind::Hook => {
            let script = agent
                .hook_script
                .context("hook adapter requires hook_script")?;
            Ok(Box::new(HookAdapter::new(script)))
        }
        AgentAdapterKind::Watch => {
            let path = agent
                .watch_file
                .context("watch adapter requires watch_file")?;
            Ok(Box::new(WatchAdapter::new(workspace_root.as_ref().join(path))?))
        }
    }
}

pub fn write_example_hooks(workspace_root: impl AsRef<Path>) -> Result<()> {
    let hooks_dir = workspace_root.as_ref().join(".squad/hooks");
    std::fs::create_dir_all(&hooks_dir)
        .with_context(|| format!("failed to create {}", hooks_dir.display()))?;

    let scripts = [
        (
            hooks_dir.join("on_complete.sh"),
            "#!/bin/sh\n# Example completion hook\nsquad-hook send \"$1\" \"$2\"\n",
        ),
        (
            hooks_dir.join("codex.sh"),
            "#!/bin/sh\n# Example Codex hook\nsquad-hook send \"$1\" \"$SQUAD_MESSAGE\"\n",
        ),
    ];

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    for (path, content) in scripts {
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write {}", path.display()))?;
        #[cfg(unix)]
        {
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms)?;
        }
    }

    Ok(())
}

pub fn ensure_output_file(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if !path.exists() {
        std::fs::write(path, "")
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

pub fn maybe_prepare_agent_artifacts(workspace_root: impl AsRef<Path>, config: &SquadConfig) -> Result<()> {
    write_example_hooks(&workspace_root)?;
    for agent in config.agents.values() {
        if agent.adapter == AgentAdapterKind::Watch {
            if let Some(path) = &agent.watch_file {
                ensure_output_file(workspace_root.as_ref().join(path))?;
            }
        }
    }
    Ok(())
}

pub fn adapter_name(kind: &AgentAdapterKind) -> &'static str {
    match kind {
        AgentAdapterKind::Mcp => "mcp",
        AgentAdapterKind::Hook => "hook",
        AgentAdapterKind::Watch => "watch",
    }
}

pub fn validate_agent_config(config: &SquadConfig) -> Result<()> {
    for (agent_name, agent) in &config.agents {
        match agent.adapter {
            AgentAdapterKind::Mcp => {}
            AgentAdapterKind::Hook if agent.hook_script.is_none() => {
                bail!("agent {agent_name} uses hook adapter but has no hook_script")
            }
            AgentAdapterKind::Watch if agent.watch_file.is_none() => {
                bail!("agent {agent_name} uses watch adapter but has no watch_file")
            }
            _ => {}
        }
    }
    Ok(())
}
