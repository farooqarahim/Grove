use super::report_model::RunReport;

/// Render a `RunReport` as a Markdown string.
///
/// Produces sections for: run metadata, cost breakdown, session summaries,
/// and a chronological event timeline.
pub fn render_markdown(report: &RunReport) -> String {
    let mut out = String::new();

    // ── Header ──────────────────────────────────────────────────────────────
    out.push_str(&format!("# Run Report: {}\n\n", report.run_id));

    // ── Metadata ────────────────────────────────────────────────────────────
    out.push_str("## Overview\n\n");
    out.push_str("| Field | Value |\n|---|---|\n");
    out.push_str(&format!("| Objective | {} |\n", report.objective));
    out.push_str(&format!("| State | `{}` |\n", report.state));
    out.push_str(&format!("| Started | {} |\n", report.created_at));
    out.push('\n');

    // ── Sessions ────────────────────────────────────────────────────────────
    out.push_str("## Sessions\n\n");
    if report.sessions.is_empty() {
        out.push_str("_No sessions recorded._\n\n");
    } else {
        out.push_str("| Session ID | Agent | State | Started | Ended |\n|---|---|---|---|---|\n");
        for s in &report.sessions {
            let started = s.started_at.as_deref().unwrap_or("—");
            let ended = s.ended_at.as_deref().unwrap_or("—");
            out.push_str(&format!(
                "| `{}` | {} | `{}` | {} | {} |\n",
                s.id, s.agent_type, s.state, started, ended
            ));
        }
        out.push('\n');
    }

    // ── Event Timeline ───────────────────────────────────────────────────────
    out.push_str("## Event Timeline\n\n");
    if report.events.is_empty() {
        out.push_str("_No events recorded._\n\n");
    } else {
        for e in &report.events {
            let sess = e
                .session_id
                .as_deref()
                .map(|id| format!(" `{id}`"))
                .unwrap_or_default();
            out.push_str(&format!(
                "- `{}` **{}**{} — {}\n",
                e.created_at, e.event_type, sess, e.payload
            ));
        }
        out.push('\n');
    }

    out
}
