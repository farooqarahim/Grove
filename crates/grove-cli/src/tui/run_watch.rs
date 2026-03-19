//! Live run-watch TUI pane — polls transport for run state and renders a
//! two-panel view: agent table (left) + log tail (right).

#[cfg(feature = "tui")]
use std::time::{Duration, Instant};

#[cfg(feature = "tui")]
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
#[cfg(feature = "tui")]
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Row, Table, TableState},
};

#[cfg(feature = "tui")]
use crate::transport::{GroveTransport, Transport};

/// A single row in the agent table.
#[cfg(feature = "tui")]
pub struct AgentRow {
    pub name: String,
    pub state: String,
    pub started: Option<String>,
}

/// View-model for the run-watch TUI.
#[cfg(feature = "tui")]
pub struct RunWatchState {
    pub run_id: String,
    pub objective: String,
    pub agents: Vec<AgentRow>,
    pub selected_agent: usize,
    pub log_lines: Vec<String>,
    pub last_refresh: Instant,
    pub scroll_offset: u16,
    pub done: bool,
    pub last_error: Option<String>,
}

#[cfg(feature = "tui")]
impl RunWatchState {
    pub fn new(run_id: String, objective: String) -> Self {
        Self {
            run_id,
            objective,
            agents: Vec::new(),
            selected_agent: 0,
            log_lines: Vec::new(),
            last_refresh: Instant::now(),
            scroll_offset: 0,
            done: false,
            last_error: None,
        }
    }

    pub fn select_next(&mut self) {
        if !self.agents.is_empty() {
            self.selected_agent = (self.selected_agent + 1) % self.agents.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.agents.is_empty() {
            self.selected_agent = self.selected_agent.saturating_sub(1);
        }
    }
}

/// Run the live run-watch loop for `run_id`.
#[cfg(feature = "tui")]
pub fn run(run_id: String, transport: GroveTransport) -> crate::error::CliResult<()> {
    // Fetch the objective from transport upfront.
    let objective = transport
        .get_run(&run_id)
        .ok()
        .flatten()
        .map(|r| r.objective)
        .unwrap_or_default();

    enable_raw_mode().map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| crate::error::CliError::Other(e.to_string()))?;

    let mut state = RunWatchState::new(run_id.clone(), objective);
    let poll_interval = Duration::from_secs(3);
    let mut table_state = TableState::default();

    let result = (|| -> crate::error::CliResult<()> {
        loop {
            // Single get_run call per iteration — used for both the agent-table
            // refresh and the terminal-state check below.
            let current_run = transport.get_run(&run_id).ok().flatten();

            // Refresh data on interval
            if state.last_refresh.elapsed() >= poll_interval {
                // Update agent list from the already-fetched run record.
                if let Some(ref run) = current_run {
                    state.agents.clear();
                    if let Some(agent) = &run.current_agent {
                        state.agents.push(AgentRow {
                            name: agent.clone(),
                            state: run.state.clone(),
                            started: None,
                        });
                    }
                }

                // Refresh logs
                if let Ok(logs) = transport.get_logs(&state.run_id, false) {
                    state.log_lines = logs
                        .iter()
                        .filter_map(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
                        .collect();
                }

                state.last_refresh = Instant::now();
            }

            // Sync table selection
            if state.agents.is_empty() {
                table_state.select(None);
            } else {
                table_state.select(Some(state.selected_agent));
            }

            terminal
                .draw(|f| draw(f, &mut state, &mut table_state))
                .map_err(|e| crate::error::CliError::Other(e.to_string()))?;

            if event::poll(Duration::from_millis(200)).unwrap_or(false) {
                if let Ok(Event::Key(key)) = event::read() {
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => break,
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                        (KeyCode::Char('a'), _) => {
                            if let Err(e) = transport.abort_run(&run_id) {
                                state.last_error = Some(e.to_string());
                            }
                        }
                        (KeyCode::Tab, _) if !state.agents.is_empty() => {
                            state.selected_agent = (state.selected_agent + 1) % state.agents.len();
                        }
                        (KeyCode::Up, _) => {
                            state.scroll_offset = state.scroll_offset.saturating_sub(1);
                        }
                        (KeyCode::Down, _) => {
                            let max_scroll = state.log_lines.len().saturating_sub(1) as u16;
                            state.scroll_offset =
                                state.scroll_offset.saturating_add(1).min(max_scroll);
                        }
                        (KeyCode::Char('j'), _) => state.select_next(),
                        (KeyCode::Char('k'), _) => state.select_prev(),
                        (KeyCode::Char('r'), _) => {
                            state.last_refresh = Instant::now() - poll_interval; // force refresh next loop
                        }
                        _ => {}
                    }
                }
            }

            // Exit automatically when run reaches a terminal state.
            // Re-uses the run record fetched at the top of this iteration.
            if let Some(run) = current_run {
                let terminal_states = ["completed", "failed", "aborted"];
                if terminal_states.contains(&run.state.as_str()) {
                    state.done = true;
                    // Give user a chance to see the final state
                    std::thread::sleep(Duration::from_secs(2));
                    break;
                }
            }
        }
        Ok(())
    })();

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result
}

/// Render function called every frame.
#[cfg(feature = "tui")]
fn draw(f: &mut Frame<'_>, state: &mut RunWatchState, table_state: &mut TableState) {
    let area = f.size();

    // Outer block with title
    let outer = Block::default()
        .title(format!(" grove watch — {} ", state.run_id))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(super::widgets::ACCENT));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    // Split into top (objective banner) + body + error bar (always 1 line, blank when no error)
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    // Objective line
    let obj_text = ratatui::widgets::Paragraph::new(format!("  {}", state.objective))
        .style(Style::default().fg(Color::White));
    f.render_widget(obj_text, vert[0]);

    // Error bar (bottom slot — always reserved, left blank when no error)
    if let Some(ref err) = state.last_error {
        let err_para = ratatui::widgets::Paragraph::new(format!("Error: {err}"))
            .style(Style::default().fg(Color::Red));
        f.render_widget(err_para, vert[2]);
    }

    // Split body into agents (left) + logs (right)
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(vert[1]);

    // Agent table
    let rows: Vec<Row> = state
        .agents
        .iter()
        .map(|agent| {
            let color = super::widgets::state_color(&agent.state);
            Row::new(vec![
                agent.name.clone(),
                agent.started.clone().unwrap_or_default(),
                agent.state.clone(),
            ])
            .style(Style::default().fg(color))
        })
        .collect();

    let widths = [
        Constraint::Percentage(40),
        Constraint::Percentage(30),
        Constraint::Percentage(30),
    ];
    let agent_table = Table::new(rows, widths)
        .header(
            Row::new(vec!["Agent", "Started", "State"])
                .style(Style::default().fg(super::widgets::ACCENT).bold()),
        )
        .block(super::widgets::titled_block("Agents"))
        .highlight_style(Style::default().bg(Color::DarkGray));
    f.render_stateful_widget(agent_table, horiz[0], table_state);

    // Log panel — use scroll_offset to control which lines are visible
    let visible_height = horiz[1].height.saturating_sub(2) as usize;
    let total_lines = state.log_lines.len();
    // Auto-tail: anchor to end, then allow user to scroll back via scroll_offset
    let auto_skip = total_lines.saturating_sub(visible_height);
    let scroll_back = state.scroll_offset as usize;
    let skip = auto_skip.saturating_sub(scroll_back);
    let items: Vec<ListItem> = state.log_lines[skip..]
        .iter()
        .take(visible_height)
        .map(|l| ListItem::new(l.as_str()))
        .collect();
    let log_list = List::new(items).block(super::widgets::titled_block("Logs"));
    f.render_widget(log_list, horiz[1]);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(feature = "tui")]
    fn run_watch_state_initialises() {
        let s = super::RunWatchState::new("run-abc123".into(), "add dark mode".into());
        assert_eq!(s.run_id, "run-abc123");
        assert!(s.agents.is_empty());
        assert_eq!(s.selected_agent, 0);
        assert_eq!(s.scroll_offset, 0);
        assert!(!s.done);
    }

    #[test]
    #[cfg(feature = "tui")]
    fn run_watch_state_select_next_wraps() {
        let mut s = super::RunWatchState::new("run-xyz".into(), "obj".into());
        // With no agents, select_next is a no-op
        s.select_next();
        assert_eq!(s.selected_agent, 0);
        // With agents, it wraps
        s.agents = vec![
            super::AgentRow {
                name: "a1".into(),
                state: "running".into(),
                started: None,
            },
            super::AgentRow {
                name: "a2".into(),
                state: "queued".into(),
                started: None,
            },
        ];
        s.select_next();
        assert_eq!(s.selected_agent, 1);
        s.select_next();
        assert_eq!(s.selected_agent, 0); // wrapped
    }

    #[test]
    #[cfg(feature = "tui")]
    fn run_watch_state_select_prev_saturates() {
        let mut s = super::RunWatchState::new("run-xyz".into(), "obj".into());
        s.agents = vec![super::AgentRow {
            name: "a1".into(),
            state: "running".into(),
            started: None,
        }];
        s.select_prev();
        assert_eq!(s.selected_agent, 0); // saturating_sub
    }
}
