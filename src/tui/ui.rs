use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::app::App;

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(20)])
        .split(layout[1]);

    let header = Paragraph::new(header_line(app))
        .block(Block::default().title("Squad").borders(Borders::ALL));
    frame.render_widget(header, layout[0]);

    let agent_items: Vec<ListItem> = app
        .agents
        .iter()
        .map(|agent| {
            let status_icon = if agent.status == "working" {
                "●"
            } else {
                "○"
            };
            ListItem::new(Line::from(format!(
                "{status_icon} {} {}",
                agent.agent_id, agent.status
            )))
        })
        .collect();
    let agents = List::new(if agent_items.is_empty() {
        vec![ListItem::new("No agents")]
    } else {
        agent_items
    })
    .block(Block::default().title("Agents").borders(Borders::ALL));
    frame.render_widget(agents, body[0]);

    let message_lines: Vec<Line> = if app.messages.is_empty() {
        vec![Line::from("[system] Waiting for daemon activity")]
    } else {
        app.messages
            .iter()
            .map(|message| {
                Line::from(vec![
                    Span::styled(
                        format!("[{}] ", message.label),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(message.content.clone()),
                ])
            })
            .collect()
    };
    let messages = Paragraph::new(message_lines)
        .block(
            Block::default()
                .title("Message Stream")
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(messages, body[1]);

    let footer = Paragraph::new("[q]uit [s]tatus [l]og [r]estart")
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, layout[2]);
}

fn header_line(app: &App) -> Line<'static> {
    let step = if app.progress.max_iterations == 0 {
        "Step 0/0".to_string()
    } else {
        format!(
            "Step {}/{}",
            app.progress.iteration.max(1),
            app.progress.max_iterations
        )
    };
    let mut sections = vec![
        format!("Project: {}", fallback(&app.progress.project, "unknown")),
        format!(
            "Workflow: {}",
            fallback(&app.progress.workflow_mode, "loop")
        ),
        step,
    ];
    if let Some(current_step) = &app.progress.current_step {
        sections.push(format!("Current: {current_step}"));
    }
    if !app.progress.running {
        sections.push("Daemon: offline".to_string());
    }
    Line::from(sections.join(" | "))
}

fn fallback<'a>(value: &'a str, default: &'a str) -> &'a str {
    if value.is_empty() {
        default
    } else {
        value
    }
}
