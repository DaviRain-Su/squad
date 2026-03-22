use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());

    match command.as_str() {
        "init" => cmd_init(),
        "join" => {
            let id = args.next().unwrap_or_default();
            if id.is_empty() {
                bail!("Usage: squad join <id> [--role <role>]");
            }
            let mut role = id.clone();
            let extra: Vec<String> = args.collect();
            let mut i = 0;
            while i < extra.len() {
                if extra[i] == "--role" {
                    role = extra.get(i + 1).cloned().unwrap_or_else(|| id.clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            cmd_join(&id, &role)
        }
        "leave" => {
            let id = args.next().unwrap_or_default();
            if id.is_empty() {
                bail!("Usage: squad leave <id>");
            }
            cmd_leave(&id)
        }
        "agents" => cmd_agents(),
        "send" => {
            let from = args.next().unwrap_or_default();
            let to = args.next().unwrap_or_default();
            let message: String = args.collect::<Vec<_>>().join(" ");
            if from.is_empty() || to.is_empty() || message.is_empty() {
                bail!("Usage: squad send <from> <to> <message>");
            }
            cmd_send(&from, &to, &message)
        }
        "receive" => {
            let id = args.next().unwrap_or_default();
            if id.is_empty() {
                bail!("Usage: squad receive <id> [--wait] [--timeout <secs>]");
            }
            let mut wait = false;
            let mut timeout_secs: u64 = 3600;
            let extra: Vec<String> = args.collect();
            let mut i = 0;
            while i < extra.len() {
                match extra[i].as_str() {
                    "--wait" => {
                        wait = true;
                        i += 1;
                    }
                    "--timeout" => {
                        if let Some(val) = extra.get(i + 1) {
                            timeout_secs = val.parse().unwrap_or(120);
                        }
                        i += 2;
                    }
                    _ => i += 1,
                }
            }
            cmd_receive(&id, wait, timeout_secs)
        }
        "pending" => cmd_pending(),
        "history" => {
            let agent = args.next();
            cmd_history(agent.as_deref())
        }
        "roles" => cmd_roles(),
        "teams" => cmd_teams(),
        "team" => {
            let name = args.next().unwrap_or_default();
            if name.is_empty() {
                bail!("Usage: squad team <name>");
            }
            cmd_team(&name)
        }
        "setup" => {
            let target = args.next();
            cmd_setup(target.as_deref())
        }
        "clean" => cmd_clean(),
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        "--version" | "-V" => {
            println!("squad {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        other => bail!("unknown command: {other}. Run 'squad help' for usage."),
    }
}

// --- Helpers ---

fn find_workspace() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join(".squad").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!("Not a squad workspace. Run 'squad init' first.");
        }
    }
}

fn open_store(workspace: &Path) -> Result<squad::store::Store> {
    let db_path = workspace.join(".squad").join("messages.db");
    squad::store::Store::open(&db_path)
}

fn ensure_agent_exists(store: &squad::store::Store, id: &str) -> Result<()> {
    if store.agent_exists(id)? {
        return Ok(());
    }
    let agents = store.list_agents()?;
    let names = agents.into_iter().map(|agent| agent.id).collect::<Vec<_>>();
    bail!("{id} does not exist. Online agents: {}", names.join(", "));
}

fn sessions_dir(workspace: &Path) -> PathBuf {
    workspace.join(".squad").join("sessions")
}

/// Check if this agent's session is still valid. Returns Ok(()) if valid or if
/// no session tracking exists (backward compat). Errors with "Session replaced" if displaced.
fn check_session(workspace: &Path, store: &squad::store::Store, agent_id: &str) -> Result<()> {
    let sessions = sessions_dir(workspace);
    if let Some(db_token) = store.get_session_token(agent_id)? {
        squad::session::validate(&sessions, agent_id, &db_token)?;
    }
    Ok(())
}

fn print_messages(messages: &[squad::store::MessageRecord], receiver: Option<&str>) {
    for msg in messages {
        println!("[from {}] {}", msg.from_agent, msg.content);
        if let Some(id) = receiver {
            println!(
                "  → Reply: squad send {id} {} \"<your response>\"",
                msg.from_agent
            );
        }
    }
}

// --- Commands ---

fn cmd_init() -> Result<()> {
    let workspace = std::env::current_dir()?;
    squad::init::init_workspace(&workspace)?;
    println!("Initialized squad workspace.");
    Ok(())
}

fn cmd_join(id: &str, role: &str) -> Result<()> {
    let workspace = find_workspace()?;
    let store = open_store(&workspace)?;
    let (actual_id, token) = store.register_agent_unique(id, role)?;
    store.touch_agent(&actual_id)?;
    squad::session::write_token(&sessions_dir(&workspace), &actual_id, &token)?;
    if actual_id != id {
        println!("ID '{id}' was taken. Joined as {actual_id} (role: {role}).");
    } else {
        println!("Joined as {actual_id} (role: {role}).");
    }

    match squad::roles::load_role(&workspace, role) {
        Ok(prompt) => {
            println!("\n=== Role Instructions ===\n{prompt}");
        }
        Err(_) => {
            println!("\nNo predefined template for \"{role}\". Interpret this role autonomously.");
            println!("Communicate using: squad send, squad receive, squad agents");
            println!("Tip: create .squad/roles/{role}.md to customize behavior.");
            let roles = squad::roles::list_roles(&workspace);
            println!("Predefined roles: {}", roles.join(", "));
        }
    }
    Ok(())
}

fn cmd_leave(id: &str) -> Result<()> {
    let workspace = find_workspace()?;
    let store = open_store(&workspace)?;
    store.unregister_agent(id)?;
    squad::session::delete_token(&sessions_dir(&workspace), id)?;
    println!("{id} left the squad.");
    Ok(())
}

fn cmd_agents() -> Result<()> {
    let workspace = find_workspace()?;
    let store = open_store(&workspace)?;
    let agents = store.list_agents()?;
    if agents.is_empty() {
        println!("No agents online.");
    } else {
        let now = chrono::Utc::now().timestamp();
        for agent in &agents {
            let status = match agent.last_seen {
                Some(ts) => {
                    let ago = now - ts;
                    if ago < 60 {
                        format!("active ({}s ago)", ago)
                    } else if ago < 600 {
                        format!("idle ({}m ago)", ago / 60)
                    } else {
                        format!("stale ({}m ago)", ago / 60)
                    }
                }
                None => "unknown".to_string(),
            };
            println!("  {} (role: {}) — {}", agent.id, agent.role, status);
        }
    }
    Ok(())
}

fn cmd_send(from: &str, to: &str, content: &str) -> Result<()> {
    let workspace = find_workspace()?;
    let store = open_store(&workspace)?;
    ensure_agent_exists(&store, from)?;
    check_session(&workspace, &store, from)?;
    store.touch_agent(from)?;
    if to == "@all" {
        let recipients = store.broadcast_message(from, content)?;
        println!(
            "Broadcast to {} agents: {}",
            recipients.len(),
            recipients.join(", ")
        );
    } else {
        store.send_message_checked(from, to, content)?;
        println!("Sent to {to}.");
    }
    Ok(())
}

fn cmd_receive(agent: &str, wait: bool, timeout_secs: u64) -> Result<()> {
    let workspace = find_workspace()?;

    // Validate session at entry (catches displacement immediately)
    let store = open_store(&workspace)?;
    check_session(&workspace, &store, agent)?;
    store.touch_agent(agent)?;

    if wait {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
        let mut last_heartbeat = std::time::Instant::now();
        loop {
            let store = open_store(&workspace)?;

            // Re-check for displacement on each poll (~500ms)
            check_session(&workspace, &store, agent)?;

            // Heartbeat: update last_seen every 30s so other agents know we're alive
            if last_heartbeat.elapsed() >= std::time::Duration::from_secs(30) {
                store.touch_agent(agent)?;
                last_heartbeat = std::time::Instant::now();
            }

            if store.has_unread_messages(agent)? {
                let messages = store.receive_messages(agent)?;
                if !messages.is_empty() {
                    print_messages(&messages, Some(agent));
                    return Ok(());
                }
            }
            if std::time::Instant::now() > deadline {
                println!("No new messages (timed out after {timeout_secs}s).");
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    } else {
        let messages = store.receive_messages(agent)?;
        if messages.is_empty() {
            println!("No new messages.");
        } else {
            print_messages(&messages, Some(agent));
        }
        Ok(())
    }
}

fn cmd_pending() -> Result<()> {
    let workspace = find_workspace()?;
    let store = open_store(&workspace)?;
    let messages = store.pending_messages()?;
    if messages.is_empty() {
        println!("No pending messages.");
    } else {
        println!("Pending messages:");
        for msg in &messages {
            let preview: String = msg.content.chars().take(60).collect();
            let suffix = if msg.content.chars().count() > 60 {
                "..."
            } else {
                ""
            };
            println!(
                "  {} -> {}: {}{}",
                msg.from_agent, msg.to_agent, preview, suffix
            );
        }
    }
    Ok(())
}

fn cmd_history(agent_id: Option<&str>) -> Result<()> {
    let workspace = find_workspace()?;
    let store = open_store(&workspace)?;
    let messages = store.all_messages(agent_id)?;
    if messages.is_empty() {
        println!("No message history.");
    } else {
        for msg in &messages {
            let marker = if msg.read { "  " } else { "* " };
            println!(
                "{marker}{} -> {}: {}",
                msg.from_agent, msg.to_agent, msg.content
            );
        }
    }
    Ok(())
}

fn cmd_roles() -> Result<()> {
    let workspace = find_workspace()?;
    let roles = squad::roles::list_roles(&workspace);
    println!("Available roles:");
    for role in &roles {
        println!("  {role}");
    }
    Ok(())
}

fn cmd_teams() -> Result<()> {
    let workspace = find_workspace()?;
    let teams = squad::teams::list_teams(&workspace);
    println!("Available teams:");
    for team in &teams {
        println!("  {team}");
    }
    Ok(())
}

fn cmd_team(name: &str) -> Result<()> {
    let workspace = find_workspace()?;
    let team = squad::teams::load_team(&workspace, name)?;
    println!("Team: {}", team.name);
    println!("Roles:");
    for (role_id, role) in &team.roles {
        println!("  {role_id} (prompt: {})", role.prompt_file);
        println!("    → squad join {role_id} --role {}", role.prompt_file);
    }
    Ok(())
}

fn cmd_clean() -> Result<()> {
    let workspace = find_workspace()?;
    let db_path = workspace.join(".squad").join("messages.db");
    if db_path.exists() {
        std::fs::remove_file(&db_path)?;
    }
    // Also remove WAL and SHM files
    let wal = workspace.join(".squad").join("messages.db-wal");
    let shm = workspace.join(".squad").join("messages.db-shm");
    if wal.exists() {
        std::fs::remove_file(&wal)?;
    }
    if shm.exists() {
        std::fs::remove_file(&shm)?;
    }
    squad::session::delete_all(&workspace.join(".squad").join("sessions"))?;
    println!("Cleaned squad state.");
    Ok(())
}

fn cmd_setup(target: Option<&str>) -> Result<()> {
    match target {
        Some("--list") => {
            println!("Supported platforms:");
            for p in squad::setup::PLATFORMS {
                let status = if squad::setup::is_installed(p.binary) {
                    "installed"
                } else {
                    "not found"
                };
                println!("  {} ({}: {})", p.name, p.binary, status);
            }
            Ok(())
        }
        Some(name) => {
            let platform = squad::setup::PLATFORMS
                .iter()
                .find(|p| p.name == name)
                .with_context(|| format!("unknown platform: {name}. Run 'squad setup --list'"))?;
            let path = squad::setup::install_for_platform(platform)?;
            println!("Installed squad for {} → {}", name, path.display());
            Ok(())
        }
        None => {
            println!("Detecting installed AI tools...");
            let results = squad::setup::run_setup();
            if results.is_empty() {
                println!("No supported AI tools found in PATH.");
                println!("Supported: claude, gemini, codex, opencode");
                return Ok(());
            }
            for (name, path, result) in &results {
                match result {
                    Ok(()) => println!("  {} → {}", name, path.display()),
                    Err(e) => println!("  {} — {}", name, e),
                }
            }
            let ok_count = results.iter().filter(|(_, _, r)| r.is_ok()).count();
            println!("Installed squad for {} tool(s).", ok_count);
            Ok(())
        }
    }
}

fn print_usage() {
    print!("{HELP_TEXT}");
}

const HELP_TEXT: &str = r#"squad — Multi-AI-agent terminal collaboration

COMMANDS
  squad init                                Initialize workspace
  squad join <id> [--role <role>]            Join as agent (role defaults to id)
  squad leave <id>                           Remove agent
  squad agents                               List online agents
  squad send <from> <to> <message>           Send message (use @all to broadcast)
  squad receive <id> [--wait] [--timeout N]  Check inbox (--wait blocks until message arrives)
  squad pending                              Show all unread messages
  squad history [agent]                      Show all messages (including read)
  squad roles                                List available roles
  squad teams                                List available teams
  squad team <name>                          Show team template
  squad setup [platform]                      Install /squad slash command for AI tools
  squad setup --list                         List supported platforms
  squad clean                                Clear all state

QUICK START
  1. squad init                              Set up workspace
  2. squad join manager --role manager        Join as manager in terminal 1
  3. squad join worker --role worker          Join as worker in terminal 2
  4. squad send manager worker "task..."      Manager assigns task
  5. squad receive worker                     Worker checks for task
  6. squad send worker manager "done..."      Worker reports back

HOW TO PARTICIPATE
  When told a role (e.g. "you are manager"), run:
  1. squad join <role> --role <role>          Register and read role instructions
  2. Do your work as instructed by the role
  3. squad send <your-id> <to> "result"       Report results
  4. squad receive <your-id>                  Check for next task or feedback

EXAMPLES
  squad send manager worker "implement auth module with JWT"
  squad send manager @all "API contract updated, rebase your work"
  squad receive worker
  squad history worker
"#;
