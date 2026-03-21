use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use super::AgentAdapter;

#[derive(Clone, Debug)]
pub struct HookAdapter {
    script: PathBuf,
}

impl HookAdapter {
    pub fn new(script: impl AsRef<Path>) -> Self {
        Self {
            script: script.as_ref().to_path_buf(),
        }
    }
}

impl AgentAdapter for HookAdapter {
    fn send(&self, message: &str) -> Result<()> {
        let status = Command::new(&self.script)
            .env("SQUAD_MESSAGE", message)
            .status()
            .with_context(|| format!("failed to run {}", self.script.display()))?;
        if !status.success() {
            bail!("hook script exited with {status}");
        }
        Ok(())
    }

    fn poll_output(&self) -> Result<Option<String>> {
        Ok(None)
    }
}
