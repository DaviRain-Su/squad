use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::tempdir;

fn run_cli(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_squad"))
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("run squad cli")
}

// ─── setup ──────────────────────────────────────────────────────────────────

#[test]
fn setup_cc_creates_mcp_json() {
    let dir = tempdir().unwrap();
    let out = run_cli(dir.path(), &["setup", "cc"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mcp_path = dir.path().join(".mcp.json");
    assert!(mcp_path.exists(), ".mcp.json should be created");

    let raw = fs::read_to_string(&mcp_path).unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["mcpServers"]["squad"]["command"], "squad-mcp");
    assert_eq!(v["mcpServers"]["squad"]["args"], serde_json::json!([]));
    assert_eq!(v["mcpServers"]["squad"]["env"]["SQUAD_AGENT_ID"], "cc");
}

#[test]
fn setup_cc_reports_already_configured() {
    let dir = tempdir().unwrap();
    run_cli(dir.path(), &["setup", "cc"]);
    let out = run_cli(dir.path(), &["setup", "cc"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("already configured"),
        "expected 'already configured', got: {stdout}"
    );
}

#[test]
fn setup_cc_with_update_claude_md() {
    let dir = tempdir().unwrap();
    let out = run_cli(dir.path(), &["setup", "cc", "--update-claude-md"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let claude_md = dir.path().join("CLAUDE.md");
    assert!(claude_md.exists(), "CLAUDE.md should be created");
    let content = fs::read_to_string(&claude_md).unwrap();
    assert!(
        content.contains("Squad Collaboration Protocol"),
        "CLAUDE.md should contain the Squad Collaboration Protocol section"
    );
    assert!(content.contains("check_inbox"));
    assert!(content.contains("send_heartbeat"));
    assert!(content.contains("send_message"));
    assert!(content.contains("mark_done"));
}

#[test]
fn setup_list_shows_supported_agents() {
    let dir = tempdir().unwrap();
    let out = run_cli(dir.path(), &["setup", "--list"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("cc"), "should list cc");
    assert!(stdout.contains("codex"), "should list codex");
    assert!(stdout.contains("gemini"), "should list gemini");
    assert!(stdout.contains("qwen"), "should list qwen");
}

#[test]
fn setup_unknown_agent_fails() {
    let dir = tempdir().unwrap();
    let out = run_cli(dir.path(), &["setup", "unknown-agent"]);
    assert!(!out.status.success(), "should fail for unknown agent");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown agent"),
        "expected 'unknown agent' in stderr, got: {stderr}"
    );
}

#[test]
fn setup_merges_into_existing_mcp_json() {
    let dir = tempdir().unwrap();
    let mcp_path = dir.path().join(".mcp.json");
    fs::write(
        &mcp_path,
        r#"{"mcpServers":{"other":{"command":"other-mcp","args":[]}}}"#,
    )
    .unwrap();

    let out = run_cli(dir.path(), &["setup", "cc"]);
    assert!(out.status.success());

    let raw = fs::read_to_string(&mcp_path).unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["mcpServers"]["squad"]["command"], "squad-mcp");
    assert_eq!(v["mcpServers"]["other"]["command"], "other-mcp");
}

// ─── doctor ─────────────────────────────────────────────────────────────────

#[test]
fn doctor_runs_without_daemon() {
    let dir = tempdir().unwrap();
    // Should succeed (exit 0) even when daemon is not running
    let out = run_cli(dir.path(), &["doctor"]);
    assert!(
        out.status.success(),
        "doctor should exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Should report daemon failure
    assert!(
        stdout.contains("daemon"),
        "output should mention daemon; got: {stdout}"
    );
}

#[test]
fn doctor_reports_missing_mcp_json() {
    let dir = tempdir().unwrap();
    let out = run_cli(dir.path(), &["doctor"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(".mcp.json"),
        "output should mention .mcp.json; got: {stdout}"
    );
}

#[test]
fn doctor_reports_ok_mcp_json_after_setup() {
    let dir = tempdir().unwrap();
    run_cli(dir.path(), &["setup", "cc"]);

    let out = run_cli(dir.path(), &["doctor"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("squad entry present"),
        "should report squad entry valid; got: {stdout}"
    );
}
