use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct Platform {
    pub name: &'static str,
    pub binary: &'static str,
    pub command_path: &'static str, // relative to home dir
    pub content: &'static str,
}

pub const PLATFORMS: &[Platform] = &[
    Platform {
        name: "claude",
        binary: "claude",
        command_path: ".claude/commands/squad.md",
        content: SQUAD_MD_CONTENT,
    },
    Platform {
        name: "gemini",
        binary: "gemini",
        command_path: ".gemini/commands/squad.toml",
        content: SQUAD_TOML_CONTENT,
    },
    Platform {
        name: "codex",
        binary: "codex",
        command_path: ".codex/prompts/squad.md",
        content: SQUAD_MD_CONTENT,
    },
    Platform {
        name: "opencode",
        binary: "opencode",
        command_path: ".config/opencode/commands/squad.md",
        content: SQUAD_MD_CONTENT,
    },
];

/// Markdown format for Claude Code, Codex, OpenCode (uses $ARGUMENTS)
pub const SQUAD_MD_CONTENT: &str = r#"---
description: Join squad multi-agent collaboration. Usage: /squad <role> [custom-id]
---

The user's input: $ARGUMENTS

You are joining a squad multi-agent collaboration team.

## Phase 1: Setup (do this once)

1. Parse the arguments above.

   **If arguments are empty or missing:**
   Run `squad roles` to list available roles, then ask the user which role they want to join as. Do NOT proceed until the user picks a role.

   **If arguments are provided:**
   - First word is the role (e.g. manager, worker, inspector)
   - Optional second word is a custom agent ID
   - If no custom ID provided, use the role name as your ID
   - Examples: "manager" → id=manager, role=manager | "worker worker-2" → id=worker-2, role=worker

2. Run `squad init` (safe to run — won't overwrite existing workspace).

3. **Check for ID conflicts before joining:**
   Run `squad agents` and look at the output.
   - If your chosen ID already appears in the list, pick a different ID by appending `-2`, `-3`, etc.
   - If your chosen ID does not appear, proceed with it.

4. Run `squad join <id> --role <role>` to register yourself.
   - Read the output line that says "Joined as ..." — that confirms your agent ID.
   - If role instructions are printed (=== Role Instructions ===), follow them.
   - If no predefined template exists, interpret the role using your own knowledge.

5. Run `squad agents` to see who else is on the team.

6. **If any squad command returns "Session replaced":** another terminal took your ID. Re-join with a different ID (e.g. `squad join worker-2 --role worker`).

## Phase 2: Work Loop (repeat continuously)

Setup is done. Now enter your work loop. This is your main job.

1. Run `squad receive <your-id> --wait` to wait for messages. This command blocks until a message arrives.
   **IMPORTANT:** Do NOT cancel, background, or interrupt this command. Let it run until it returns on its own.
2. If you received a message:
   a. Execute the task or respond as appropriate for your role.
   b. Report results: `squad send <your-id> <recipient> "<result>"`
3. If the command timed out with no messages, **immediately run it again.** Keep retrying until a message arrives. Never stop waiting unless the user tells you to.

Other useful commands:
- `squad send <your-id> <to> "<message>"` — send a message (use @all to broadcast)
- `squad agents` — see who is online
- `squad pending` — check all unread messages
- `squad history` — view message history
"#;

/// TOML format for Gemini CLI (uses {{args}})
pub const SQUAD_TOML_CONTENT: &str = r#"description = "Join squad multi-agent collaboration. Usage: /squad <role> [custom-id]"

prompt = """
The user's input: {{args}}

You are joining a squad multi-agent collaboration team.

## Phase 1: Setup (do this once)

1. Parse the arguments above.

   **If arguments are empty or missing:**
   Run `squad roles` to list available roles, then ask the user which role they want to join as. Do NOT proceed until the user picks a role.

   **If arguments are provided:**
   - First word is the role (e.g. manager, worker, inspector)
   - Optional second word is a custom agent ID
   - If no custom ID provided, use the role name as your ID
   - Examples: "manager" → id=manager, role=manager | "worker worker-2" → id=worker-2, role=worker

2. Run `squad init` (safe to run — won't overwrite existing workspace).

3. **Check for ID conflicts before joining:**
   Run `squad agents` and look at the output.
   - If your chosen ID already appears in the list, pick a different ID by appending `-2`, `-3`, etc.
   - If your chosen ID does not appear, proceed with it.

4. Run `squad join <id> --role <role>` to register yourself.
   - Read the output line that says "Joined as ..." — that confirms your agent ID.
   - If role instructions are printed (=== Role Instructions ===), follow them.
   - If no predefined template exists, interpret the role using your own knowledge.

5. Run `squad agents` to see who else is on the team.

6. **If any squad command returns "Session replaced":** another terminal took your ID. Re-join with a different ID (e.g. `squad join worker-2 --role worker`).

## Phase 2: Work Loop (repeat continuously)

Setup is done. Now enter your work loop. This is your main job.

1. Run `squad receive <your-id> --wait` to wait for messages. This command blocks until a message arrives.
   **IMPORTANT:** Do NOT cancel, background, or interrupt this command. Let it run until it returns on its own.
2. If you received a message:
   a. Execute the task or respond as appropriate for your role.
   b. Report results: `squad send <your-id> <recipient> "<result>"`
3. If the command timed out with no messages, **immediately run it again.** Keep retrying until a message arrives. Never stop waiting unless the user tells you to.

Other useful commands:
- `squad send <your-id> <to> "<message>"` — send a message (use @all to broadcast)
- `squad agents` — see who is online
- `squad pending` — check all unread messages
- `squad history` — view message history
"""
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

/// Install the squad command file for a platform.
pub fn install_command(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(path, content)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Install for a specific platform.
pub fn install_for_platform(platform: &Platform) -> Result<PathBuf> {
    let path = command_path(platform)?;
    install_command(&path, platform.content)?;
    Ok(path)
}

/// Run setup: detect platforms and install.
pub fn run_setup() -> Vec<(String, PathBuf, Result<()>)> {
    let mut results = Vec::new();
    for platform in PLATFORMS {
        if !is_installed(platform.binary) {
            continue;
        }
        match install_for_platform(platform) {
            Ok(path) => results.push((platform.name.to_string(), path, Ok(()))),
            Err(e) => results.push((platform.name.to_string(), PathBuf::new(), Err(e))),
        }
    }
    results
}
