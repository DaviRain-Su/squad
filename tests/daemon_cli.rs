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

#[test]
fn init_creates_template_config() {
    let workspace = tempdir().expect("tempdir");
    let output = run_cli(workspace.path(), &["init"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let config = fs::read_to_string(workspace.path().join("squad.yaml")).expect("read config");
    assert!(config.contains("project: my-project"));
    assert!(config.contains("max_iterations: 10"));
}

#[test]
fn daemon_lifecycle_handles_socket_requests() {
    let workspace = tempdir().expect("tempdir");
    let init_output = run_cli(workspace.path(), &["init"]);
    assert!(
        init_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&init_output.stderr)
    );

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
                "content": "Implement the socket daemon"
            }
        }),
    );
    assert_eq!(send["Ok"]["queued"], 1);

    let inbox = send_request(
        &socket_path,
        json!({
            "CheckInbox": {
                "agent_id": "cc"
            }
        }),
    );
    assert_eq!(inbox["Ok"]["message"]["from"], "assistant");
    assert_eq!(
        inbox["Ok"]["message"]["content"],
        "Implement the socket daemon"
    );

    let status_output = run_cli(workspace.path(), &["status"]);
    assert!(
        status_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&status_output.stderr)
    );
    let status_text = String::from_utf8_lossy(&status_output.stdout);
    assert!(status_text.contains("running: true"));
    assert!(status_text.contains("cc (implementer)"));

    let stop_output = run_cli(workspace.path(), &["stop"]);
    assert!(
        stop_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&stop_output.stderr)
    );
    wait_for(|| !socket_path.exists());
}
