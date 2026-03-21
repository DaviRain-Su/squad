use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;

use anyhow::{Context, Result};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use super::AgentAdapter;

pub struct WatchAdapter {
    output_path: PathBuf,
    last_seen: Mutex<String>,
    receiver: Mutex<Receiver<notify::Result<notify::Event>>>,
    _watcher: RecommendedWatcher,
}

impl WatchAdapter {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let output_path = path.as_ref().to_path_buf();
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        if !output_path.exists() {
            fs::write(&output_path, "")
                .with_context(|| format!("failed to write {}", output_path.display()))?;
        }

        let initial = fs::read_to_string(&output_path)
            .with_context(|| format!("failed to read {}", output_path.display()))?;
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = tx.send(event);
        })?;
        watcher.watch(&output_path, RecursiveMode::NonRecursive)?;

        Ok(Self {
            output_path,
            last_seen: Mutex::new(initial),
            receiver: Mutex::new(rx),
            _watcher: watcher,
        })
    }
}

impl AgentAdapter for WatchAdapter {
    fn send(&self, message: &str) -> Result<()> {
        fs::write(&self.output_path, message)
            .with_context(|| format!("failed to write {}", self.output_path.display()))?;
        let mut last_seen = self.last_seen.lock().expect("watch last_seen mutex");
        *last_seen = message.to_string();
        Ok(())
    }

    fn poll_output(&self) -> Result<Option<String>> {
        while let Ok(event) = self
            .receiver
            .lock()
            .expect("watch receiver mutex")
            .try_recv()
        {
            let _ = event?;
        }

        let content = fs::read_to_string(&self.output_path)
            .with_context(|| format!("failed to read {}", self.output_path.display()))?;
        let mut last_seen = self.last_seen.lock().expect("watch last_seen mutex");
        if content != *last_seen {
            *last_seen = content.clone();
            return Ok(Some(content));
        }
        Ok(None)
    }
}
