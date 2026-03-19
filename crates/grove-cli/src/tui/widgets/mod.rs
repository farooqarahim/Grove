//! Shared TUI widget primitives and style helpers.

#[cfg(feature = "tui")]
use ratatui::{
    prelude::*,
    widgets::{Block, Borders},
};

/// Accent green matching grove-gui palette.
#[cfg(feature = "tui")]
pub const ACCENT: Color = Color::Rgb(49, 185, 123);

/// Returns a styled block with a title and accent-coloured borders.
#[cfg(feature = "tui")]
pub fn titled_block(title: &str) -> Block<'static> {
    Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
}

/// Maps a run/agent state string to a display colour.
#[cfg(feature = "tui")]
pub fn state_color(state: &str) -> Color {
    match state {
        "running" | "executing" | "planning" | "verifying" => Color::Green,
        "completed" => Color::Cyan,
        "failed" => Color::Red,
        "queued" | "created" => Color::Yellow,
        "paused" | "waiting_for_gate" => Color::Magenta,
        _ => Color::Gray,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(feature = "tui")]
    fn state_color_known_states() {
        use ratatui::prelude::Color;
        assert_eq!(super::state_color("running"), Color::Green);
        assert_eq!(super::state_color("completed"), Color::Cyan);
        assert_eq!(super::state_color("failed"), Color::Red);
        assert_eq!(super::state_color("queued"), Color::Yellow);
        assert_eq!(super::state_color("unknown_xyz"), Color::Gray);
    }
}
