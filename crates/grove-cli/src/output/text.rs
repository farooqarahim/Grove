#![allow(dead_code)] // Public API used by CLI commands (Tasks 6+)

use console::Style;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use tabled::builder::Builder;

pub fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut b = Builder::default();
    b.push_record(headers.iter().copied());
    for row in rows {
        b.push_record(row.iter().map(String::as_str));
    }
    b.build().to_string()
}

pub fn success(msg: &str) {
    println!("{}", Style::new().green().apply_to(msg));
}

pub fn error_line(msg: &str) {
    eprintln!("{}", Style::new().red().apply_to(msg));
}

pub fn dim(msg: &str) -> String {
    Style::new().dim().apply_to(msg).to_string()
}

pub fn bold(msg: &str) -> String {
    Style::new().bold().apply_to(msg).to_string()
}

/// Create and start a spinner. Call `.finish_and_clear()` when done.
pub fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_includes_headers_and_data() {
        let rows = vec![vec![
            "abc12345".to_string(),
            "Add OAuth".to_string(),
            "running".to_string(),
        ]];
        let out = render_table(&["ID", "OBJECTIVE", "STATE"], &rows);
        assert!(out.contains("ID"));
        assert!(out.contains("Add OAuth"));
    }

    #[test]
    fn dim_returns_non_empty_string() {
        assert!(!dim("hello").is_empty());
    }
}
