use anyhow::Result;
use async_trait::async_trait;
use squad::config::SquadConfig;
use squad::daemon::WorkflowDispatcher;
use squad::workflow::engine::{WorkflowEngine, WorkflowOutcome};
use squad::workflow::WorkflowState;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

#[derive(Clone, Default)]
struct RecordingDispatcher {
    sent: Arc<Mutex<Vec<(String, String, usize)>>>,
}

impl RecordingDispatcher {
    fn messages(&self) -> Vec<(String, String, usize)> {
        self.sent.lock().expect("recorded messages").clone()
    }
}

#[async_trait]
impl WorkflowDispatcher for RecordingDispatcher {
    async fn dispatch(&self, recipient: &str, message: squad::workflow::WorkflowMessage) -> Result<()> {
        self.sent.lock().expect("recorded messages").push((
            recipient.to_string(),
            message.content,
            message.iteration,
        ));
        Ok(())
    }
}

#[test]
fn parses_modern_workflow_yaml_and_generates_agent_step_ids() -> Result<()> {
    let config = SquadConfig::from_yaml(
        r#"
project: my-project
workflow:
  mode: pipeline
  max_iterations: 3
  on_timeout: restart
  timeout_seconds: 120
  steps:
    - agent: cc
      action: implement
      prompt: "Implement: {goal}"
      then: codex
    - agent: codex
      action: review
      prompt: "Review: {previous_output}"
      on_pass: done
"#,
    )?;

    assert_eq!(config.project, "my-project");
    assert_eq!(config.workflow.mode.as_str(), "pipeline");
    assert_eq!(config.workflow.max_iterations, 3);
    assert_eq!(config.workflow.on_timeout.as_str(), "restart");
    assert_eq!(config.workflow.timeout_seconds, 120);
    assert_eq!(config.workflow.start_at, "cc");
    assert_eq!(config.workflow.steps[0].id, "cc");
    assert_eq!(config.workflow.steps[0].next.as_deref(), Some("codex"));
    assert_eq!(config.workflow.steps[1].id, "codex");
    Ok(())
}

#[tokio::test]
async fn pipeline_mode_passes_previous_output_into_next_prompt() -> Result<()> {
    let config = SquadConfig::from_yaml(
        r#"
workflow:
  mode: pipeline
  steps:
    - agent: cc
      prompt: "Implement the feature: {goal}"
      then: codex
    - agent: codex
      prompt: "Review this output: {previous_output} (iteration {iteration})"
      on_pass: done
"#,
    )?;
    let dispatcher = RecordingDispatcher::default();
    let mut engine = WorkflowEngine::new(config.workflow.clone(), dispatcher.clone());
    let mut state = WorkflowState::new(config.workflow.start_at.clone());

    engine.start(&mut state, "ship pipeline mode").await?;
    let outcome = engine
        .handle_mark_done_with_goal(&mut state, "cc", "implemented pipeline mode", "ship pipeline mode")
        .await?;

    assert_eq!(outcome, WorkflowOutcome::Advanced);
    assert_eq!(state.current_step.as_deref(), Some("codex"));
    assert_eq!(state.iteration, 1);
    let messages = dispatcher.messages();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].0, "cc");
    assert!(messages[0].1.contains("ship pipeline mode"));
    assert_eq!(messages[1].0, "codex");
    assert!(messages[1].1.contains("implemented pipeline mode"));
    assert!(messages[1].1.contains("iteration 1"));
    Ok(())
}

#[tokio::test]
async fn loop_mode_stops_when_max_iterations_is_reached() -> Result<()> {
    let config = SquadConfig::from_yaml(
        r#"
workflow:
  mode: loop
  max_iterations: 1
  on_timeout: stop
  start_at: review
  steps:
    - id: review
      agent: codex
      prompt: "Review {goal}"
      on_fail: implement
      on_pass: done
    - id: implement
      agent: cc
      prompt: "Fix: {previous_output}"
      then: review
"#,
    )?;
    let dispatcher = RecordingDispatcher::default();
    let mut engine = WorkflowEngine::new(config.workflow.clone(), dispatcher.clone());
    let mut state = WorkflowState::new(config.workflow.start_at.clone());

    engine.start(&mut state, "close review gaps").await?;
    let outcome = engine
        .handle_mark_done_with_goal(&mut state, "codex", "fail: still missing tests", "close review gaps")
        .await?;

    assert_eq!(outcome, WorkflowOutcome::Complete);
    assert_eq!(state.current_step, None);
    assert_eq!(state.iteration, 1);
    let messages = dispatcher.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].0, "codex");
    Ok(())
}

#[tokio::test]
async fn parallel_mode_dispatches_all_agents_and_waits_for_every_result() -> Result<()> {
    let config = SquadConfig::from_yaml(
        r#"
workflow:
  mode: parallel
  steps:
    - agent: cc
      prompt: "Implement {goal}"
    - agent: codex
      prompt: "Review {goal}"
"#,
    )?;
    let dispatcher = RecordingDispatcher::default();
    let mut engine = WorkflowEngine::new(config.workflow.clone(), dispatcher.clone());
    let mut state = WorkflowState::new(config.workflow.start_at.clone());

    engine.start(&mut state, "parallel workflow support").await?;
    let first = engine
        .handle_mark_done_with_goal(&mut state, "cc", "implemented workflow support", "parallel workflow support")
        .await?;
    let second = engine
        .handle_mark_done_with_goal(&mut state, "codex", "LGTM with 2 notes", "parallel workflow support")
        .await?;

    assert_eq!(first, WorkflowOutcome::Waiting);
    assert_eq!(second, WorkflowOutcome::Complete);
    assert_eq!(state.current_step, None);
    assert_eq!(state.iteration, 1);
    assert!(state
        .previous_output
        .as_deref()
        .expect("aggregate output")
        .contains("LGTM with 2 notes"));
    let messages = dispatcher.messages();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].0, "cc");
    assert_eq!(messages[1].0, "codex");
    Ok(())
}

#[test]
fn workflow_state_round_trips_through_state_file() -> Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("state.json");
    let mut state = WorkflowState::new("review".to_string());
    state.iteration = 3;
    state.previous_output = Some("reviewed output".to_string());

    state.save_to_path(&path)?;
    let loaded = WorkflowState::load_from_path(&path)?;

    assert_eq!(loaded.current_step.as_deref(), Some("review"));
    assert_eq!(loaded.iteration, 3);
    assert_eq!(loaded.previous_output.as_deref(), Some("reviewed output"));
    assert!(loaded.started_at_unix > 0);
    Ok(())
}
