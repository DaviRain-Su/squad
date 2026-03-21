use crate::daemon::{WatchAgent, WatchMessage, WatchSnapshot};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkflowProgress {
    pub project: String,
    pub workflow_mode: String,
    pub current_step: Option<String>,
    pub iteration: usize,
    pub max_iterations: usize,
    pub started_at_unix: Option<u64>,
    pub running: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct App {
    pub agents: Vec<WatchAgent>,
    pub messages: Vec<WatchMessage>,
    pub progress: WorkflowProgress,
    pub should_quit: bool,
}

impl App {
    pub fn apply_snapshot(&mut self, snapshot: WatchSnapshot) {
        self.progress = WorkflowProgress {
            project: snapshot.project,
            workflow_mode: snapshot.workflow_mode,
            current_step: snapshot.current_step,
            iteration: snapshot.iteration,
            max_iterations: snapshot.max_iterations,
            started_at_unix: snapshot.started_at_unix,
            running: snapshot.running,
        };
        self.agents = snapshot.agents;
        self.messages = snapshot.messages;
        if self.messages.len() > 64 {
            self.messages = self.messages.split_off(self.messages.len() - 64);
        }
    }

    pub fn on_key(&mut self, key: char) {
        if key == 'q' {
            self.should_quit = true;
        }
    }
}
