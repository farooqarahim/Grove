//! Top-level `grove tui` dashboard — multi-tab TUI with Sessions, Issues, Settings, and Dashboard screens.

#[cfg(feature = "tui")]
use std::time::Duration;

#[cfg(feature = "tui")]
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
#[cfg(feature = "tui")]
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

#[cfg(feature = "tui")]
use crate::transport::{GroveTransport, Transport};

/// The active tab / screen the dashboard is showing.
#[cfg(feature = "tui")]
#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum Screen {
    Dashboard,
    #[default]
    Sessions,
    Issues,
    Settings,
}

#[cfg(feature = "tui")]
impl Screen {
    /// Returns the next screen in the cycle.
    pub fn next(self) -> Self {
        match self {
            Screen::Dashboard => Screen::Sessions,
            Screen::Sessions => Screen::Issues,
            Screen::Issues => Screen::Settings,
            Screen::Settings => Screen::Dashboard,
        }
    }

    /// Returns the 0-based index for this screen (used for tab highlighting).
    #[allow(dead_code)]
    pub fn index(self) -> usize {
        match self {
            Screen::Dashboard => 0,
            Screen::Sessions => 1,
            Screen::Issues => 2,
            Screen::Settings => 3,
        }
    }
}

/// View-model for the full grove TUI dashboard.
#[cfg(feature = "tui")]
#[derive(Default)]
pub struct DashboardState {
    pub screen: Screen,
    pub projects: Vec<String>,
    #[allow(dead_code)]
    pub selected_project: usize,
    pub conversations: Vec<String>,
    pub selected_conversation: usize,
    /// Tuples of `(id, objective, state)` for the runs list.
    pub runs: Vec<(String, String, String)>,
    pub changed_files: Vec<String>,
    pub branch: Option<String>,
}

/// Run the full grove TUI dashboard.
#[cfg(feature = "tui")]
pub fn run(transport: GroveTransport) -> crate::error::CliResult<()> {
    // Initial data load.
    let mut state = DashboardState::default();

    let projects = transport.list_projects().unwrap_or_default();
    state.projects = projects
        .iter()
        .map(|p| p.name.clone().unwrap_or_else(|| p.root_path.clone()))
        .collect();

    let conversations = transport.list_conversations(50).unwrap_or_default();
    state.conversations = conversations
        .iter()
        .map(|c| c.title.clone().unwrap_or_else(|| c.id.clone()))
        .collect();

    let runs = transport.list_runs(50).unwrap_or_default();
    state.runs = runs
        .iter()
        .map(|r| (r.id.clone(), r.objective.clone(), r.state.clone()))
        .collect();

    // Attempt to populate git metadata from the current directory.
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
    {
        if output.status.success() {
            state.branch = Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
    }
    if let Ok(output) = std::process::Command::new("git")
        .args(["status", "--short"])
        .output()
    {
        if output.status.success() {
            state.changed_files = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::to_string)
                .collect();
        }
    }

    enable_raw_mode().map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| crate::error::CliError::Other(e.to_string()))?;

    let result = (|| -> crate::error::CliResult<()> {
        loop {
            terminal
                .draw(|f| draw(f, &state))
                .map_err(|e| crate::error::CliError::Other(e.to_string()))?;

            if event::poll(Duration::from_millis(500)).unwrap_or(false) {
                if let Ok(Event::Key(key)) = event::read() {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('1') => state.screen = Screen::Dashboard,
                        KeyCode::Char('2') => state.screen = Screen::Sessions,
                        KeyCode::Char('3') => state.screen = Screen::Issues,
                        KeyCode::Char('4') => state.screen = Screen::Settings,
                        KeyCode::Tab => state.screen = state.screen.next(),
                        KeyCode::Up => {
                            if state.selected_conversation > 0 {
                                state.selected_conversation -= 1;
                            }
                        }
                        KeyCode::Down => {
                            let max = state.conversations.len().saturating_sub(1);
                            if state.selected_conversation < max {
                                state.selected_conversation += 1;
                            }
                        }
                        _ => {}
                    }
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

/// Render the dashboard for the current frame.
#[cfg(feature = "tui")]
pub fn draw(f: &mut Frame<'_>, state: &DashboardState) {
    let area = f.size();

    // Split the full area into main content + bottom nav bar.
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let main_area = layout[0];
    let nav_area = layout[1];

    // Bottom navigation bar.
    let nav_text = " [1] Dashboard  [2] Sessions  [3] Issues  [4] Settings    q: quit";
    let nav = Paragraph::new(nav_text).style(Style::default().fg(super::widgets::ACCENT));
    f.render_widget(nav, nav_area);

    // Render the active screen.
    match state.screen {
        Screen::Sessions => draw_sessions(f, state, main_area),
        Screen::Dashboard => {
            let para = Paragraph::new("Dashboard — coming soon")
                .block(super::widgets::titled_block(" Grove Dashboard "));
            f.render_widget(para, main_area);
        }
        Screen::Issues => {
            let para = Paragraph::new("Issues — use `grove issue board` for full kanban")
                .block(super::widgets::titled_block(" Issues "));
            f.render_widget(para, main_area);
        }
        Screen::Settings => {
            let para = Paragraph::new("Settings — use `grove auth list` and `grove llm list`")
                .block(super::widgets::titled_block(" Settings "));
            f.render_widget(para, main_area);
        }
    }
}

/// Render the Sessions screen: three-column layout.
#[cfg(feature = "tui")]
fn draw_sessions(f: &mut Frame<'_>, state: &DashboardState, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),
            Constraint::Min(0),
            Constraint::Length(28),
        ])
        .split(area);

    // Left: conversation list with selection highlight.
    let conv_items: Vec<ListItem> = state
        .conversations
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let style = if i == state.selected_conversation {
                Style::default().fg(Color::Black).bg(super::widgets::ACCENT)
            } else {
                Style::default()
            };
            ListItem::new(name.as_str()).style(style)
        })
        .collect();

    let conv_list = List::new(conv_items).block(super::widgets::titled_block(" Sessions "));
    f.render_widget(conv_list, cols[0]);

    // Middle: runs list showing first 8 chars of id + objective + state.
    let run_items: Vec<ListItem> = state
        .runs
        .iter()
        .map(|(id, objective, run_state)| {
            let short_id = if id.len() >= 8 { &id[..8] } else { id.as_str() };
            let color = super::widgets::state_color(run_state);
            let text = format!("{short_id}  {objective}  {run_state}");
            ListItem::new(text).style(Style::default().fg(color))
        })
        .collect();

    let runs_list = List::new(run_items).block(super::widgets::titled_block(" Runs "));
    f.render_widget(runs_list, cols[1]);

    // Right: git panel — changed files + branch in title.
    let branch_label = state.branch.as_deref().unwrap_or("(no branch)");
    let git_title = format!(" Git  [{branch_label}] ");
    let file_items: Vec<ListItem> = state
        .changed_files
        .iter()
        .map(|f| ListItem::new(f.as_str()))
        .collect();

    let git_block = Block::default()
        .title(git_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(super::widgets::ACCENT));
    let git_list = List::new(file_items).block(git_block);
    f.render_widget(git_list, cols[2]);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(feature = "tui")]
    fn dashboard_state_default_screen_is_sessions() {
        let s = super::DashboardState::default();
        assert_eq!(s.screen, super::Screen::Sessions);
    }

    #[test]
    #[cfg(feature = "tui")]
    fn screen_cycle_wraps_correctly() {
        assert_eq!(super::Screen::Sessions.next(), super::Screen::Issues);
        assert_eq!(super::Screen::Settings.next(), super::Screen::Dashboard);
    }
}
