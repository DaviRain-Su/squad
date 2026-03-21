/// End-to-end integration tests that exercise the full daemon lifecycle:
/// init → start → register → send_message → check_inbox → mark_done
///   → workflow advance → audit log → stop → socket cleanup
///
/// These tests spawn a real daemon process (no mocks) and communicate
/// over the Unix socket exactly as a real agent would.
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tempfile::tempdir;

// ─── helpers ────────────────────────────────────────────────────────────────

fn run_cli(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_squad"))
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("run squad cli")
}

fn wait_for(label: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("condition '{label}' not met before timeout");
}

fn send_request(socket_path: &Path, payload: Value) -> Value {
    let mut stream = UnixStream::connect(socket_path).expect("connect socket");
    let body = serde_json::to_vec(&payload).expect("serialize payload");
    stream.write_all(&body).expect("write request");
    stream
        .shutdown(std::net::Shutdown::Write)
        .expect("shutdown write");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    serde_json::from_str(&response).expect("parse response")
}

fn write_two_step_pipeline(workspace: &Path) {
    fs::write(
        workspace.join("squad.yaml"),
        r#"project: e2e-test
workflow:
  mode: pipeline
  start_at: implement
  steps:
    - id: implement
      agent: cc
      action: implement
      message: "Goal: {goal}"
    - id: review
      agent: reviewer
      action: review
      message: "Review: {previous_output}"
"#,
    )
    .expect("write squad.yaml");
}

// ─── tests ───────────────────────────────────────────────────────────────────

/// Full pipeline: init → start → register two agents → send_message to cc →
/// cc checks inbox → cc marks_done → workflow advances → reviewer gets message
/// in inbox → verify routing → stop → socket cleaned up.
#[test]
fn full_pipeline_message_routing_and_workflow_advance() {
    let workspace = tempdir().expect("tempdir");
    write_two_step_pipeline(workspace.path());

    // start daemon
    let start = run_cli(workspace.path(), &["start"]);
    assert!(
        start.status.success(),
        "start failed: {}",
        String::from_utf8_lossy(&start.stderr)
    );

    let socket_path = workspace.path().join(".squad/squad.sock");
    wait_for("socket exists", || socket_path.exists());

    // register both agents
    let reg_cc = send_request(
        &socket_path,
        json!({ "Register": { "agent_id": "cc", "role": "implement" } }),
    );
    assert_eq!(reg_cc["Ok"]["agent_id"], "cc", "cc registration failed");

    let reg_reviewer = send_request(
        &socket_path,
        json!({ "Register": { "agent_id": "reviewer", "role": "review" } }),
    );
    assert_eq!(
        reg_reviewer["Ok"]["agent_id"], "reviewer",
        "reviewer registration failed"
    );

    // send initial message to cc (simulates workflow dispatch)
    let send = send_request(
        &socket_path,
        json!({
            "SendMessage": {
                "from": "workflow",
                "to": "cc",
                "content": "Implement the new feature"
            }
        }),
    );
    assert_eq!(send["Ok"]["queued"], 1, "message queuing failed");

    // cc checks inbox — message should be present
    let inbox_cc = send_request(
        &socket_path,
        json!({ "CheckInbox": { "agent_id": "cc" } }),
    );
    assert_eq!(
        inbox_cc["Ok"]["message"]["from"], "workflow",
        "cc inbox empty or wrong sender"
    );
    assert_eq!(
        inbox_cc["Ok"]["message"]["content"], "Implement the new feature",
        "cc inbox has wrong content"
    );

    // cc marks done → workflow should advance to 'review' step and put a
    // message in reviewer's inbox
    let mark_done = send_request(
        &socket_path,
        json!({
            "MarkDone": {
                "agent_id": "cc",
                "message": "Feature implemented successfully"
            }
        }),
    );
    assert!(
        mark_done.get("Ok").is_some(),
        "mark_done returned error: {mark_done}"
    );

    // reviewer's inbox should now contain the workflow-dispatched message
    let inbox_reviewer = send_request(
        &socket_path,
        json!({ "CheckInbox": { "agent_id": "reviewer" } }),
    );
    let reviewer_msg = &inbox_reviewer["Ok"]["message"];
    assert!(
        !reviewer_msg.is_null(),
        "reviewer inbox is empty after workflow advance"
    );
    // The workflow engine renders the step message template; it should contain
    // the previous_output from cc's mark_done summary.
    let reviewer_content = reviewer_msg["content"].as_str().unwrap_or("");
    assert!(
        reviewer_content.contains("Feature implemented successfully"),
        "reviewer message does not reference cc summary: {reviewer_content}"
    );

    // stop daemon
    let stop = run_cli(workspace.path(), &["stop"]);
    assert!(
        stop.status.success(),
        "stop failed: {}",
        String::from_utf8_lossy(&stop.stderr)
    );

    // socket must be removed after stop
    wait_for("socket removed", || !socket_path.exists());
    assert!(
        !socket_path.exists(),
        "socket still exists after daemon stopped"
    );
}

fn write_two_step_pipeline_with_persistence(workspace: &Path) {
    fs::write(
        workspace.join("squad.yaml"),
        r#"project: e2e-test-audit
persistence:
  enabled: true
workflow:
  mode: pipeline
  start_at: implement
  steps:
    - id: implement
      agent: cc
      action: implement
      message: "Goal: {goal}"
    - id: review
      agent: reviewer
      action: review
      message: "Review: {previous_output}"
"#,
    )
    .expect("write squad.yaml");
}

/// Verify the audit log records AgentRegistered, MessageSent, MessageDelivered,
/// and WorkflowAdvanced events across a full session.
/// Note: audit events are only persisted when `persistence.enabled: true`.
#[test]
fn audit_log_records_full_session_events() {
    let workspace = tempdir().expect("tempdir");
    write_two_step_pipeline_with_persistence(workspace.path());

    let start = run_cli(workspace.path(), &["start"]);
    assert!(start.status.success());

    let socket_path = workspace.path().join(".squad/squad.sock");
    wait_for("socket exists", || socket_path.exists());

    send_request(
        &socket_path,
        json!({ "Register": { "agent_id": "cc", "role": "implement" } }),
    );
    send_request(
        &socket_path,
        json!({
            "SendMessage": {
                "from": "workflow",
                "to": "cc",
                "content": "Build the thing"
            }
        }),
    );
    send_request(
        &socket_path,
        json!({ "CheckInbox": { "agent_id": "cc" } }),
    );
    send_request(
        &socket_path,
        json!({
            "MarkDone": {
                "agent_id": "cc",
                "message": "done"
            }
        }),
    );

    let stop = run_cli(workspace.path(), &["stop"]);
    assert!(stop.status.success());
    wait_for("socket removed", || !socket_path.exists());

    // Use the CLI to read the audit log — it must contain key event types
    let log = run_cli(workspace.path(), &["log"]);
    assert!(log.status.success());
    let log_text = String::from_utf8_lossy(&log.stdout);
    assert!(
        log_text.contains("AgentRegistered"),
        "audit log missing AgentRegistered: {log_text}"
    );
    assert!(
        log_text.contains("MessageSent"),
        "audit log missing MessageSent: {log_text}"
    );
    assert!(
        log_text.contains("MessageDelivered"),
        "audit log missing MessageDelivered: {log_text}"
    );
    assert!(
        log_text.contains("WorkflowAdvanced"),
        "audit log missing WorkflowAdvanced: {log_text}"
    );
}

/// Verify that the socket file is removed and the daemon pid file is cleaned
/// up when the daemon is stopped gracefully.
#[test]
fn daemon_stop_cleans_up_socket_and_pid() {
    let workspace = tempdir().expect("tempdir");
    write_two_step_pipeline(workspace.path());

    let start = run_cli(workspace.path(), &["start"]);
    assert!(start.status.success());

    let socket_path = workspace.path().join(".squad/squad.sock");
    let pid_path = workspace.path().join(".squad/daemon.pid");
    wait_for("socket exists", || socket_path.exists());

    assert!(
        socket_path.exists(),
        "socket should exist while daemon is running"
    );
    assert!(pid_path.exists(), "pid file should exist while daemon is running");

    let stop = run_cli(workspace.path(), &["stop"]);
    assert!(stop.status.success());
    wait_for("socket removed", || !socket_path.exists());

    assert!(!socket_path.exists(), "socket not removed after stop");
    assert!(!pid_path.exists(), "pid file not removed after stop");
}

/// Verify that `squad status` correctly reports running state before and after
/// the daemon lifecycle.
#[test]
fn status_reflects_daemon_running_state() {
    let workspace = tempdir().expect("tempdir");
    write_two_step_pipeline(workspace.path());

    // Before start: not running
    let before = run_cli(workspace.path(), &["status"]);
    assert!(before.status.success());
    assert!(
        String::from_utf8_lossy(&before.stdout).contains("daemon: stopped"),
        "expected 'daemon: stopped' before start"
    );

    let start = run_cli(workspace.path(), &["start"]);
    assert!(start.status.success());

    let socket_path = workspace.path().join(".squad/squad.sock");
    wait_for("socket exists", || socket_path.exists());

    // While running: status says true
    let during = run_cli(workspace.path(), &["status"]);
    assert!(during.status.success());
    assert!(
        String::from_utf8_lossy(&during.stdout).contains("daemon: running"),
        "expected 'daemon: running' while daemon is up"
    );

    let stop = run_cli(workspace.path(), &["stop"]);
    assert!(stop.status.success());
    wait_for("socket removed", || !socket_path.exists());

    // After stop: not running
    let after = run_cli(workspace.path(), &["status"]);
    assert!(after.status.success());
    assert!(
        String::from_utf8_lossy(&after.stdout).contains("daemon: stopped"),
        "expected 'daemon: stopped' after stop"
    );
}
