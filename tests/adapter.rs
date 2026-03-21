use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::{json, Value};
use squad::adapter::{AgentAdapter, HookAdapter, WatchAdapter};
use squad::config::{AgentAdapterKind, SquadConfig};
use tempfile::tempdir;

fn run_squad(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_squad"))
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("run squad")
}

fn run_hook(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new(std::env::var("CARGO_BIN_EXE_squad-hook").expect("squad-hook binary path"))
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("run squad-hook")
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
fn parses_agent_adapter_configuration() -> Result<()> {
    let config = SquadConfig::from_yaml(
        r#"
project: adapters
agents:
  cc:
    adapter: mcp
  codex:
    adapter: hook
    hook_script: ~/.config/squad/hooks/codex.sh
  qwen:
    adapter: watch
    watch_file: .squad/outputs/qwen.txt
workflow:
  steps:
    - agent: cc
      prompt: "Implement {goal}"
"#,
    )?;

    assert_eq!(config.agents["cc"].adapter, AgentAdapterKind::Mcp);
    assert_eq!(config.agents["codex"].adapter, AgentAdapterKind::Hook);
    assert_eq!(config.agents["qwen"].adapter, AgentAdapterKind::Watch);
    assert_eq!(
        config.agents["qwen"].watch_file.as_deref(),
        Some(".squad/outputs/qwen.txt")
    );
    Ok(())
}

#[test]
fn hook_adapter_runs_script_with_message_payload() -> Result<()> {
    let workspace = tempdir()?;
    let script = workspace.path().join("hook.sh");
    let output = workspace.path().join("hook.out");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s' \"$SQUAD_MESSAGE\" > \"{}\"\n",
            output.display()
        ),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms)?;
    }

    let adapter = HookAdapter::new(script);
    adapter.send("ship it")?;

    assert_eq!(fs::read_to_string(output)?, "ship it");
    Ok(())
}

#[test]
fn watch_adapter_reads_new_output_when_file_changes() -> Result<()> {
    let workspace = tempdir()?;
    let output = workspace.path().join("agent.txt");
    fs::write(&output, "")?;

    let adapter = WatchAdapter::new(&output)?;
    fs::write(&output, "review complete")?;

    let message = adapter
        .poll_output()?
        .expect("watch adapter should read new output");
    assert_eq!(message, "review complete");
    Ok(())
}

#[test]
fn squad_hook_send_enqueues_message_for_agent() {
    let workspace = tempdir().expect("tempdir");
    fs::write(
        workspace.path().join("squad.yaml"),
        r#"
project: adapters
workflow:
  start_at: cc
  steps:
    - agent: cc
      prompt: "Implement"
"#,
    )
    .expect("write config");

    let start = run_squad(workspace.path(), &["start"]);
    assert!(
        start.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&start.stderr)
    );

    let socket_path = workspace.path().join(".squad/squad.sock");
    wait_for(|| socket_path.exists());

    let register = send_request(
        &socket_path,
        json!({"Register": {"agent_id": "cc", "role": "implementer"}}),
    );
    assert_eq!(register["Ok"]["agent_id"], "cc");

    let send = run_hook(workspace.path(), &["send", "cc", "hook payload"]);
    assert!(
        send.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&send.stderr)
    );

    let inbox = send_request(&socket_path, json!({"CheckInbox": {"agent_id": "cc"}}));
    assert_eq!(inbox["Ok"]["message"]["content"], "hook payload");

    let stop = run_squad(workspace.path(), &["stop"]);
    assert!(
        stop.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&stop.stderr)
    );
}
