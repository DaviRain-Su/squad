use anyhow::{bail, Context, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_string());
    let workspace = std::env::current_dir()?;

    match command.as_str() {
        "init" => {
            let flag = args.next();
            let fresh = matches!(flag.as_deref(), Some("--fresh") | Some("--force"));
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
        "run" => {
            let goal: String = std::iter::once(args.next())
                .flatten()
                .chain(args)
                .collect::<Vec<_>>()
                .join(" ");
            if goal.trim().is_empty() {
                bail!("Usage: squad run <goal>\nExample: squad run \"implement login feature\"");
            }
            let paths = squad::daemon::DaemonPaths::new(&workspace);
            let response = squad::daemon::send_request(
                paths.socket_path(),
                &squad::protocol::Request::StartWorkflow { goal },
            )
            .await
            .map_err(|err| {
                let msg = err.to_string();
                if msg.contains("failed to connect")
                    || msg.contains("No such file")
                    || msg.contains("Connection refused")
                    || msg.contains("os error 2")
                {
                    anyhow::anyhow!(
                        "Squad daemon is not running. Run squad start first."
                    )
                } else {
                    err
                }
            })?;
            match response {
                squad::protocol::Response::Ok(_) => {
                    println!("Workflow started. Run 'squad watch' to observe progress.");
                    Ok(())
                }
                squad::protocol::Response::Error { message } => bail!(message),
            }
        }
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
        "--version" | "-V" => {
            println!("squad {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        other => bail!("unknown command: {other}"),
    }
}

fn run_setup(workspace: &std::path::Path, sub: &str, extra: Vec<String>) -> Result<()> {
    use squad::setup::{
        agent_instructions, setup_hook_agent, setup_mcp_json, update_claude_md, MCP_AGENTS,
        SUPPORTED_AGENTS,
    };

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

    if MCP_AGENTS.contains(&sub) {
        // MCP agent: write .mcp.json
        match setup_mcp_json(workspace, sub)? {
            true => println!(".mcp.json: squad MCP server registered."),
            false => println!(".mcp.json: squad entry already configured."),
        }
        if update_claude {
            match update_claude_md(workspace)? {
                true => println!("CLAUDE.md: Squad Collaboration Protocol section appended."),
                false => println!("CLAUDE.md: Squad Collaboration Protocol section already present."),
            }
        }
    } else {
        // Hook agent: write .squad/hooks/<agent>.sh
        match setup_hook_agent(workspace, sub)? {
            true => println!(".squad/hooks/{sub}.sh: hook script created."),
            false => println!(".squad/hooks/{sub}.sh: hook script already exists."),
        }
    }

    println!("{}", agent_instructions(sub));
    Ok(())
}

fn print_usage() {
    println!(
        "Usage: squad <init|start|run|status|stop|log|history|clean|watch|setup|doctor>"
    );
    println!();
    println!("Commands:");
    println!("  init               Initialise workspace (writes squad.yaml)");
    println!("    --force             Overwrite existing squad.yaml");
    println!("  run <goal>         Start the workflow with a goal string");
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
