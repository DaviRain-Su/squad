use anyhow::Result;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use squad::daemon::{WatchAgent, WatchMessage, WatchSnapshot};
use squad::tui::app::App;
use tempfile::tempdir;

#[test]
fn watch_snapshot_reads_workspace_state_and_message_log() -> Result<()> {
    let workspace = tempdir()?;
    std::fs::create_dir_all(workspace.path().join(".squad"))?;
    std::fs::write(
        workspace.path().join("squad.yaml"),
        r#"
project: my-project
workflow:
  mode: loop
  max_iterations: 10
  steps:
    - agent: cc
      prompt: "Implement {goal}"
    - agent: codex
      prompt: "Review {previous_output}"
"#,
    )?;
    std::fs::write(
        workspace.path().join(".squad/state.json"),
        r#"{
  "current_step": "codex",
  "iteration": 2,
  "started_at_unix": 1710000000,
  "previous_output": "implemented support",
  "active_steps": ["codex"],
  "parallel_outputs": []
}"#,
    )?;
    std::fs::write(
        workspace.path().join(".squad/messages.log"),
        "{\"label\":\"cc -> codex\",\"content\":\"Implementation ready\"}\n",
    )?;

    let snapshot = squad::daemon::watch_snapshot(workspace.path())?;

    assert_eq!(snapshot.project, "my-project");
    assert_eq!(snapshot.workflow_mode, "loop");
    assert_eq!(snapshot.iteration, 2);
    assert_eq!(snapshot.max_iterations, 10);
    assert_eq!(snapshot.current_step.as_deref(), Some("codex"));
    assert_eq!(snapshot.messages.len(), 1);
    assert_eq!(snapshot.messages[0].label, "cc -> codex");
    assert!(!snapshot.running);
    Ok(())
}

#[test]
fn app_updates_from_snapshot_and_handles_quit_key() {
    let mut app = App::default();
    app.apply_snapshot(WatchSnapshot {
        project: "demo".into(),
        workflow_mode: "parallel".into(),
        iteration: 3,
        max_iterations: 10,
        current_step: Some("codex".into()),
        started_at_unix: Some(1710000000),
        running: true,
        agents: vec![WatchAgent {
            agent_id: "cc".into(),
            role: "implementer".into(),
            status: "working".into(),
            health: "online".into(),
            last_seen_unix: 1710000000,
        }],
        messages: vec![WatchMessage {
            label: "system".into(),
            content: "Iteration 3 started".into(),
        }],
    });

    assert_eq!(app.progress.project, "demo");
    assert_eq!(app.progress.workflow_mode, "parallel");
    assert_eq!(app.agents.len(), 1);
    assert_eq!(app.messages.len(), 1);
    assert!(!app.should_quit);

    app.on_key('q');
    assert!(app.should_quit);
}

#[test]
fn ui_render_draws_project_agents_and_message_stream() -> Result<()> {
    let mut terminal = Terminal::new(TestBackend::new(100, 24))?;
    let mut app = App::default();
    app.apply_snapshot(WatchSnapshot {
        project: "my-project".into(),
        workflow_mode: "loop".into(),
        iteration: 2,
        max_iterations: 10,
        current_step: Some("codex".into()),
        started_at_unix: Some(1710000000),
        running: true,
        agents: vec![
            WatchAgent {
                agent_id: "cc".into(),
                role: "implementer".into(),
                status: "working".into(),
                health: "online".into(),
                last_seen_unix: 1710000000,
            },
            WatchAgent {
                agent_id: "codex".into(),
                role: "reviewer".into(),
                status: "idle".into(),
                health: "online".into(),
                last_seen_unix: 1710000000,
            },
        ],
        messages: vec![WatchMessage {
            label: "cc -> codex".into(),
            content: "Implementation ready".into(),
        }],
    });

    terminal.draw(|frame| squad::tui::ui::render(frame, &app))?;
    let backend = terminal.backend();
    let lines: Vec<String> = (0..24)
        .map(|y| {
            (0..100)
                .map(|x| {
                    backend
                        .buffer()
                        .cell((x, y))
                        .map(|cell| cell.symbol())
                        .unwrap_or(" ")
                })
                .collect::<String>()
        })
        .collect();
    let screen = lines.join("\n");

    assert!(screen.contains("Project: my-project"));
    assert!(screen.contains("Workflow: loop"));
    assert!(screen.contains("cc"));
    assert!(screen.contains("Implementation ready"));
    assert!(screen.contains("[q]uit"));
    Ok(())
}
