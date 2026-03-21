use crate::config::{TimeoutPolicy, WorkflowConfig, WorkflowMode, WorkflowStepConfig};
use crate::daemon::WorkflowDispatcher;
use crate::workflow::{WorkflowMessage, WorkflowState};
use anyhow::{bail, Context, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowOutcome {
    Advanced,
    Waiting,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReviewDisposition {
    Pass,
    Fail,
}

pub struct WorkflowEngine<D> {
    workflow: WorkflowConfig,
    dispatcher: D,
}

impl<D> WorkflowEngine<D>
where
    D: WorkflowDispatcher,
{
    pub fn new(workflow: WorkflowConfig, dispatcher: D) -> Self {
        Self {
            workflow,
            dispatcher,
        }
    }

    pub async fn start(&self, state: &mut WorkflowState, goal: &str) -> Result<()> {
        match self.workflow.mode {
            WorkflowMode::Parallel => {
                state.active_steps = self
                    .workflow
                    .steps
                    .iter()
                    .map(|step| step.id.clone())
                    .collect();
                state.current_step = state.active_steps.first().cloned();
                for step_id in state.active_steps.clone() {
                    let step = self
                        .step(&step_id)
                        .with_context(|| format!("unknown workflow step: {step_id}"))?;
                    self.dispatch_step(step, state, goal).await?;
                }
            }
            _ => {
                let current_id = state
                    .current_step
                    .clone()
                    .or_else(|| self.start_step_id())
                    .context("workflow has no current step")?;
                state.current_step = Some(current_id.clone());
                state.active_steps = vec![current_id.clone()];
                let step = self
                    .step(&current_id)
                    .with_context(|| format!("unknown workflow step: {current_id}"))?;
                self.dispatch_step(step, state, goal).await?;
            }
        }
        Ok(())
    }

    pub async fn handle_mark_done(
        &mut self,
        state: &mut WorkflowState,
        summary: &str,
    ) -> Result<WorkflowOutcome> {
        let current_id = state
            .current_step
            .clone()
            .context("workflow has no current step")?;
        self.handle_mark_done_with_goal(state, &current_id, summary, "")
            .await
    }

    pub async fn handle_mark_done_with_goal(
        &mut self,
        state: &mut WorkflowState,
        step_or_agent: &str,
        summary: &str,
        goal: &str,
    ) -> Result<WorkflowOutcome> {
        match self.workflow.mode {
            WorkflowMode::Parallel => self.handle_parallel_mark_done(state, step_or_agent, summary).await,
            WorkflowMode::Loop | WorkflowMode::Pipeline => {
                self.handle_serial_mark_done(state, step_or_agent, summary, goal)
                    .await
            }
        }
    }

    fn step(&self, id: &str) -> Option<&WorkflowStepConfig> {
        self.workflow.steps.iter().find(|step| step.id == id)
    }

    async fn handle_serial_mark_done(
        &mut self,
        state: &mut WorkflowState,
        step_or_agent: &str,
        summary: &str,
        goal: &str,
    ) -> Result<WorkflowOutcome> {
        let current_id = state
            .current_step
            .clone()
            .context("workflow has no current step")?;
        let resolved_id = self
            .resolve_step_id(step_or_agent)
            .unwrap_or_else(|| current_id.clone());
        if resolved_id != current_id {
            bail!("mark_done for inactive step: {step_or_agent}");
        }

        let current = self
            .step(&current_id)
            .with_context(|| format!("unknown workflow step: {current_id}"))?;
        state.iteration += 1;
        state.previous_output = Some(summary.to_string());

        if self.workflow.mode == WorkflowMode::Loop
            && state.iteration >= self.workflow.max_iterations
            && self.next_step_id(current, summary).is_some()
        {
            return self.handle_timeout(state, current, goal).await;
        }

        let Some(next_id) = self.next_step_id(current, summary) else {
            state.current_step = None;
            state.active_steps.clear();
            return Ok(WorkflowOutcome::Complete);
        };

        state.current_step = Some(next_id.clone());
        state.active_steps = vec![next_id.clone()];
        let next = self
            .step(&next_id)
            .with_context(|| format!("unknown next workflow step: {next_id}"))?;
        self.dispatch_step(next, state, goal).await?;
        Ok(WorkflowOutcome::Advanced)
    }

    async fn handle_parallel_mark_done(
        &mut self,
        state: &mut WorkflowState,
        step_or_agent: &str,
        summary: &str,
    ) -> Result<WorkflowOutcome> {
        let step_id = self
            .resolve_step_id(step_or_agent)
            .context("parallel workflow step not found")?;
        if !state.active_steps.iter().any(|candidate| candidate == &step_id) {
            bail!("mark_done for inactive parallel step: {step_or_agent}");
        }

        state.active_steps.retain(|candidate| candidate != &step_id);
        state.parallel_outputs.push(format!("[{step_id}] {summary}"));
        state.previous_output = Some(state.parallel_outputs.join("\n"));

        if state.active_steps.is_empty() {
            state.current_step = None;
            state.iteration += 1;
            return Ok(WorkflowOutcome::Complete);
        }

        state.current_step = state.active_steps.first().cloned();
        Ok(WorkflowOutcome::Waiting)
    }

    async fn handle_timeout(
        &self,
        state: &mut WorkflowState,
        current: &WorkflowStepConfig,
        goal: &str,
    ) -> Result<WorkflowOutcome> {
        if let Some(next_id) = normalize_target(current.on_timeout.clone()) {
            state.current_step = Some(next_id.clone());
            state.active_steps = vec![next_id.clone()];
            let next = self
                .step(&next_id)
                .with_context(|| format!("unknown timeout workflow step: {next_id}"))?;
            self.dispatch_step(next, state, goal).await?;
            return Ok(WorkflowOutcome::Advanced);
        }

        match self.workflow.on_timeout {
            TimeoutPolicy::Stop | TimeoutPolicy::Notify => {
                state.current_step = None;
                state.active_steps.clear();
                Ok(WorkflowOutcome::Complete)
            }
            TimeoutPolicy::Restart => {
                let start_id = self.start_step_id().context("workflow has no start step")?;
                state.reset(&start_id);
                let step = self
                    .step(&start_id)
                    .with_context(|| format!("unknown workflow start step: {start_id}"))?;
                self.dispatch_step(step, state, goal).await?;
                Ok(WorkflowOutcome::Advanced)
            }
        }
    }

    async fn dispatch_step(
        &self,
        step: &WorkflowStepConfig,
        state: &WorkflowState,
        goal: &str,
    ) -> Result<()> {
        let message = WorkflowMessage {
            step_id: step.id.clone(),
            content: render_template(
                &step.message,
                goal,
                state.previous_output.as_deref().unwrap_or(""),
                state.iteration,
            ),
            iteration: state.iteration,
        };
        self.dispatcher.dispatch(&step.agent, message).await
    }

    fn resolve_step_id(&self, step_or_agent: &str) -> Option<String> {
        self.step(step_or_agent)
            .map(|step| step.id.clone())
            .or_else(|| {
                self.workflow
                    .steps
                    .iter()
                    .find(|step| step.agent == step_or_agent)
                    .map(|step| step.id.clone())
            })
    }

    fn next_step_id(&self, step: &WorkflowStepConfig, summary: &str) -> Option<String> {
        let target = if step.on_pass.is_some() || step.on_fail.is_some() {
            match classify_summary(summary) {
                ReviewDisposition::Pass => step
                    .on_pass
                    .clone()
                    .or_else(|| step.next.clone())
                    .or_else(|| self.pipeline_next(step)),
                ReviewDisposition::Fail => step
                    .on_fail
                    .clone()
                    .or_else(|| step.next.clone())
                    .or_else(|| self.pipeline_next(step)),
            }
        } else {
            step.next.clone().or_else(|| self.pipeline_next(step))
        };
        normalize_target(target)
    }

    fn pipeline_next(&self, step: &WorkflowStepConfig) -> Option<String> {
        if self.workflow.mode != WorkflowMode::Pipeline {
            return None;
        }
        let index = self.workflow.steps.iter().position(|candidate| candidate.id == step.id)?;
        self.workflow.steps.get(index + 1).map(|next| next.id.clone())
    }

    fn start_step_id(&self) -> Option<String> {
        if !self.workflow.start_at.trim().is_empty() {
            Some(self.workflow.start_at.clone())
        } else {
            self.workflow.steps.first().map(|step| step.id.clone())
        }
    }
}

fn classify_summary(summary: &str) -> ReviewDisposition {
    let normalized = summary.to_ascii_lowercase();
    if normalized.contains("fail")
        || normalized.contains("changes requested")
        || normalized.contains("blocked")
    {
        ReviewDisposition::Fail
    } else {
        ReviewDisposition::Pass
    }
}

fn normalize_target(target: Option<String>) -> Option<String> {
    target.and_then(|value| {
        if value.eq_ignore_ascii_case("done") {
            None
        } else {
            Some(value)
        }
    })
}

fn render_template(template: &str, goal: &str, previous_output: &str, iteration: usize) -> String {
    template
        .replace("{goal}", goal)
        .replace("{previous_output}", previous_output)
        .replace("{iteration}", &iteration.to_string())
}
