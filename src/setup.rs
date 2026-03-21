use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

// Supported agent names in setup command
pub const SUPPORTED_AGENTS: &[&str] = &["cc", "codex", "gemini", "qwen"];

// Agents that use MCP; all others use the hook adapter.
pub const MCP_AGENTS: &[&str] = &["cc"];

const CLAUDE_MD_SECTION: &str = r#"
## Squad Collaboration Protocol

This project uses [squad](https://github.com/mco-org/squad) for multi-agent coordination.
The squad MCP server provides tools for agent communication:

- **`check_inbox`** – Poll your inbox for pending messages. Call this at the start of each
  turn to receive instructions from orchestrators or peer agents.
- **`send_heartbeat`** – Notify the daemon that you are alive. Call periodically (e.g.,
  every 15–30 s) during long-running tasks so the daemon does not mark you offline.
- **`send_message`** – Send a message to another agent by `agent_id`. Use this to report
  results, request reviews, or escalate issues.
- **`mark_done`** – Acknowledge a received message by its `message_id`. Always mark
  messages as done after you have acted on them to keep the inbox clean.

### Typical turn loop
1. `check_inbox` → process any pending messages
2. Do your work; call `send_heartbeat` if the work takes > 30 s
3. `send_message` to report results or ask for follow-up
4. `mark_done` for each processed message
"#;

/// Set up .mcp.json for a given agent.
/// Returns true if the entry was newly written, false if it already existed.
pub fn setup_mcp_json(workspace: &Path, agent: &str) -> Result<bool> {
    let mcp_path = workspace.join(".mcp.json");

    let mut root: Value = if mcp_path.exists() {
        let raw = std::fs::read_to_string(&mcp_path)
            .with_context(|| format!("failed to read {}", mcp_path.display()))?;
        serde_json::from_str(&raw).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Ensure mcpServers object exists
    if root.get("mcpServers").is_none() {
        root["mcpServers"] = json!({});
    }

    let servers = root["mcpServers"].as_object_mut().unwrap();
    if servers.contains_key("squad") {
        return Ok(false);
    }

    servers.insert(
        "squad".to_string(),
        json!({
            "command": "squad-mcp",
            "args": [],
            "env": {
                "SQUAD_AGENT_ID": agent
            }
        }),
    );

    let output = serde_json::to_string_pretty(&root)?;
    std::fs::write(&mcp_path, output)
        .with_context(|| format!("failed to write {}", mcp_path.display()))?;
    Ok(true)
}

/// Create .squad/hooks/<agent>.sh for non-MCP agents.
/// Returns true if newly written, false if already exists.
pub fn setup_hook_agent(workspace: &Path, agent: &str) -> Result<bool> {
    let hooks_dir = workspace.join(".squad/hooks");
    std::fs::create_dir_all(&hooks_dir)
        .with_context(|| format!("failed to create {}", hooks_dir.display()))?;
    let script_path = hooks_dir.join(format!("{agent}.sh"));
    if script_path.exists() {
        return Ok(false);
    }
    let script = format!(
        "#!/bin/sh\n\
         # squad hook for {agent}\n\
         # Called by the squad daemon when a message arrives.\n\
         # $SQUAD_MESSAGE contains the message content.\n\
         # Use squad-hook send <to> <message> to reply back to the daemon.\n\
         echo \"$SQUAD_MESSAGE\" | {agent}\n"
    );
    std::fs::write(&script_path, &script)
        .with_context(|| format!("failed to write {}", script_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("failed to chmod {}", script_path.display()))?;
    }
    Ok(true)
}

/// Append the Squad Collaboration Protocol section to CLAUDE.md if not already present.
pub fn update_claude_md(workspace: &Path) -> Result<bool> {
    let claude_md = workspace.join("CLAUDE.md");

    let existing = if claude_md.exists() {
        std::fs::read_to_string(&claude_md)
            .with_context(|| format!("failed to read {}", claude_md.display()))?
    } else {
        String::new()
    };

    if existing.contains("Squad Collaboration Protocol") {
        return Ok(false);
    }

    let updated = existing + CLAUDE_MD_SECTION;
    std::fs::write(&claude_md, updated)
        .with_context(|| format!("failed to write {}", claude_md.display()))?;
    Ok(true)
}

/// Return per-agent setup instructions for display.
pub fn agent_instructions(agent: &str) -> &'static str {
    match agent {
        "cc" => "Claude Code: .mcp.json registered with squad-mcp. \
                 Start the daemon with `squad start`, then launch Claude Code in this directory.",
        "codex" => "Codex CLI (hook adapter): hook script written to .squad/hooks/codex.sh. \
                    Edit the script to invoke codex with $SQUAD_MESSAGE, then start the daemon \
                    with `squad start` and run `squad run <goal>` to begin the workflow.",
        "gemini" => "Gemini CLI (hook adapter): hook script written to .squad/hooks/gemini.sh. \
                     Edit the script to invoke gemini with $SQUAD_MESSAGE, then start the daemon \
                     with `squad start` and run `squad run <goal>` to begin the workflow.",
        "qwen" => "Qwen agent (hook adapter): hook script written to .squad/hooks/qwen.sh. \
                   Edit the script to invoke qwen with $SQUAD_MESSAGE, then start the daemon \
                   with `squad start` and run `squad run <goal>` to begin the workflow.",
        _ => "Unknown agent. Run `squad setup --list` for supported agents.",
    }
}

// ─── doctor ──────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

pub struct CheckResult {
    pub label: String,
    pub status: CheckStatus,
    pub message: String,
}

impl CheckResult {
    fn ok(label: impl Into<String>, message: impl Into<String>) -> Self {
        Self { label: label.into(), status: CheckStatus::Ok, message: message.into() }
    }
    fn warn(label: impl Into<String>, message: impl Into<String>) -> Self {
        Self { label: label.into(), status: CheckStatus::Warn, message: message.into() }
    }
    fn fail(label: impl Into<String>, message: impl Into<String>) -> Self {
        Self { label: label.into(), status: CheckStatus::Fail, message: message.into() }
    }
}

pub fn run_doctor(workspace: &Path) -> Result<Vec<CheckResult>> {
    let mut results = Vec::new();

    // 1. Check daemon
    let socket_path = workspace.join(".squad/squad.sock");
    if !socket_path.exists() {
        results.push(CheckResult::fail("daemon", "socket not found — run `squad start`"));
    } else {
        match check_daemon_responds(&socket_path) {
            Ok(true) => results.push(CheckResult::ok("daemon", "running and responding")),
            Ok(false) => results.push(CheckResult::warn(
                "daemon",
                "socket exists but daemon did not respond — may be stale",
            )),
            Err(err) => results.push(CheckResult::warn("daemon", format!("socket exists but error: {err}"))),
        }
    }

    // 2. Check squad-mcp in PATH
    match which_in_path("squad-mcp") {
        Some(path) => results.push(CheckResult::ok("squad-mcp", format!("found at {path}"))),
        None => results.push(CheckResult::fail(
            "squad-mcp",
            "not found in PATH — run `cargo install squad` or ensure the binary is installed",
        )),
    }

    // 3. Check .mcp.json
    let mcp_path = workspace.join(".mcp.json");
    if !mcp_path.exists() {
        results.push(CheckResult::warn(
            ".mcp.json",
            "not found — run `squad setup cc` to create it",
        ));
    } else {
        let raw = std::fs::read_to_string(&mcp_path)
            .with_context(|| format!("failed to read {}", mcp_path.display()))?;
        match serde_json::from_str::<Value>(&raw) {
            Ok(v) => {
                if v["mcpServers"]["squad"]["command"].as_str() == Some("squad-mcp") {
                    results.push(CheckResult::ok(".mcp.json", "squad entry present and valid"));
                } else {
                    results.push(CheckResult::warn(
                        ".mcp.json",
                        "exists but squad entry missing or invalid — run `squad setup cc`",
                    ));
                }
            }
            Err(err) => results.push(CheckResult::fail(
                ".mcp.json",
                format!("invalid JSON: {err}"),
            )),
        }
    }

    Ok(results)
}

fn check_daemon_responds(socket_path: &Path) -> Result<bool> {
    use crate::protocol::Request;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = match UnixStream::connect(socket_path) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };
    let payload = serde_json::to_vec(&Request::Status)?;
    stream.write_all(&payload)?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(!response.is_empty())
}

fn which_in_path(binary: &str) -> Option<String> {
    // Try std::process::Command with `which` first, fall back to PATH search
    if let Ok(output) = std::process::Command::new("which").arg(binary).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }
    // Manual PATH search
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            let candidate = Path::new(dir).join(binary);
            if candidate.exists() {
                return Some(candidate.display().to_string());
            }
        }
    }
    None
}

pub fn render_doctor_results(results: &[CheckResult]) -> String {
    let mut lines = Vec::new();
    for r in results {
        let (icon, color_start, color_end) = match r.status {
            CheckStatus::Ok => ("✓", "\x1b[32m", "\x1b[0m"),   // green
            CheckStatus::Warn => ("⚠", "\x1b[33m", "\x1b[0m"), // yellow
            CheckStatus::Fail => ("✗", "\x1b[31m", "\x1b[0m"), // red
        };
        lines.push(format!(
            "{color_start}{icon} {:<12}{color_end} {}",
            r.label, r.message
        ));
    }
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn setup_mcp_json_creates_new_file() {
        let dir = tempdir().unwrap();
        let written = setup_mcp_json(dir.path(), "cc").unwrap();
        assert!(written, "should report newly written");

        let mcp_path = dir.path().join(".mcp.json");
        assert!(mcp_path.exists());
        let raw = std::fs::read_to_string(&mcp_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["mcpServers"]["squad"]["command"], "squad-mcp");
        assert_eq!(v["mcpServers"]["squad"]["env"]["SQUAD_AGENT_ID"], "cc");
    }

    #[test]
    fn setup_mcp_json_uses_agent_name_as_squad_agent_id() {
        for agent in &["codex", "gemini", "qwen"] {
            let dir = tempdir().unwrap();
            setup_mcp_json(dir.path(), agent).unwrap();
            let raw = std::fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
            let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(v["mcpServers"]["squad"]["env"]["SQUAD_AGENT_ID"], *agent);
        }
    }

    #[test]
    fn setup_mcp_json_reports_already_configured() {
        let dir = tempdir().unwrap();
        setup_mcp_json(dir.path(), "cc").unwrap();
        let written = setup_mcp_json(dir.path(), "cc").unwrap();
        assert!(!written, "second call should report already configured");
    }

    #[test]
    fn setup_mcp_json_merges_into_existing_file() {
        let dir = tempdir().unwrap();
        let mcp_path = dir.path().join(".mcp.json");
        std::fs::write(
            &mcp_path,
            r#"{"mcpServers":{"other":{"command":"other-mcp","args":[]}}}"#,
        )
        .unwrap();

        let written = setup_mcp_json(dir.path(), "cc").unwrap();
        assert!(written);

        let raw = std::fs::read_to_string(&mcp_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["mcpServers"]["squad"]["command"], "squad-mcp");
        assert_eq!(v["mcpServers"]["other"]["command"], "other-mcp");
    }

    #[test]
    fn update_claude_md_appends_section() {
        let dir = tempdir().unwrap();
        let added = update_claude_md(dir.path()).unwrap();
        assert!(added);
        let content = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert!(content.contains("Squad Collaboration Protocol"));
        assert!(content.contains("check_inbox"));
    }

    #[test]
    fn update_claude_md_skips_if_already_present() {
        let dir = tempdir().unwrap();
        update_claude_md(dir.path()).unwrap();
        let added = update_claude_md(dir.path()).unwrap();
        assert!(!added, "should not re-append if section already exists");
    }

    #[test]
    fn doctor_reports_missing_socket() {
        let dir = tempdir().unwrap();
        let results = run_doctor(dir.path()).unwrap();
        let daemon_check = results.iter().find(|r| r.label == "daemon").unwrap();
        assert_eq!(daemon_check.status, CheckStatus::Fail);
    }

    #[test]
    fn doctor_reports_missing_mcp_json() {
        let dir = tempdir().unwrap();
        let results = run_doctor(dir.path()).unwrap();
        let mcp_check = results.iter().find(|r| r.label == ".mcp.json").unwrap();
        assert_eq!(mcp_check.status, CheckStatus::Warn);
    }

    #[test]
    fn doctor_reports_valid_mcp_json() {
        let dir = tempdir().unwrap();
        setup_mcp_json(dir.path(), "cc").unwrap();
        let results = run_doctor(dir.path()).unwrap();
        let mcp_check = results.iter().find(|r| r.label == ".mcp.json").unwrap();
        assert_eq!(mcp_check.status, CheckStatus::Ok);
    }

    #[test]
    fn render_doctor_results_contains_ansi() {
        let results = vec![
            CheckResult::ok("daemon", "running"),
            CheckResult::warn("squad-mcp", "not found"),
            CheckResult::fail(".mcp.json", "missing"),
        ];
        let output = render_doctor_results(&results);
        assert!(output.contains("\x1b[32m"));
        assert!(output.contains("\x1b[33m"));
        assert!(output.contains("\x1b[31m"));
        assert!(output.contains("✓"));
        assert!(output.contains("⚠"));
        assert!(output.contains("✗"));
    }
}
