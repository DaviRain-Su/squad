pub mod app;
pub mod ui;

use std::io::{self, Stdout};
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::daemon;
use crate::tui::app::App;

pub fn run(workspace_root: impl AsRef<Path>) -> Result<()> {
    let workspace_root = workspace_root.as_ref().to_path_buf();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_loop(&mut terminal, &workspace_root);
    restore_terminal(terminal)?;
    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    workspace_root: &Path,
) -> Result<()> {
    let mut app = App::default();

    loop {
        let snapshot = daemon::watch_snapshot(workspace_root)?;
        app.apply_snapshot(snapshot);
        terminal.draw(|frame| ui::render(frame, &app))?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char(ch) => app.on_key(ch),
                        KeyCode::Esc => app.should_quit = true,
                        _ => {}
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn restore_terminal(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

#[allow(dead_code)]
fn _terminal_is_raw_enabled() -> bool {
    terminal::is_raw_mode_enabled().unwrap_or(false)
}
