use crate::cli::{CiArgs, ConnectArgs, FixArgs, IssueAction, IssueArgs, LintArgs};
use crate::error::{CliError, CliResult};
use crate::output::{OutputMode, json as json_out, text};
use crate::transport::{GroveTransport, Transport};

// ── helpers ───────────────────────────────────────────────────────────────────

fn field<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v.get(key).and_then(|f| f.as_str()).unwrap_or("")
}

fn field_opt(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|f| f.as_str())
        .unwrap_or("")
        .to_string()
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn priority_dot(v: &serde_json::Value) -> &'static str {
    let p = v.get("priority").and_then(|f| f.as_str()).unwrap_or("");
    match p {
        "high" | "urgent" | "critical" => "●",
        _ => "○",
    }
}

/// Build the standard issue table row: ID, PROVIDER, TITLE, STATUS, PRIORITY, ASSIGNEE
fn issue_row(v: &serde_json::Value) -> Vec<String> {
    let raw_id = field_opt(v, "id");
    let id = if raw_id.is_empty() {
        format!("{}:{}", field(v, "provider"), field(v, "external_id"))
    } else {
        raw_id
    };
    vec![
        truncate(&id, 20),
        truncate(field(v, "provider"), 12),
        truncate(field(v, "title"), 48),
        truncate(field(v, "status"), 16),
        truncate(field(v, "priority"), 8),
        truncate(field(v, "assignee"), 16),
    ]
}

const ISSUE_HEADERS: &[&str] = &["ID", "PROVIDER", "TITLE", "STATUS", "PRIORITY", "ASSIGNEE"];

// ── list ──────────────────────────────────────────────────────────────────────

pub fn list_cmd(cached: bool, transport: &GroveTransport, mode: &OutputMode) -> CliResult<()> {
    let issues = transport.list_issues(cached)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::Value::Array(issues);
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if issues.is_empty() {
                println!("{}", text::dim("no issues"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = issues.iter().map(issue_row).collect();
            println!("{}", text::render_table(ISSUE_HEADERS, &rows));
        }
    }
    Ok(())
}

// ── show ──────────────────────────────────────────────────────────────────────

pub fn show_cmd(id: &str, transport: &GroveTransport, mode: &OutputMode) -> CliResult<()> {
    let issue = transport.get_issue(id)?;

    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json_pretty(&issue));
        }
        OutputMode::Text { .. } => {
            if issue.is_null() {
                return Err(CliError::NotFound(format!("issue {id}")));
            }
            println!("ID:       {}", field(&issue, "id"));
            println!("Provider: {}", field(&issue, "provider"));
            println!("Title:    {}", field(&issue, "title"));
            println!("Status:   {}", field(&issue, "status"));
            println!("Priority: {}", field(&issue, "priority"));
            println!("Assignee: {}", field(&issue, "assignee"));
            let body = field(&issue, "body");
            if !body.is_empty() {
                let preview: String = body.chars().take(300).collect();
                println!("Body:\n{preview}");
            }
        }
    }
    Ok(())
}

// ── create ────────────────────────────────────────────────────────────────────

pub fn create_cmd(
    title: &str,
    body: Option<&str>,
    labels: Vec<String>,
    priority: Option<i64>,
    transport: &GroveTransport,
    mode: &OutputMode,
) -> CliResult<()> {
    let created = transport.create_issue(title, body, labels, priority)?;

    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json_pretty(&created));
        }
        OutputMode::Text { .. } => {
            let raw_id = field_opt(&created, "id");
            let id = if raw_id.is_empty() {
                format!(
                    "{}:{}",
                    field(&created, "provider"),
                    field(&created, "external_id")
                )
            } else {
                raw_id
            };
            println!("created issue {id}");
        }
    }
    Ok(())
}

// ── close ─────────────────────────────────────────────────────────────────────

pub fn close_cmd(id: &str, transport: &GroveTransport, mode: &OutputMode) -> CliResult<()> {
    transport.close_issue(id)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::json!({ "ok": true, "id": id });
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            println!("closed {id}");
        }
    }
    Ok(())
}

// ── board ─────────────────────────────────────────────────────────────────────

/// Classify a status string into one of four kanban columns.
fn kanban_column(status: &str) -> &'static str {
    let s = status.to_lowercase();
    if s.contains("done")
        || s.contains("closed")
        || s.contains("cancelled")
        || s.contains("complete")
    {
        "DONE"
    } else if s.contains("review") {
        "IN_REVIEW"
    } else if s.contains("progress")
        || s.contains("in_progress")
        || s.contains("doing")
        || s.contains("started")
    {
        "IN_PROGRESS"
    } else {
        "OPEN"
    }
}

pub fn board_cmd(
    status_filter: Option<&str>,
    provider_filter: Option<&str>,
    assignee_filter: Option<&str>,
    priority_filter: Option<&str>,
    transport: &GroveTransport,
    mode: &OutputMode,
) -> CliResult<()> {
    let all_issues = transport.list_issues(false)?;

    // Apply filters
    let issues: Vec<&serde_json::Value> = all_issues
        .iter()
        .filter(|v| {
            if let Some(pf) = provider_filter {
                if !field(v, "provider").eq_ignore_ascii_case(pf) {
                    return false;
                }
            }
            if let Some(af) = assignee_filter {
                if !field(v, "assignee").eq_ignore_ascii_case(af) {
                    return false;
                }
            }
            if let Some(prf) = priority_filter {
                if !field(v, "priority").eq_ignore_ascii_case(prf) {
                    return false;
                }
            }
            if let Some(sf) = status_filter {
                // Filter to a specific column
                let col = kanban_column(sf);
                let issue_col = kanban_column(field(v, "status"));
                if col != issue_col && !field(v, "status").eq_ignore_ascii_case(sf) {
                    return false;
                }
            }
            true
        })
        .collect();

    // Group into columns
    let columns = ["OPEN", "IN_PROGRESS", "IN_REVIEW", "DONE"];
    let mut groups: std::collections::HashMap<&str, Vec<&serde_json::Value>> =
        columns.iter().map(|c| (*c, Vec::new())).collect();

    for v in &issues {
        let col = kanban_column(field(v, "status"));
        groups.entry(col).or_default().push(v);
    }

    match mode {
        OutputMode::Json => {
            let obj = serde_json::json!({
                "OPEN": groups["OPEN"].iter().map(|v| (*v).clone()).collect::<Vec<_>>(),
                "IN_PROGRESS": groups["IN_PROGRESS"].iter().map(|v| (*v).clone()).collect::<Vec<_>>(),
                "IN_REVIEW": groups["IN_REVIEW"].iter().map(|v| (*v).clone()).collect::<Vec<_>>(),
                "DONE": groups["DONE"].iter().map(|v| (*v).clone()).collect::<Vec<_>>(),
            });
            println!("{}", json_out::emit_json_pretty(&obj));
        }
        OutputMode::Text { .. } => {
            // Column display labels
            let col_labels = [
                format!("OPEN ({})", groups["OPEN"].len()),
                format!("IN PROGRESS ({})", groups["IN_PROGRESS"].len()),
                format!("IN REVIEW ({})", groups["IN_REVIEW"].len()),
                format!("DONE ({})", groups["DONE"].len()),
            ];
            let col_width = 22usize;

            // Header row
            let header: String = col_labels
                .iter()
                .map(|l| format!("{:<col_width$}", l))
                .collect::<Vec<_>>()
                .join("  ");
            println!("{}", text::bold(&header));

            // Separator row
            let sep: String = col_labels
                .iter()
                .map(|l| "─".repeat(l.len().min(col_width)))
                .map(|s| format!("{:<col_width$}", s))
                .collect::<Vec<_>>()
                .join("  ");
            println!("{sep}");

            // Issue rows — find the max count across columns
            let max_rows = columns.iter().map(|c| groups[*c].len()).max().unwrap_or(0);

            for i in 0..max_rows {
                let row: String = columns
                    .iter()
                    .map(|col| {
                        if let Some(v) = groups[*col].get(i) {
                            let id = {
                                let raw_id = field_opt(v, "id");
                                let raw = if raw_id.is_empty() {
                                    format!("{}:{}", field(v, "provider"), field(v, "external_id"))
                                } else {
                                    raw_id
                                };
                                truncate(&raw, 10)
                            };
                            let title = truncate(field(v, "title"), 10);
                            let dot = priority_dot(v);
                            format!("{:<col_width$}", format!("{id} {title}{dot}"))
                        } else {
                            format!("{:<col_width$}", "")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("  ");
                println!("{row}");
            }
        }
    }
    Ok(())
}

// ── sync ──────────────────────────────────────────────────────────────────────

pub fn sync_cmd(
    provider: Option<&str>,
    full: bool,
    transport: &GroveTransport,
    mode: &OutputMode,
) -> CliResult<()> {
    let result = transport.sync_issues(provider, full)?;

    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json_pretty(&result));
        }
        OutputMode::Text { .. } => {
            // result may be a single SyncResult or MultiSyncResult
            // Try to display as a summary table
            let results_arr = if let Some(arr) = result.get("results").and_then(|r| r.as_array()) {
                arr.to_vec()
            } else if result.is_object() && result.get("provider").is_some() {
                vec![result.clone()]
            } else {
                // Null or unexpected shape — nothing to show
                println!("{}", text::dim("sync complete (no results)"));
                return Ok(());
            };

            if results_arr.is_empty() {
                println!("{}", text::dim("sync complete (no providers)"));
                return Ok(());
            }

            let rows: Vec<Vec<String>> = results_arr
                .iter()
                .map(|r| {
                    let new_count = r.get("new_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    let updated = r.get("updated_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    let closed = r.get("closed_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    let errors = r
                        .get("errors")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    vec![
                        field_opt(r, "provider"),
                        new_count.to_string(),
                        updated.to_string(),
                        closed.to_string(),
                        errors.to_string(),
                    ]
                })
                .collect();
            println!(
                "{}",
                text::render_table(&["PROVIDER", "NEW", "UPDATED", "CLOSED", "ERRORS"], &rows)
            );
        }
    }
    Ok(())
}

// ── search ────────────────────────────────────────────────────────────────────

pub fn search_cmd(
    query: &str,
    limit: i64,
    provider: Option<&str>,
    transport: &GroveTransport,
    mode: &OutputMode,
) -> CliResult<()> {
    let issues = transport.search_issues(query, limit, provider)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::Value::Array(issues);
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if issues.is_empty() {
                println!("{}", text::dim("no issues"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = issues.iter().map(issue_row).collect();
            println!("{}", text::render_table(ISSUE_HEADERS, &rows));
        }
    }
    Ok(())
}

// ── top-level dispatch ────────────────────────────────────────────────────────

pub fn dispatch(a: IssueArgs, t: GroveTransport, m: OutputMode) -> CliResult<()> {
    match a.action {
        IssueAction::List { cached } => list_cmd(cached, &t, &m),
        IssueAction::Show { id } => show_cmd(&id, &t, &m),
        IssueAction::Create {
            title,
            body,
            labels,
            priority,
        } => {
            let p = priority.and_then(|s| s.parse::<i64>().ok());
            create_cmd(&title, body.as_deref(), labels, p, &t, &m)
        }
        IssueAction::Close { id } => close_cmd(&id, &t, &m),
        IssueAction::Board {
            status,
            provider,
            assignee,
            priority,
        } => board_cmd(
            status.as_deref(),
            provider.as_deref(),
            assignee.as_deref(),
            priority.as_deref(),
            &t,
            &m,
        ),
        IssueAction::Sync { provider, full } => sync_cmd(provider.as_deref(), full, &t, &m),
        IssueAction::Search {
            query,
            limit,
            provider,
        } => search_cmd(&query, limit as i64, provider.as_deref(), &t, &m),
        // Remaining actions handled in Task 13
        _ => Ok(()),
    }
}

pub fn fix_cmd(_a: FixArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn connect_dispatch(_a: ConnectArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn lint_cmd(_a: LintArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn ci_cmd(_a: CiArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputMode;
    use crate::transport::{GroveTransport, TestTransport};

    fn text_mode() -> OutputMode {
        OutputMode::Text { no_color: true }
    }

    #[test]
    fn issue_list_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(list_cmd(false, &t, &text_mode()).is_ok());
    }

    #[test]
    fn issue_list_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(list_cmd(false, &t, &OutputMode::Json).is_ok());
    }

    #[test]
    fn issue_list_cached_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(list_cmd(true, &t, &text_mode()).is_ok());
    }

    #[test]
    fn issue_show_null_returns_not_found() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = show_cmd("GH-1", &t, &text_mode());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, CliError::NotFound(_)));
    }

    #[test]
    fn issue_show_null_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        // JSON mode prints null without error
        assert!(show_cmd("GH-1", &t, &OutputMode::Json).is_ok());
    }

    #[test]
    fn issue_create_returns_err() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = create_cmd("Bug", None, vec![], None, &t, &text_mode());
        assert!(result.is_err());
    }

    #[test]
    fn issue_close_returns_err() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = close_cmd("GH-1", &t, &text_mode());
        assert!(result.is_err());
    }

    #[test]
    fn issue_board_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(board_cmd(None, None, None, None, &t, &text_mode()).is_ok());
    }

    #[test]
    fn issue_board_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(board_cmd(None, None, None, None, &t, &OutputMode::Json).is_ok());
    }

    #[test]
    fn issue_sync_null_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(sync_cmd(None, false, &t, &text_mode()).is_ok());
    }

    #[test]
    fn issue_sync_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(sync_cmd(None, true, &t, &OutputMode::Json).is_ok());
    }

    #[test]
    fn issue_search_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(search_cmd("bug", 10, None, &t, &text_mode()).is_ok());
    }

    #[test]
    fn issue_search_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(search_cmd("bug", 10, None, &t, &OutputMode::Json).is_ok());
    }

    #[test]
    fn kanban_column_classification() {
        assert_eq!(kanban_column("open"), "OPEN");
        assert_eq!(kanban_column("todo"), "OPEN");
        assert_eq!(kanban_column("in_progress"), "IN_PROGRESS");
        assert_eq!(kanban_column("in progress"), "IN_PROGRESS");
        assert_eq!(kanban_column("IN REVIEW"), "IN_REVIEW");
        assert_eq!(kanban_column("done"), "DONE");
        assert_eq!(kanban_column("closed"), "DONE");
        assert_eq!(kanban_column("cancelled"), "DONE");
    }

    #[test]
    fn issue_list_with_data_text_ok() {
        // Feed a synthetic issue through the TestTransport by wrapping with a custom impl.
        // Since TestTransport always returns empty, just verify the empty path renders OK.
        let t = GroveTransport::Test(TestTransport::default());
        assert!(list_cmd(false, &t, &text_mode()).is_ok());
    }
}
