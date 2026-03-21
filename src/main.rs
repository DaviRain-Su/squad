use anyhow::{bail, Context, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());
    let workspace = std::env::current_dir()?;

    match command.as_str() {
        "init" => {
            let fresh = matches!(args.next().as_deref(), Some("--fresh"));
            squad::daemon::init_workspace_with_options(&workspace, fresh)
        }
        "start" => squad::daemon::start_daemon(&workspace),
        "status" => {
            print!("{}", squad::daemon::status_text(&workspace)?);
            Ok(())
        }
        "stop" => squad::daemon::stop_daemon(&workspace),
        "log" => {
            let mut tail = None;
            let mut filter = None;
            let extra: Vec<String> = args.collect();
            let mut index = 0;
            while index < extra.len() {
                match extra[index].as_str() {
                    "--tail" => {
                        let value = extra
                            .get(index + 1)
                            .context("--tail requires a numeric value")?;
                        tail = Some(value.parse::<usize>().context("invalid --tail value")?);
                        index += 2;
                    }
                    "--filter" => {
                        let value = extra
                            .get(index + 1)
                            .context("--filter requires key=value")?;
                        filter = Some(value.clone());
                        index += 2;
                    }
                    other => bail!("unknown log flag: {other}"),
                }
            }
            print!("{}", squad::daemon::log_text(&workspace, tail, filter.as_deref())?);
            Ok(())
        }
        "history" => {
            print!("{}", squad::daemon::history_text(&workspace)?);
            Ok(())
        }
        "clean" => squad::daemon::clean_history(&workspace),
        "watch" => squad::tui::run(&workspace),
        "daemon-run" => squad::daemon::run_daemon_foreground(&workspace).await,
        "setup" => {
            let sub = args.next().unwrap_or_default();
            run_setup(&workspace, &sub, args.collect())
        }
        "doctor" => {
            let results = squad::setup::run_doctor(&workspace)?;
            print!("{}", squad::setup::render_doctor_results(&results));
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        other => bail!("unknown command: {other}"),
    }
}

fn run_setup(workspace: &std::path::Path, sub: &str, extra: Vec<String>) -> Result<()> {
    use squad::setup::{agent_instructions, setup_mcp_json, update_claude_md, SUPPORTED_AGENTS};

    // squad setup --list
    if sub == "--list" || sub == "-l" {
        println!("Supported agents: {}", SUPPORTED_AGENTS.join(", "));
        return Ok(());
    }

    if sub.is_empty() {
        bail!("Usage: squad setup <agent> [--update-claude-md]\n       squad setup --list");
    }

    if !SUPPORTED_AGENTS.contains(&sub) {
        bail!(
            "unknown agent '{}'. Supported: {}",
            sub,
            SUPPORTED_AGENTS.join(", ")
        );
    }

    let update_claude = extra.iter().any(|arg| arg == "--update-claude-md");

    // Write / check .mcp.json
    match setup_mcp_json(workspace, sub)? {
        true => println!(".mcp.json: squad MCP server registered."),
        false => println!(".mcp.json: squad entry already configured."),
    }

    // Optionally update CLAUDE.md
    if update_claude {
        match update_claude_md(workspace)? {
            true => println!("CLAUDE.md: Squad Collaboration Protocol section appended."),
            false => println!("CLAUDE.md: Squad Collaboration Protocol section already present."),
        }
    }

    println!("{}", agent_instructions(sub));
    Ok(())
}

fn print_usage() {
    println!(
        "Usage: squad <init|start|status|stop|log|history|clean|watch|setup|doctor>"
    );
    println!();
    println!("Commands:");
    println!("  init               Initialise workspace (writes squad.yaml)");
    println!("  start              Start the daemon");
    println!("  status             Show daemon status");
    println!("  stop               Stop the daemon");
    println!("  log                Show audit log (--tail N, --filter key=value)");
    println!("  history            Show message history summary");
    println!("  clean              Remove runtime artefacts");
    println!("  watch              Open TUI dashboard");
    println!("  setup <agent>      Register squad MCP server for an agent");
    println!("    --update-claude-md  Also append Squad Collaboration Protocol to CLAUDE.md");
    println!("    --list              List supported agents");
    println!("  doctor             Diagnose daemon, squad-mcp, and .mcp.json");
}
