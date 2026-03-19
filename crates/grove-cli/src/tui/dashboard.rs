//! Top-level `grove tui` dashboard entry point.
//!
//! Task 17 will implement the full multi-tab dashboard. This module provides
//! the entry-point function and a minimal placeholder screen so that `grove tui`
//! is already functional with the TUI feature enabled.

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
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

#[cfg(feature = "tui")]
use crate::transport::{GroveTransport, Transport};

/// Run the full grove TUI dashboard.
#[cfg(feature = "tui")]
pub fn run(transport: GroveTransport) -> crate::error::CliResult<()> {
    enable_raw_mode().map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| crate::error::CliError::Other(e.to_string()))?;

    let result = (|| -> crate::error::CliResult<()> {
        loop {
            let runs = transport.list_runs(20).unwrap_or_default();

            terminal
                .draw(|f| {
                    let area = f.size();

                    let outer = Block::default()
                        .title(" grove dashboard ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(super::widgets::ACCENT));
                    let inner = outer.inner(area);
                    f.render_widget(outer, area);

                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(1), Constraint::Min(0)])
                        .split(inner);

                    let hint = Paragraph::new("  Press q or Esc to quit")
                        .style(Style::default().fg(Color::DarkGray));
                    f.render_widget(hint, chunks[0]);

                    let items: Vec<ListItem> = runs
                        .iter()
                        .map(|r| {
                            let color = super::widgets::state_color(&r.state);
                            ListItem::new(format!(" {:>20}  {}  {}", r.id, r.state, r.objective))
                                .style(Style::default().fg(color))
                        })
                        .collect();

                    let list = List::new(items).block(super::widgets::titled_block("Recent Runs"));
                    f.render_widget(list, chunks[1]);
                })
                .map_err(|e| crate::error::CliError::Other(e.to_string()))?;

            if event::poll(Duration::from_millis(500)).unwrap_or(false) {
                if let Ok(Event::Key(key)) = event::read() {
                    if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                        break;
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
