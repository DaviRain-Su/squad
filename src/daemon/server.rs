use std::fs;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::select;
use tokio::sync::{watch, Mutex};
use ulid::Ulid;

use crate::config::SquadConfig;
use crate::daemon::audit::{AuditEventKind, AuditLog};
use crate::daemon::health::AgentHealthChecker;
use crate::daemon::mailbox::Mailbox;
use crate::daemon::recovery::RecoveryPolicy;
use crate::daemon::registry::Registry;
use crate::daemon::session::{SessionState, SessionStore};
use crate::daemon::store::MessageStore;
use crate::daemon::{append_watch_message, DaemonPaths};
use crate::protocol::{
    DaemonEnvelope, DaemonRequest, DaemonResponse, InboxMessage, Message, Request, Response,
};
use crate::workflow::engine::WorkflowEngine;
use crate::workflow::WorkflowState;

#[derive(Debug)]
struct DaemonState {
    mailbox: Mailbox,
    registry: Registry,
    workflow_config: crate::config::WorkflowConfig,
    workflow_state: WorkflowState,
    persistence_enabled: bool,
    session_id: String,
    message_store: MessageStore,
    audit_log: AuditLog,
    /// Agent IDs that are valid targets — populated from config and updated on Register.
    configured_agents: std::collections::HashSet<String>,
}

#[derive(Clone)]
struct ServerDispatcher {
    paths: DaemonPaths,
    state: Arc<Mutex<DaemonState>>,
}

pub struct DaemonServer {
    listener: UnixListener,
    paths: DaemonPaths,
    shutdown_tx: watch::Sender<bool>,
    state: Arc<Mutex<DaemonState>>,
    health_checker: AgentHealthChecker,
    recovery_policy: RecoveryPolicy,
}

#[async_trait]
impl crate::daemon::WorkflowDispatcher for ServerDispatcher {
    async fn dispatch(&self, recipient: &str, message: crate::workflow::WorkflowMessage) -> Result<()> {
        let mut state = self.state.lock().await;
        state
            .mailbox
            .push(recipient.to_string(), Message::new("workflow", message.content.clone()));
        let _ = state.registry.set_status(recipient, "working");
        let _ = state.registry.heartbeat(recipient);
        drop(state);
        append_watch_message(
            &self.paths,
            format!("workflow -> {recipient}"),
            message.content,
        )
    }
}

impl DaemonServer {
    pub async fn bind(paths: DaemonPaths) -> Result<Self> {
        paths.ensure_runtime_dir()?;
        if paths.socket_path().exists() {
            fs::remove_file(paths.socket_path()).with_context(|| {
                format!(
                    "failed to remove stale socket {}",
                    paths.socket_path().display()
                )
            })?;
        }

        let listener = UnixListener::bind(paths.socket_path())
            .with_context(|| format!("failed to bind {}", paths.socket_path().display()))?;
        fs::write(paths.pid_path(), std::process::id().to_string())
            .with_context(|| format!("failed to write {}", paths.pid_path().display()))?;

        let config = SquadConfig::from_path(&paths.config_path())?;
        let persistence_enabled = config.persistence.enabled;
        let mut configured_agents: std::collections::HashSet<String> =
            config.agents.keys().cloned().collect();
        for step in &config.workflow.steps {
            configured_agents.insert(step.agent.clone());
        }
        let mut workflow_state = if paths.state_path().exists() {
            WorkflowState::load_from_path(&paths.state_path())?
        } else {
            let state = WorkflowState::new(config.workflow.start_at.clone());
            state.save_to_path(&paths.state_path())?;
            state
        };
        let mut registry = Registry::default();
        let mut mailbox = Mailbox::default();
        let message_store = MessageStore::new(paths.messages_db_path());
        let audit_log = AuditLog::new(paths.audit_path());
        let mut session_id = Ulid::new().to_string();

        if persistence_enabled {
            if let Some(session) = SessionStore::new(paths.session_path()).load()? {
                session_id = session.session_id;
                registry.restore(session.agents);
                workflow_state = session.workflow_state;
            }
            for message in message_store.pending_messages()? {
                mailbox.push(message.to.clone(), Message::new(message.from, message.content));
            }
        }

        let (shutdown_tx, _) = watch::channel(false);
        let state = Arc::new(Mutex::new(DaemonState {
            mailbox,
            registry,
            workflow_config: config.workflow.clone(),
            workflow_state,
            persistence_enabled,
            session_id,
            message_store,
            audit_log,
            configured_agents,
        }));

        Ok(Self {
            listener,
            paths,
            shutdown_tx,
            state,
            health_checker: AgentHealthChecker::new(config.heartbeat_timeout_seconds),
            recovery_policy: RecoveryPolicy::new(config.recovery),
        })
    }

    pub async fn serve_until_shutdown(self) -> Result<()> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let listener = self.listener;
        let paths = self.paths.clone();
        let shutdown_tx = self.shutdown_tx.clone();
        let state = self.state.clone();
        let health_checker = self.health_checker.clone();
        let recovery_policy = self.recovery_policy.clone();

        loop {
            select! {
                accept_result = listener.accept() => {
                    let (stream, _) = accept_result?;
                    let state = state.clone();
                    let shutdown_tx = shutdown_tx.clone();
                    let paths = paths.clone();
                    tokio::spawn(async move {
                        if let Err(error) = handle_connection(stream, state, shutdown_tx, paths).await {
                            eprintln!("failed to handle daemon connection: {error:#}");
                        }
                    });
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    let registry_snapshot = { state.lock().await.registry.list() };
                    if !registry_snapshot.is_empty() {
                        let _ = health_checker
                            .ping_registered_agents(paths.socket_path(), &registry_snapshot)
                            .await;
                        let now = now_unix_secs();
                        let offline = {
                            let mut guard = state.lock().await;
                            health_checker
                                .mark_stale_agents_offline(&paths, &mut guard.registry, now)
                                .unwrap_or_default()
                        };
                        for agent in offline {
                            let _ = recovery_policy.handle_agent_offline(&paths, &agent.agent_id);
                        }
                    }
                }
                changed = shutdown_rx.changed() => {
                    match changed {
                        Ok(()) if *shutdown_rx.borrow() => break,
                        Ok(()) => {}
                        Err(_) => break,
                    }
                }
            }
        }

        record_offline_agents(state.clone()).await?;
        persist_session(state, &paths).await?;
        paths.cleanup()?;
        Ok(())
    }
}

async fn handle_connection(
    mut stream: UnixStream,
    state: Arc<Mutex<DaemonState>>,
    shutdown_tx: watch::Sender<bool>,
    paths: DaemonPaths,
) -> Result<()> {
    let mut buffer = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        let read = stream.read(&mut byte).await?;
        if read == 0 {
            break;
        }
        buffer.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }

    if buffer.is_empty() {
        return Ok(());
    }

    let payload = if let Ok(request) = serde_json::from_slice::<Request>(&buffer) {
        let response = process_legacy_request(state, shutdown_tx, paths, request).await;
        serde_json::to_vec(&response)?
    } else {
        let envelope = serde_json::from_slice::<DaemonEnvelope>(&buffer)
            .context("failed to parse daemon request")?;
        let body = process_envelope_request(state, shutdown_tx, paths, envelope.body).await;
        let mut payload = serde_json::to_vec(&DaemonEnvelope {
            id: envelope.id,
            body,
        })?;
        payload.push(b'\n');
        payload
    };

    stream.write_all(&payload).await?;
    Ok(())
}

async fn process_legacy_request(
    state: Arc<Mutex<DaemonState>>,
    shutdown_tx: watch::Sender<bool>,
    paths: DaemonPaths,
    request: Request,
) -> Response {
    match request {
        Request::Register { agent_id, role } => {
            let mut guard = state.lock().await;
            guard.configured_agents.insert(agent_id.clone());
            let registered = guard.registry.register(agent_id.clone(), role.clone());
            let _ = record_audit(
                &mut guard,
                AuditEventKind::AgentRegistered,
                Some(agent_id.as_str()),
                format!("role={role}"),
            );
            drop(guard);
            let _ = persist_session(state.clone(), &paths).await;
            Response::ok(json!(registered))
        }
        Request::SendMessage { from, to, content } => {
            let content_for_log = content.clone();
            let to_for_log = to.clone();
            let from_for_log = from.clone();
            let mut guard = state.lock().await;
            if !guard.configured_agents.contains(&to_for_log) {
                return Response::Error {
                    message: format!(
                        "unknown agent '{to_for_log}': not in configured agents or registered"
                    ),
                };
            }
            guard.mailbox.push(to, Message::new(from, content));
            let _ = guard.registry.set_status(&to_for_log, "working");
            let _ = guard.registry.heartbeat(&to_for_log);
            if guard.persistence_enabled {
                let _ = guard.message_store.append_message(
                    from_for_log.clone(),
                    to_for_log.clone(),
                    content_for_log.clone(),
                );
            }
            let _ = record_audit(
                &mut guard,
                AuditEventKind::MessageSent,
                Some(to_for_log.as_str()),
                format!("{from_for_log} -> {to_for_log}: {content_for_log}"),
            );
            drop(guard);
            let _ = append_watch_message(&paths, format!("{from_for_log} -> {to_for_log}"), content_for_log);
            let _ = persist_session(state.clone(), &paths).await;
            Response::ok(json!({ "queued": 1 }))
        }
        Request::CheckInbox { agent_id } => {
            let mut guard = state.lock().await;
            let unknown_warning = if !guard.configured_agents.contains(&agent_id) {
                Some(format!(
                    "agent '{agent_id}' is not in configured agents; inbox may always be empty"
                ))
            } else {
                None
            };
            let _ = guard.registry.heartbeat(&agent_id);
            let message = guard.mailbox.pop(&agent_id);
            if message.is_some() {
                let _ = guard.registry.set_status(&agent_id, "working");
                if guard.persistence_enabled {
                    let _ = guard.message_store.mark_delivered_for_agent(&agent_id);
                }
                let _ = record_audit(
                    &mut guard,
                    AuditEventKind::MessageDelivered,
                    Some(agent_id.as_str()),
                    format!("delivered to {agent_id}"),
                );
            }
            drop(guard);
            let _ = persist_session(state.clone(), &paths).await;
            Response::ok(json!({ "message": message, "warning": unknown_warning }))
        }
        Request::MarkDone { agent_id, message } => {
            let _ = advance_workflow(state.clone(), paths.clone(), &agent_id, &message).await;
            let mut guard = state.lock().await;
            let status = guard.registry.mark_done(&agent_id).map(|agent| agent.status);
            let agent_status = guard.registry.get_status(&agent_id);
            Response::ok(json!({
                "agent_id": agent_id,
                "message": message,
                "status": status,
                "agentStatus": agent_status,
            }))
        }
        Request::Heartbeat { agent_id } => {
            let mut guard = state.lock().await;
            let agent_status = guard.registry.heartbeat(&agent_id).map(|agent| json!({
                "agent_id": agent.agent_id,
                "status": agent.status,
                "health": agent.health.as_str(),
                "last_seen_unix": agent.last_seen_unix,
            }));
            Response::ok(json!({ "agentStatus": agent_status }))
        }
        Request::GetAgentStatus { agent_id } => {
            let guard = state.lock().await;
            Response::ok(json!({ "agentStatus": guard.registry.get_status(&agent_id) }))
        }
        Request::PingAgent { agent_id } => {
            let _ = append_watch_message(&paths, "ping", format!("ping agent {agent_id}"));
            Response::ok(json!({ "pinged": agent_id }))
        }
        Request::Status => {
            let guard = state.lock().await;
            Response::ok(json!({
                "running": true,
                "socket_path": paths.socket_path().display().to_string(),
                "agents": guard.registry.list(),
            }))
        }
        Request::Shutdown => {
            let _ = persist_session(state.clone(), &paths).await;
            let _ = shutdown_tx.send(true);
            Response::ok(json!({ "stopping": true }))
        }
    }
}

async fn process_envelope_request(
    state: Arc<Mutex<DaemonState>>,
    shutdown_tx: watch::Sender<bool>,
    paths: DaemonPaths,
    request: DaemonRequest,
) -> DaemonResponse {
    let default_agent_id = std::env::var("SQUAD_AGENT_ID").unwrap_or_else(|_| "assistant".to_string());

    match request {
        DaemonRequest::SendMessage { to, content } => {
            let content_for_log = content.clone();
            let to_for_log = to.clone();
            let mut guard = state.lock().await;
            if !guard.configured_agents.contains(&to_for_log) {
                return DaemonResponse::Error {
                    message: format!(
                        "unknown agent '{to_for_log}': not in configured agents or registered"
                    ),
                };
            }
            guard
                .mailbox
                .push(to, Message::new(default_agent_id.clone(), content));
            let _ = guard.registry.set_status(&to_for_log, "working");
            let _ = guard.registry.heartbeat(&to_for_log);
            if guard.persistence_enabled {
                let _ = guard.message_store.append_message(
                    default_agent_id.clone(),
                    to_for_log.clone(),
                    content_for_log.clone(),
                );
            }
            let _ = record_audit(
                &mut guard,
                AuditEventKind::MessageSent,
                Some(to_for_log.as_str()),
                format!("{default_agent_id} -> {to_for_log}: {content_for_log}"),
            );
            drop(guard);
            let _ = append_watch_message(
                &paths,
                format!("{default_agent_id} -> {to_for_log}"),
                content_for_log,
            );
            let _ = persist_session(state.clone(), &paths).await;
            DaemonResponse::Ack {
                message: "queued".to_string(),
            }
        }
        DaemonRequest::CheckInbox => {
            let mut guard = state.lock().await;
            let _ = guard.registry.heartbeat(&default_agent_id);
            let messages: Vec<InboxMessage> = guard
                .mailbox
                .pop(&default_agent_id)
                .into_iter()
                .map(|message| InboxMessage {
                    from: message.from,
                    content: message.content,
                })
                .collect();
            if !messages.is_empty() {
                let _ = guard.registry.set_status(&default_agent_id, "working");
                if guard.persistence_enabled {
                    let _ = guard.message_store.mark_delivered_for_agent(&default_agent_id);
                }
                let _ = record_audit(
                    &mut guard,
                    AuditEventKind::MessageDelivered,
                    Some(default_agent_id.as_str()),
                    format!("delivered to {default_agent_id}"),
                );
            }
            drop(guard);
            let _ = persist_session(state.clone(), &paths).await;
            DaemonResponse::Inbox { messages }
        }
        DaemonRequest::MarkDone { summary } => {
            let _ = advance_workflow(state.clone(), paths.clone(), &default_agent_id, &summary).await;
            let mut guard = state.lock().await;
            let _ = guard.registry.mark_done(&default_agent_id);
            drop(guard);
            let _ = persist_session(state.clone(), &paths).await;
            let _ = shutdown_tx.send(false);
            DaemonResponse::Done {
                message: format!("recorded: {summary}"),
            }
        }
        DaemonRequest::Heartbeat { agent_id } => {
            let mut guard = state.lock().await;
            match guard.registry.heartbeat(&agent_id) {
                Some(agent) => DaemonResponse::Ack {
                    message: format!("heartbeat recorded for {}", agent.agent_id),
                },
                None => DaemonResponse::Error {
                    message: format!("unknown agent: {agent_id}"),
                },
            }
        }
        DaemonRequest::GetAgentStatus { agent_id } => {
            let guard = state.lock().await;
            match guard.registry.get_status(&agent_id) {
                Some(agent_status) => DaemonResponse::AgentStatus { agent_status },
                None => DaemonResponse::Error {
                    message: format!("unknown agent: {agent_id}"),
                },
            }
        }
    }
}

async fn advance_workflow(
    state: Arc<Mutex<DaemonState>>,
    paths: DaemonPaths,
    agent_id: &str,
    summary: &str,
) -> Result<()> {
    let (workflow_config, mut workflow_state) = {
        let guard = state.lock().await;
        (guard.workflow_config.clone(), guard.workflow_state.clone())
    };

    let dispatcher = ServerDispatcher {
        paths: paths.clone(),
        state: state.clone(),
    };
    let mut engine = WorkflowEngine::new(workflow_config, dispatcher);
    let _ = engine
        .handle_mark_done_with_goal(&mut workflow_state, agent_id, summary, "")
        .await?;
    workflow_state.save_to_path(&paths.state_path())?;

    let mut guard = state.lock().await;
    guard.workflow_state = workflow_state;
    let _ = record_audit(
        &mut guard,
        AuditEventKind::WorkflowAdvanced,
        Some(agent_id),
        summary.to_string(),
    );
    drop(guard);

    append_watch_message(&paths, format!("{agent_id} -> workflow"), summary)?;
    persist_session(state, &paths).await
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after unix epoch")
        .as_secs()
}


async fn record_offline_agents(state: Arc<Mutex<DaemonState>>) -> Result<()> {
    let mut guard = state.lock().await;
    if !guard.persistence_enabled {
        return Ok(());
    }
    let session_id = guard.session_id.clone();
    let offline_agents: Vec<String> = guard
        .registry
        .list()
        .into_iter()
        .filter(|agent| agent.health.as_str() == "offline")
        .map(|agent| agent.agent_id)
        .collect();
    for agent_id in offline_agents {
        let _ = guard.audit_log.append(
            session_id.clone(),
            AuditEventKind::AgentOffline,
            Some(&agent_id),
            "agent offline during shutdown",
        );
    }
    Ok(())
}

async fn persist_session(state: Arc<Mutex<DaemonState>>, paths: &DaemonPaths) -> Result<()> {
    let guard = state.lock().await;
    if !guard.persistence_enabled {
        return Ok(());
    }

    let session = SessionState::new(
        guard.session_id.clone(),
        guard.registry.list(),
        guard.workflow_state.clone(),
        guard.message_store.pending_message_ids()?,
    );
    SessionStore::new(paths.session_path()).save(&session)
}

fn record_audit(
    guard: &mut DaemonState,
    event: AuditEventKind,
    agent: Option<&str>,
    detail: impl Into<String>,
) -> Result<()> {
    if !guard.persistence_enabled {
        return Ok(());
    }
    guard
        .audit_log
        .append(guard.session_id.clone(), event, agent, detail)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use crate::daemon::{send_request, DaemonPaths};
    use crate::mcp::client::DaemonClient;
    use crate::protocol::Request;

    use super::DaemonServer;

    #[tokio::test]
    async fn serves_register_send_and_inbox_requests() -> Result<()> {
        let workspace = tempdir()?;
        std::fs::write(
            workspace.path().join("squad.yaml"),
            "workflow:\n  start_at: cc\n  steps:\n    - agent: cc\n      prompt: \"Build it\"\n",
        )?;
        let paths = DaemonPaths::new(workspace.path());
        let server = DaemonServer::bind(paths.clone()).await?;

        let handle = tokio::spawn(async move { server.serve_until_shutdown().await });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let register = send_request(
            paths.socket_path(),
            &Request::Register {
                agent_id: "cc".into(),
                role: "implementer".into(),
            },
        )
        .await?;
        let register_value = serde_json::to_value(register)?;
        assert_eq!(register_value["Ok"]["agent_id"], "cc");

        let send = send_request(
            paths.socket_path(),
            &Request::SendMessage {
                from: "assistant".into(),
                to: "cc".into(),
                content: "build daemon".into(),
            },
        )
        .await?;
        let send_value = serde_json::to_value(send)?;
        assert_eq!(send_value["Ok"]["queued"], 1);

        let inbox = send_request(
            paths.socket_path(),
            &Request::CheckInbox {
                agent_id: "cc".into(),
            },
        )
        .await?;
        let inbox_value = serde_json::to_value(inbox)?;
        assert_eq!(inbox_value["Ok"]["message"]["content"], "build daemon");

        let _ = send_request(paths.socket_path(), &Request::Shutdown).await?;
        handle.await??;
        Ok(())
    }

    #[tokio::test]
    async fn serves_mcp_client_requests_without_waiting_for_eof() -> Result<()> {
        let workspace = tempdir()?;
        std::fs::write(
            workspace.path().join("squad.yaml"),
            "workflow:\n  start_at: cc\n  steps:\n    - agent: cc\n      prompt: \"Build it\"\n",
        )?;
        let paths = DaemonPaths::new(workspace.path());
        let server = DaemonServer::bind(paths.clone()).await?;

        let handle = tokio::spawn(async move { server.serve_until_shutdown().await });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let _ = send_request(
            paths.socket_path(),
            &Request::Register {
                agent_id: "assistant".into(),
                role: "agent".into(),
            },
        )
        .await?;

        let client = DaemonClient::for_workspace(workspace.path().to_path_buf());
        let inbox =
            tokio::time::timeout(std::time::Duration::from_millis(500), client.check_inbox())
                .await
                .expect("daemon response timeout")?;
        assert!(inbox.is_empty());

        let _ = send_request(paths.socket_path(), &Request::Shutdown).await?;
        handle.await??;
        Ok(())
    }
}
