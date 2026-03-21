use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::config::SquadConfig;
use crate::protocol::{Request, Response};
use crate::workflow::{WorkflowMessage, WorkflowState};
use crate::{adapter, config};

pub mod audit;
pub mod health;
pub mod mailbox;
pub mod recovery;
pub mod registry;
pub mod session;
pub mod server;
pub mod store;

pub use server::DaemonServer;

const DEFAULT_CONFIG_TEMPLATE: &str = r#"project: my-project

agents:
  builder:
    adapter: mcp
  reviewer:
    adapter: mcp

workflow:
  mode: loop
  start_at: implement
  max_iterations: 6
  steps:
    - id: implement
      agent: builder
      action: implement
      message: "Goal: {goal}\nPrevious feedback: {previous_output}\nImplement the required changes."
      next: review
    - id: review
      agent: reviewer
      action: review
      message: "Review iteration {iteration}:\n{previous_output}\nReply PASS if acceptable, FAIL with details if not."
      on_pass: done
      on_fail: implement
"#;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchAgent {
    pub agent_id: String,
    pub role: String,
    pub status: String,
    pub health: String,
    pub last_seen_unix: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchMessage {
    pub label: String,
    pub content: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchSnapshot {
    pub project: String,
    pub workflow_mode: String,
    pub iteration: usize,
    pub max_iterations: usize,
    pub current_step: Option<String>,
    pub started_at_unix: Option<u64>,
    pub running: bool,
    pub agents: Vec<WatchAgent>,
    pub messages: Vec<WatchMessage>,
}

#[async_trait]
pub trait WorkflowDispatcher: Send + Sync {
    async fn dispatch(&self, recipient: &str, message: WorkflowMessage) -> Result<()>;
}

#[async_trait]
pub trait WorkflowStateStore: Send + Sync {
    async fn load(&self) -> Result<WorkflowState>;
    async fn save(&self, state: &WorkflowState) -> Result<()>;
}


#[derive(Clone, Debug)]
pub struct DaemonPaths {
    workspace_root: PathBuf,
}

impl DaemonPaths {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn config_path(&self) -> PathBuf {
        self.workspace_root.join("squad.yaml")
    }

    pub fn runtime_dir(&self) -> PathBuf {
        self.workspace_root.join(".squad")
    }

    pub fn socket_path(&self) -> PathBuf {
        self.runtime_dir().join("squad.sock")
    }

    pub fn pid_path(&self) -> PathBuf {
        self.runtime_dir().join("daemon.pid")
    }

    pub fn state_path(&self) -> PathBuf {
        self.runtime_dir().join("state.json")
    }

    pub fn messages_path(&self) -> PathBuf {
        self.runtime_dir().join("messages.log")
    }

    pub fn messages_db_path(&self) -> PathBuf {
        self.runtime_dir().join("messages.db")
    }

    pub fn session_path(&self) -> PathBuf {
        self.runtime_dir().join("session.json")
    }

    pub fn audit_path(&self) -> PathBuf {
        self.runtime_dir().join("audit.log")
    }

    pub fn ensure_runtime_dir(&self) -> Result<()> {
        fs::create_dir_all(self.runtime_dir()).with_context(|| {
            format!(
                "failed to create runtime dir {}",
                self.runtime_dir().display()
            )
        })?;
        Ok(())
    }

    pub fn cleanup(&self) -> Result<()> {
        remove_if_exists(&self.socket_path())?;
        remove_if_exists(&self.pid_path())?;
        Ok(())
    }
}

pub fn init_workspace(workspace_root: impl AsRef<Path>) -> Result<()> {
    init_workspace_with_options(workspace_root, false)
}

pub fn init_workspace_with_options(workspace_root: impl AsRef<Path>, fresh: bool) -> Result<()> {
    let paths = DaemonPaths::new(workspace_root);
    if paths.config_path().exists() && !fresh {
        eprintln!(
            "squad: {} already exists. Use --force to overwrite.",
            paths.config_path().display()
        );
        return Ok(());
    }
    if fresh {
        clean_history(paths.workspace_root())?;
    }
    fs::write(paths.config_path(), DEFAULT_CONFIG_TEMPLATE)
        .with_context(|| format!("failed to write {}", paths.config_path().display()))?;
    let config = config::SquadConfig::from_path(&paths.config_path())?;
    adapter::maybe_prepare_agent_artifacts(paths.workspace_root(), &config)?;
    Ok(())
}

pub fn start_daemon(workspace_root: impl AsRef<Path>) -> Result<()> {
    let paths = DaemonPaths::new(workspace_root);
    ensure_config_exists(&paths)?;
    let config = SquadConfig::from_path(&paths.config_path())?;
    adapter::validate_agent_config(&config)?;
    adapter::maybe_prepare_agent_artifacts(paths.workspace_root(), &config)?;

    if paths.socket_path().exists() {
        if send_request_blocking(paths.socket_path(), &Request::Status).is_ok() {
            eprintln!("squad: daemon is already running ({})", paths.socket_path().display());
            return Ok(());
        }
        // Stale socket left by a previous SIGKILL or crash — clean it up
        eprintln!("Previous daemon crashed. Cleaning up and starting fresh.");
    }

    remove_if_exists(&paths.socket_path())?;
    paths.ensure_runtime_dir()?;

    let mut child =
        Command::new(std::env::current_exe().context("failed to resolve squad binary path")?)
            .arg("daemon-run")
            .current_dir(paths.workspace_root())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to spawn daemon process")?;

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if paths.socket_path().exists() {
            return Ok(());
        }

        if let Some(status) = child.try_wait().context("failed to poll daemon process")? {
            bail!("daemon exited before creating socket: {status}");
        }

        thread::sleep(Duration::from_millis(50));
    }

    bail!("daemon did not create socket before timeout")
}

pub async fn run_daemon_foreground(workspace_root: impl AsRef<Path>) -> Result<()> {
    let paths = DaemonPaths::new(workspace_root);
    ensure_config_exists(&paths)?;
    let server = DaemonServer::bind(paths).await?;
    server.serve_until_shutdown().await
}

pub fn stop_daemon(workspace_root: impl AsRef<Path>) -> Result<()> {
    let paths = DaemonPaths::new(workspace_root);
    if !paths.socket_path().exists() {
        paths.cleanup()?;
        return Ok(());
    }

    match send_request_blocking(paths.socket_path(), &Request::Shutdown) {
        Ok(_) => {}
        Err(_) => {
            paths.cleanup()?;
            return Ok(());
        }
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !paths.socket_path().exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }

    bail!("daemon socket still exists after shutdown request")
}

pub fn status_text(workspace_root: impl AsRef<Path>) -> Result<String> {
    let paths = DaemonPaths::new(workspace_root);
    if !paths.socket_path().exists() {
        return Ok("running: false\n".to_string());
    }

    let response = match send_request_blocking(paths.socket_path(), &Request::Status) {
        Ok(response) => response,
        Err(_) => return Ok("running: false\n".to_string()),
    };

    let payload = match response {
        Response::Ok(payload) => payload,
        Response::Error { message } => bail!(message),
    };

    let mut lines = vec![format!(
        "running: {}",
        payload["running"].as_bool().unwrap_or(false)
    )];

    if let Some(socket_path) = payload["socket_path"].as_str() {
        lines.push(format!("socket: {socket_path}"));
    }

    if let Some(agents) = payload["agents"].as_array() {
        if agents.is_empty() {
            lines.push("agents: none".to_string());
        } else {
            for agent in agents {
                let agent_id = agent["agent_id"].as_str().unwrap_or("unknown");
                let role = agent["role"].as_str().unwrap_or("unknown");
                let status = agent["status"].as_str().unwrap_or("unknown");
                let health = agent["health"].as_str().unwrap_or("unknown");
                let last_seen = agent["last_seen_unix"].as_u64().unwrap_or(0);
                let rendered = format!("{agent_id} ({role}) [{status}] health={health} last_seen={last_seen}");
                if health.eq_ignore_ascii_case("offline") {
                    lines.push(format!("\x1b[31m{rendered}\x1b[0m"));
                } else {
                    lines.push(rendered);
                }
            }
        }
    }

    Ok(lines.join("\n") + "\n")
}

pub fn log_text(
    workspace_root: impl AsRef<Path>,
    tail: Option<usize>,
    filter: Option<&str>,
) -> Result<String> {
    let paths = DaemonPaths::new(workspace_root);
    let audit = audit::AuditLog::new(paths.audit_path());
    let filter = match filter {
        Some(value) => Some(audit::AuditFilter::parse(value)?),
        None => None,
    };
    audit.render(tail, filter.as_ref())
}

pub fn history_text(workspace_root: impl AsRef<Path>) -> Result<String> {
    let paths = DaemonPaths::new(workspace_root);
    let audit = audit::AuditLog::new(paths.audit_path());
    audit.history_summary()
}

pub fn clean_history(workspace_root: impl AsRef<Path>) -> Result<()> {
    let paths = DaemonPaths::new(workspace_root);
    remove_if_exists(&paths.messages_db_path())?;
    remove_if_exists(&paths.session_path())?;
    remove_if_exists(&paths.audit_path())?;
    remove_if_exists(&paths.state_path())?;
    remove_if_exists(&paths.messages_path())?;
    Ok(())
}

pub fn watch_snapshot(workspace_root: impl AsRef<Path>) -> Result<WatchSnapshot> {
    let paths = DaemonPaths::new(workspace_root);
    let config = if paths.config_path().exists() {
        SquadConfig::from_path(&paths.config_path())?
    } else {
        SquadConfig::from_yaml("workflow: {}\n")?
    };
    let state = if paths.state_path().exists() {
        Some(WorkflowState::load_from_path(&paths.state_path())?)
    } else {
        None
    };

    let mut agents = configured_agents(&config, paths.socket_path().exists());
    let mut running = false;
    if paths.socket_path().exists() {
        if let Ok(Response::Ok(payload)) = send_request_blocking(paths.socket_path(), &Request::Status) {
            running = payload["running"].as_bool().unwrap_or(false);
            for agent in payload["agents"].as_array().into_iter().flatten() {
                let item = WatchAgent {
                    agent_id: agent["agent_id"].as_str().unwrap_or("unknown").to_string(),
                    role: agent["role"].as_str().unwrap_or("unknown").to_string(),
                    status: agent["status"].as_str().unwrap_or("unknown").to_string(),
                    health: agent["health"].as_str().unwrap_or("unknown").to_string(),
                    last_seen_unix: agent["last_seen_unix"].as_u64().unwrap_or(0),
                };
                if let Some(existing) = agents.iter_mut().find(|candidate| candidate.agent_id == item.agent_id) {
                    *existing = item;
                } else {
                    agents.push(item);
                }
            }
        }
    }

    Ok(WatchSnapshot {
        project: config.project,
        workflow_mode: config.workflow.mode.as_str().to_string(),
        iteration: state.as_ref().map(|value| value.iteration).unwrap_or(0),
        max_iterations: config.workflow.max_iterations,
        current_step: state.as_ref().and_then(|value| value.current_step.clone()),
        started_at_unix: state.as_ref().map(|value| value.started_at_unix),
        running,
        agents,
        messages: read_watch_messages(&paths.messages_path(), 32)?,
    })
}

pub async fn send_request(socket_path: impl AsRef<Path>, request: &Request) -> Result<Response> {
    let mut stream = UnixStream::connect(socket_path.as_ref())
        .await
        .with_context(|| format!("failed to connect to {}", socket_path.as_ref().display()))?;
    let payload = serde_json::to_vec(request)?;
    stream.write_all(&payload).await?;
    stream.shutdown().await?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    serde_json::from_slice(&response).context("failed to parse daemon response")
}

fn ensure_config_exists(paths: &DaemonPaths) -> Result<()> {
    if !paths.config_path().exists() {
        bail!("missing {}", paths.config_path().display());
    }
    Ok(())
}

fn send_request_blocking(socket_path: impl AsRef<Path>, request: &Request) -> Result<Response> {
    let socket_path = socket_path.as_ref();
    let mut stream = StdUnixStream::connect(socket_path)
        .with_context(|| format!("failed to connect to {}", socket_path.display()))?;
    let payload = serde_json::to_vec(request)?;
    stream.write_all(&payload)?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    serde_json::from_str(&response).context("failed to parse daemon response")
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

pub(crate) fn append_watch_message(paths: &DaemonPaths, label: impl Into<String>, content: impl Into<String>) -> Result<()> {
    paths.ensure_runtime_dir()?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.messages_path())
        .with_context(|| format!("failed to open {}", paths.messages_path().display()))?;
    let record = WatchMessage {
        label: label.into(),
        content: content.into(),
    };
    let payload = serde_json::to_string(&record)?;
    use std::io::Write as _;
    writeln!(file, "{payload}")?;
    Ok(())
}

fn configured_agents(config: &SquadConfig, socket_exists: bool) -> Vec<WatchAgent> {
    let mut agents = Vec::new();
    for step in &config.workflow.steps {
        if agents.iter().any(|candidate: &WatchAgent| candidate.agent_id == step.agent) {
            continue;
        }
        agents.push(WatchAgent {
            agent_id: step.agent.clone(),
            role: if step.action.is_empty() {
                "configured".to_string()
            } else {
                step.action.clone()
            },
            status: if socket_exists {
                "idle".to_string()
            } else {
                "offline".to_string()
            },
            health: if socket_exists {
                "online".to_string()
            } else {
                "offline".to_string()
            },
            last_seen_unix: 0,
        });
    }
    agents.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
    agents
}

fn read_watch_messages(path: &Path, limit: usize) -> Result<Vec<WatchMessage>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut messages = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(parsed) = serde_json::from_str::<WatchMessage>(line) {
            messages.push(parsed);
        } else {
            messages.push(WatchMessage {
                label: "system".to_string(),
                content: line.to_string(),
            });
        }
    }
    if messages.len() > limit {
        Ok(messages.split_off(messages.len() - limit))
    } else {
        Ok(messages)
    }
}
