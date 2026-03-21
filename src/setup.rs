use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct Platform {
    pub name: &'static str,
    pub binary: &'static str,
    pub command_path: &'static str, // relative to home dir
}

pub const PLATFORMS: &[Platform] = &[
    Platform {
        name: "claude",
        binary: "claude",
        command_path: ".claude/commands/squad.md",
    },
    // Gemini CLI: custom slash commands not yet supported.
    // ~/.gemini/antigravity/global_workflows/ is not a native command directory.
    Platform {
        name: "codex",
        binary: "codex",
        command_path: ".codex/prompts/squad.md",
    },
    Platform {
        name: "opencode",
        binary: "opencode",
        command_path: ".config/opencode/commands/squad.md",
    },
];

pub const SQUAD_COMMAND_CONTENT: &str = r#"---
description: Join squad multi-agent collaboration. Usage: /squad <role> [custom-id]
---

You are joining a squad multi-agent collaboration team.

## Instructions

1. Parse the arguments: $ARGUMENTS

   **If arguments are empty or missing:**
   Run `squad roles` to list available roles, then ask the user which role they want to join as. Do NOT proceed until the user picks a role.

   **If arguments are provided:**
   - First word is the role (e.g. manager, worker, inspector)
   - Optional second word is a custom agent ID
   - If no custom ID provided, use the role name as your ID
   - Examples: "manager" → id=manager, role=manager | "worker worker-2" → id=worker-2, role=worker

2. Run these commands in order:
   a. `squad join <id> --role <role>` — register yourself and read the output
   b. If role instructions are printed (=== Role Instructions ===), follow them
   c. If no predefined template exists, **interpret the role using your own knowledge**. Adapt your behavior to what that role would do in a software team. Any role name works — you are not limited to predefined roles.
   d. `squad agents` — check who else is on the team

3. Communicate using squad commands:
   - `squad send <your-id> <to> "<message>"` — send a message (use @all to broadcast)
   - `squad receive <your-id> --wait` — block until a message arrives
   - `squad agents` — see who is online
   - `squad pending` — check unread messages
   - `squad history` — view message history

4. After completing any task, always run `squad receive <your-id> --wait` to wait for the next message.
"#;

/// Check if a binary exists in PATH.
pub fn is_installed(binary: &str) -> bool {
    std::process::Command::new("which")
        .arg(binary)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Detect which platforms are installed.
pub fn detect_platforms() -> Vec<&'static Platform> {
    PLATFORMS.iter().filter(|p| is_installed(p.binary)).collect()
}

/// Get the full path for a platform's command file.
pub fn command_path(platform: &Platform) -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    Ok(PathBuf::from(home).join(platform.command_path))
}

/// Install the squad command file to the given path.
pub fn install_command(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(path, SQUAD_COMMAND_CONTENT)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Run setup: detect platforms and install command files.
/// Returns list of (platform_name, path, installed_or_error).
pub fn run_setup() -> Vec<(String, PathBuf, Result<()>)> {
    let mut results = Vec::new();
    for platform in PLATFORMS {
        if !is_installed(platform.binary) {
            continue;
        }
        match command_path(platform) {
            Ok(path) => {
                let result = install_command(&path);
                results.push((platform.name.to_string(), path, result));
            }
            Err(e) => {
                results.push((platform.name.to_string(), PathBuf::new(), Err(e)));
            }
        }
    }
    results
}
