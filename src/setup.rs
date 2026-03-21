use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum InstallMethod {
    /// Write a slash command file to the given path
    SlashCommand { command_path: &'static str },
    /// Append instructions to a global config .md file
    AppendConfig { config_path: &'static str },
}

pub struct Platform {
    pub name: &'static str,
    pub binary: &'static str,
    pub method: InstallMethod,
}

pub const PLATFORMS: &[Platform] = &[
    Platform {
        name: "claude",
        binary: "claude",
        method: InstallMethod::SlashCommand {
            command_path: ".claude/commands/squad.md",
        },
    },
    Platform {
        name: "gemini",
        binary: "gemini",
        method: InstallMethod::AppendConfig {
            config_path: ".gemini/GEMINI.md",
        },
    },
    Platform {
        name: "codex",
        binary: "codex",
        method: InstallMethod::SlashCommand {
            command_path: ".codex/prompts/squad.md",
        },
    },
    Platform {
        name: "opencode",
        binary: "opencode",
        method: InstallMethod::SlashCommand {
            command_path: ".config/opencode/commands/squad.md",
        },
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

const SQUAD_CONFIG_SECTION: &str = "\n## Squad Collaboration

This project uses squad for multi-agent collaboration.
When the user asks you to join squad as a role, run: `squad join <role> --role <role>`
Then follow the role instructions from the output. Key commands:
- `squad send <your-id> <to> \"<message>\"` — send a message
- `squad receive <your-id> --wait` — wait for incoming messages
- `squad agents` — see who is online
- `squad help` — full command reference
";

const SQUAD_CONFIG_MARKER: &str = "## Squad Collaboration";

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

/// Get the full path for a platform's install target.
pub fn install_path(platform: &Platform) -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let rel = match &platform.method {
        InstallMethod::SlashCommand { command_path } => command_path,
        InstallMethod::AppendConfig { config_path } => config_path,
    };
    Ok(PathBuf::from(home).join(rel))
}

/// Install the squad command/config for a given path and method.
pub fn install_command(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(path, SQUAD_COMMAND_CONTENT)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Append squad section to a config .md file (idempotent).
pub fn append_config(path: &Path) -> Result<()> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        if content.contains(SQUAD_CONFIG_MARKER) {
            return Ok(()); // already installed
        }
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    write!(file, "{SQUAD_CONFIG_SECTION}")?;
    Ok(())
}

/// Install for a specific platform.
pub fn install_for_platform(platform: &Platform) -> Result<PathBuf> {
    let path = install_path(platform)?;
    match &platform.method {
        InstallMethod::SlashCommand { .. } => install_command(&path)?,
        InstallMethod::AppendConfig { .. } => append_config(&path)?,
    }
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
