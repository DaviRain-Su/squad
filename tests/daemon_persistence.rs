use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tempfile::tempdir;

fn run_cli(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_squad"))
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("run squad cli")
}

fn wait_for(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("condition not met before timeout");
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

fn write_persistent_config(workspace: &Path) {
    fs::write(
        workspace.join("squad.yaml"),
        r#"
project: persistence-demo
persistence:
  enabled: true
workflow:
  start_at: cc
  steps:
    - agent: cc
      action: implement
      prompt: "Implement {goal}"
"#,
    )
    .expect("write config");
}

#[test]
fn pending_messages_survive_daemon_restart_when_persistence_enabled() {
    let workspace = tempdir().expect("tempdir");
    write_persistent_config(workspace.path());

    let start_output = run_cli(workspace.path(), &["start"]);
    assert!(
        start_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&start_output.stderr)
    );

    let socket_path = workspace.path().join(".squad/squad.sock");
    wait_for(|| socket_path.exists());

    let register = send_request(
        &socket_path,
        json!({
            "Register": {
                "agent_id": "cc",
                "role": "implementer"
            }
        }),
    );
    assert_eq!(register["Ok"]["agent_id"], "cc");

    let send = send_request(
        &socket_path,
        json!({
            "SendMessage": {
                "from": "assistant",
                "to": "cc",
                "content": "Persist this task"
            }
        }),
    );
    assert_eq!(send["Ok"]["queued"], 1);

    let stop_output = run_cli(workspace.path(), &["stop"]);
    assert!(
        stop_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&stop_output.stderr)
    );
    wait_for(|| !socket_path.exists());

    let restart_output = run_cli(workspace.path(), &["start"]);
    assert!(
        restart_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&restart_output.stderr)
    );
    wait_for(|| socket_path.exists());

    let inbox = send_request(
        &socket_path,
        json!({
            "CheckInbox": {
                "agent_id": "cc"
            }
        }),
    );
    assert_eq!(inbox["Ok"]["message"]["content"], "Persist this task");

    let final_stop = run_cli(workspace.path(), &["stop"]);
    assert!(final_stop.status.success());
}

#[test]
fn log_history_and_clean_commands_operate_on_persistent_artifacts() {
    let workspace = tempdir().expect("tempdir");
    write_persistent_config(workspace.path());

    let start_output = run_cli(workspace.path(), &["start"]);
    assert!(start_output.status.success());

    let socket_path = workspace.path().join(".squad/squad.sock");
    wait_for(|| socket_path.exists());

    let _ = send_request(
        &socket_path,
        json!({
            "Register": {
                "agent_id": "cc",
                "role": "implementer"
            }
        }),
    );
    let _ = send_request(
        &socket_path,
        json!({
            "SendMessage": {
                "from": "assistant",
                "to": "cc",
                "content": "Audit this task"
            }
        }),
    );
    let _ = send_request(
        &socket_path,
        json!({
            "CheckInbox": {
                "agent_id": "cc"
            }
        }),
    );

    let stop_output = run_cli(workspace.path(), &["stop"]);
    assert!(stop_output.status.success());

    let log_output = run_cli(
        workspace.path(),
        &["log", "--tail", "2", "--filter", "agent=cc"],
    );
    assert!(
        log_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&log_output.stderr)
    );
    let log_text = String::from_utf8_lossy(&log_output.stdout);
    assert!(log_text.contains("cc"));
    assert!(log_text.contains("MessageDelivered") || log_text.contains("MessageSent"));

    let history_output = run_cli(workspace.path(), &["history"]);
    assert!(history_output.status.success());
    let history_text = String::from_utf8_lossy(&history_output.stdout);
    assert!(history_text.contains("session"));
    assert!(history_text.contains("messages"));

    let clean_output = run_cli(workspace.path(), &["clean"]);
    assert!(clean_output.status.success());
    assert!(workspace.path().join("squad.yaml").exists());
    assert!(!workspace.path().join(".squad/messages.db").exists());
    assert!(!workspace.path().join(".squad/audit.log").exists());
    assert!(!workspace.path().join(".squad/session.json").exists());
}
