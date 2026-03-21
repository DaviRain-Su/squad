use anyhow::{bail, Result};
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
                    role = extra
                        .get(i + 1)
                        .cloned()
                        .unwrap_or_else(|| id.clone());
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
            let mut timeout_secs: u64 = 120;
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

fn print_messages(messages: &[squad::store::MessageRecord]) {
    for msg in messages {
        println!("[from {}] {}", msg.from_agent, msg.content);
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
    store.register_agent(id, role)?;
    println!("Joined as {id} (role: {role}).");

    // Output role prompt if available
    match squad::roles::load_role(&workspace, role) {
        Ok(prompt) => {
            println!("\n=== Role Instructions ===\n{prompt}");
        }
        Err(_) => {} // Custom role not found, that's fine
    }
    Ok(())
}

fn cmd_leave(id: &str) -> Result<()> {
    let workspace = find_workspace()?;
    let store = open_store(&workspace)?;
    store.unregister_agent(id)?;
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
        for agent in &agents {
            println!("  {} (role: {})", agent.id, agent.role);
        }
    }
    Ok(())
}

fn cmd_send(from: &str, to: &str, content: &str) -> Result<()> {
    let workspace = find_workspace()?;
    let store = open_store(&workspace)?;
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

    if wait {
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
        loop {
            let store = open_store(&workspace)?;
            let messages = store.receive_messages(agent)?;
            if !messages.is_empty() {
                print_messages(&messages);
                return Ok(());
            }
            if std::time::Instant::now() > deadline {
                println!("No new messages (timed out after {timeout_secs}s).");
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    } else {
        let store = open_store(&workspace)?;
        let messages = store.receive_messages(agent)?;
        if messages.is_empty() {
            println!("No new messages.");
        } else {
            print_messages(&messages);
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
            let status = if msg.read { " " } else { "*" };
            println!(
                " {status} {} -> {}: {}",
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
    println!("Cleaned squad state.");
    Ok(())
}

fn print_usage() {
    println!("squad v{} — Multi-AI-agent terminal collaboration", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage: squad <command> [args]");
    println!();
    println!("Commands:");
    println!("  init                                Initialize workspace");
    println!("  join <id> [--role <role>]            Join as agent (role defaults to id)");
    println!("  leave <id>                           Remove agent");
    println!("  agents                               List online agents");
    println!("  send <from> <to> <message>           Send message (use @all to broadcast)");
    println!("  receive <id> [--wait] [--timeout N]  Check inbox (--wait blocks until message)");
    println!("  pending                              Show all unread messages");
    println!("  history [agent]                      Show all messages (including read)");
    println!("  roles                                List available roles");
    println!("  teams                                List available teams");
    println!("  team <name>                          Show team template");
    println!("  clean                                Clear all state");
}
