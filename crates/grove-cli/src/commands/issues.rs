use crate::cli::{
    BoardConfigAction, CiArgs, ConnectAction, ConnectArgs, FixArgs, IssueAction, IssueArgs,
    LintArgs,
};
use crate::error::{CliError, CliResult};
use crate::output::{OutputMode, json as json_out, text};
use crate::transport::{GroveTransport, Transport};

// ── helpers ───────────────────────────────────────────────────────────────────

fn field<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v.get(key).and_then(|f| f.as_str()).unwrap_or("")
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
    let id = if field(v, "id").is_empty() {
        format!("{}:{}", field(v, "provider"), field(v, "external_id"))
    } else {
        field(v, "id").to_owned()
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
            let id = if field(&created, "id").is_empty() {
                format!(
                    "{}:{}",
                    field(&created, "provider"),
                    field(&created, "external_id")
                )
            } else {
                field(&created, "id").to_owned()
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
                if col != issue_col {
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
            let empty: Vec<&serde_json::Value> = Vec::new();
            let obj = serde_json::json!({
                "OPEN": groups.get("OPEN").unwrap_or(&empty).iter().map(|v| (*v).clone()).collect::<Vec<_>>(),
                "IN_PROGRESS": groups.get("IN_PROGRESS").unwrap_or(&empty).iter().map(|v| (*v).clone()).collect::<Vec<_>>(),
                "IN_REVIEW": groups.get("IN_REVIEW").unwrap_or(&empty).iter().map(|v| (*v).clone()).collect::<Vec<_>>(),
                "DONE": groups.get("DONE").unwrap_or(&empty).iter().map(|v| (*v).clone()).collect::<Vec<_>>(),
            });
            println!("{}", json_out::emit_json_pretty(&obj));
        }
        OutputMode::Text { .. } => {
            // Column display labels
            let col_labels = [
                format!("OPEN ({})", groups.get("OPEN").map_or(0, |v| v.len())),
                format!(
                    "IN PROGRESS ({})",
                    groups.get("IN_PROGRESS").map_or(0, |v| v.len())
                ),
                format!(
                    "IN REVIEW ({})",
                    groups.get("IN_REVIEW").map_or(0, |v| v.len())
                ),
                format!("DONE ({})", groups.get("DONE").map_or(0, |v| v.len())),
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
            let max_rows = columns
                .iter()
                .map(|c| groups.get(*c).map_or(0, |v| v.len()))
                .max()
                .unwrap_or(0);

            for i in 0..max_rows {
                let row: String = columns
                    .iter()
                    .map(|col| {
                        let col_issues = groups.get(*col).map(|v| v.as_slice()).unwrap_or(&[]);
                        if let Some(v) = col_issues.get(i) {
                            let id = {
                                let raw = if field(v, "id").is_empty() {
                                    format!("{}:{}", field(v, "provider"), field(v, "external_id"))
                                } else {
                                    field(v, "id").to_owned()
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
                        field(r, "provider").to_owned(),
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

// ── update ────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn update_cmd(
    id: &str,
    title: Option<&str>,
    status: Option<&str>,
    label: Option<&str>,
    assignee: Option<&str>,
    priority: Option<&str>,
    transport: &GroveTransport,
    mode: &OutputMode,
) -> CliResult<()> {
    let updated = transport.update_issue(id, title, status, label, assignee, priority)?;

    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json_pretty(&updated));
        }
        OutputMode::Text { .. } => {
            println!("updated {id}");
        }
    }
    Ok(())
}

// ── comment ───────────────────────────────────────────────────────────────────

pub fn comment_cmd(
    id: &str,
    body: &str,
    transport: &GroveTransport,
    mode: &OutputMode,
) -> CliResult<()> {
    let result = transport.comment_issue(id, body)?;

    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json_pretty(&result));
        }
        OutputMode::Text { .. } => {
            println!("commented on {id}");
        }
    }
    Ok(())
}

// ── assign ────────────────────────────────────────────────────────────────────

pub fn assign_cmd(
    id: &str,
    assignee: &str,
    transport: &GroveTransport,
    mode: &OutputMode,
) -> CliResult<()> {
    transport.assign_issue(id, assignee)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::json!({ "ok": true, "id": id, "assignee": assignee });
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            println!("assigned {id} to {assignee}");
        }
    }
    Ok(())
}

// ── move ──────────────────────────────────────────────────────────────────────

pub fn move_cmd(
    id: &str,
    status: &str,
    transport: &GroveTransport,
    mode: &OutputMode,
) -> CliResult<()> {
    transport.move_issue(id, status)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::json!({ "ok": true, "id": id, "status": status });
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            println!("moved {id} to {status}");
        }
    }
    Ok(())
}

// ── reopen ────────────────────────────────────────────────────────────────────

pub fn reopen_cmd(id: &str, transport: &GroveTransport, mode: &OutputMode) -> CliResult<()> {
    transport.reopen_issue(id)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::json!({ "ok": true, "id": id });
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            println!("reopened {id}");
        }
    }
    Ok(())
}

// ── activity ──────────────────────────────────────────────────────────────────

pub fn activity_cmd(id: &str, transport: &GroveTransport, mode: &OutputMode) -> CliResult<()> {
    let activities = transport.activity_issue(id)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::Value::Array(activities);
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if activities.is_empty() {
                println!("{}", text::dim("no activity"));
                return Ok(());
            }
            for entry in &activities {
                let actor = field(entry, "actor");
                let kind = field(entry, "type");
                let body = field(entry, "body");
                let ts = field(entry, "created_at");
                println!("{ts}  {actor}  {kind}  {body}");
            }
        }
    }
    Ok(())
}

// ── push ──────────────────────────────────────────────────────────────────────

pub fn push_cmd(
    id: &str,
    provider: &str,
    transport: &GroveTransport,
    mode: &OutputMode,
) -> CliResult<()> {
    let result = transport.push_issue(id, provider)?;

    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json_pretty(&result));
        }
        OutputMode::Text { .. } => {
            println!("pushed {id} to {provider}");
        }
    }
    Ok(())
}

// ── ready ─────────────────────────────────────────────────────────────────────

pub fn ready_cmd(id: &str, transport: &GroveTransport, mode: &OutputMode) -> CliResult<()> {
    let result = transport.issue_ready(id)?;

    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json_pretty(&result));
        }
        OutputMode::Text { .. } => {
            println!("marked {id} as ready");
        }
    }
    Ok(())
}

// ── board-config ──────────────────────────────────────────────────────────────

pub fn board_config_cmd(action: BoardConfigAction, mode: &OutputMode) -> CliResult<()> {
    match action {
        BoardConfigAction::Show => match mode {
            OutputMode::Json => {
                let val = serde_json::json!({ "config": null });
                println!("{}", json_out::emit_json(&val));
            }
            OutputMode::Text { .. } => {
                println!("{}", text::dim("no board config set"));
            }
        },
        BoardConfigAction::Set { file } => match mode {
            OutputMode::Json => {
                let val = serde_json::json!({ "ok": true, "file": file });
                println!("{}", json_out::emit_json(&val));
            }
            OutputMode::Text { .. } => {
                println!("board config set from {file}");
            }
        },
        BoardConfigAction::Reset => match mode {
            OutputMode::Json => {
                let val = serde_json::json!({ "ok": true });
                println!("{}", json_out::emit_json(&val));
            }
            OutputMode::Text { .. } => {
                println!("board config reset");
            }
        },
    }
    Ok(())
}

// ── connect helpers ───────────────────────────────────────────────────────────

pub fn connect_status_cmd(transport: &GroveTransport, mode: &OutputMode) -> CliResult<()> {
    let statuses = transport.connect_status()?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::Value::Array(statuses);
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if statuses.is_empty() {
                println!("{}", text::dim("no providers connected"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = statuses
                .iter()
                .map(|v| {
                    vec![
                        truncate(field(v, "provider"), 16),
                        truncate(field(v, "connected"), 12),
                        truncate(field(v, "user"), 24),
                        truncate(field(v, "error"), 32),
                    ]
                })
                .collect();
            println!(
                "{}",
                text::render_table(&["PROVIDER", "CONNECTED", "USER", "ERROR"], &rows)
            );
        }
    }
    Ok(())
}

pub fn connect_provider_cmd(
    provider: &str,
    token: Option<&str>,
    site: Option<&str>,
    email: Option<&str>,
    transport: &GroveTransport,
    mode: &OutputMode,
) -> CliResult<()> {
    transport.connect_provider(provider, token, site, email)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::json!({ "ok": true, "provider": provider });
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            println!("connected {provider}");
        }
    }
    Ok(())
}

pub fn connect_disconnect_cmd(
    provider: &str,
    transport: &GroveTransport,
    mode: &OutputMode,
) -> CliResult<()> {
    transport.disconnect_provider(provider)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::json!({ "ok": true, "provider": provider });
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            println!("disconnected {provider}");
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
        } => search_cmd(&query, i64::from(limit), provider.as_deref(), &t, &m),
        IssueAction::Update {
            id,
            title,
            status,
            label,
            assignee,
            priority,
        } => {
            let lbl = label.first().map(|s| s.as_str());
            update_cmd(
                &id,
                title.as_deref(),
                status.as_deref(),
                lbl,
                assignee.as_deref(),
                priority.as_deref(),
                &t,
                &m,
            )
        }
        IssueAction::Comment { id, body } => comment_cmd(&id, &body, &t, &m),
        IssueAction::Assign { id, assignee } => assign_cmd(&id, &assignee, &t, &m),
        IssueAction::Move { id, status } => move_cmd(&id, &status, &t, &m),
        IssueAction::Reopen { id } => reopen_cmd(&id, &t, &m),
        IssueAction::Activity { id } => activity_cmd(&id, &t, &m),
        IssueAction::Push { id, to } => push_cmd(&id, &to, &t, &m),
        IssueAction::Ready => {
            // `grove issue ready` marks the current issue as ready; id is resolved server-side
            ready_cmd("current", &t, &m)
        }
        IssueAction::BoardConfig { action } => board_config_cmd(action, &m),
    }
}

pub fn fix_cmd(a: FixArgs, t: GroveTransport, m: OutputMode) -> CliResult<()> {
    use crate::transport::StartRunRequest;

    let issue_id = a.issue_id.clone();
    let objective = if let Some(prompt) = a.prompt.as_deref() {
        prompt.to_owned()
    } else if let Some(ref id) = issue_id {
        format!("fix issue {id}")
    } else {
        "fix current issue".to_owned()
    };

    let result = t.start_run(StartRunRequest {
        objective,
        pipeline: None,
        model: None,
        permission_mode: None,
        conversation_id: None,
        continue_last: false,
        issue_id,
        max_agents: a.max.map(|n| n as u16),
    })?;

    match m {
        OutputMode::Json => {
            let val = serde_json::json!({
                "run_id": result.run_id,
                "state": result.state,
                "objective": result.objective,
            });
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            println!("started fix run {} ({})", result.run_id, result.state);
        }
    }
    Ok(())
}

pub fn connect_dispatch(a: ConnectArgs, t: GroveTransport, m: OutputMode) -> CliResult<()> {
    match a.action {
        ConnectAction::Status => connect_status_cmd(&t, &m),
        ConnectAction::Github { token } => {
            connect_provider_cmd("github", token.as_deref(), None, None, &t, &m)
        }
        ConnectAction::Jira { site, email, token } => {
            connect_provider_cmd("jira", Some(&token), Some(&site), Some(&email), &t, &m)
        }
        ConnectAction::Linear { token } => {
            connect_provider_cmd("linear", Some(&token), None, None, &t, &m)
        }
        ConnectAction::Disconnect { provider } => connect_disconnect_cmd(&provider, &t, &m),
    }
}

pub fn lint_cmd(a: LintArgs, t: GroveTransport, m: OutputMode) -> CliResult<()> {
    let result = t.run_lint(a.fix, a.model.as_deref())?;

    match m {
        OutputMode::Json => {
            println!("{}", json_out::emit_json_pretty(&result));
        }
        OutputMode::Text { .. } => {
            let status = result
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let issues = result.get("issues").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("lint: {status} ({issues} issues)");
        }
    }
    Ok(())
}

pub fn ci_cmd(a: CiArgs, t: GroveTransport, m: OutputMode) -> CliResult<()> {
    let result = t.run_ci(
        a.branch.as_deref(),
        a.wait,
        a.timeout,
        a.fix,
        a.model.as_deref(),
    )?;

    match m {
        OutputMode::Json => {
            println!("{}", json_out::emit_json_pretty(&result));
        }
        OutputMode::Text { .. } => {
            let status = result
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let branch = result
                .get("branch")
                .and_then(|v| v.as_str())
                .unwrap_or("(unknown)");
            println!("ci: {status} on {branch}");
        }
    }
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
    fn issue_board_with_status_filter_ok() {
        // Verify board_cmd with a status filter does not panic even when groups are empty.
        let t = GroveTransport::Test(TestTransport::default());
        assert!(board_cmd(Some("open"), None, None, None, &t, &text_mode()).is_ok());
    }

    // ── Task 13 tests ─────────────────────────────────────────────────────────

    #[test]
    fn connect_status_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(connect_status_cmd(&t, &OutputMode::Text { no_color: true }).is_ok());
    }

    #[test]
    fn connect_status_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(connect_status_cmd(&t, &OutputMode::Json).is_ok());
    }

    #[test]
    fn issue_update_returns_err() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = update_cmd(
            "GH-1",
            Some("new title"),
            None,
            None,
            None,
            None,
            &t,
            &text_mode(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn issue_comment_returns_err() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = comment_cmd("GH-1", "looks good", &t, &text_mode());
        assert!(result.is_err());
    }

    #[test]
    fn issue_assign_returns_err() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = assign_cmd("GH-1", "alice", &t, &text_mode());
        assert!(result.is_err());
    }

    #[test]
    fn issue_move_returns_err() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = move_cmd("GH-1", "in_progress", &t, &text_mode());
        assert!(result.is_err());
    }

    #[test]
    fn issue_reopen_returns_err() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = reopen_cmd("GH-1", &t, &text_mode());
        assert!(result.is_err());
    }

    #[test]
    fn issue_activity_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(activity_cmd("GH-1", &t, &text_mode()).is_ok());
    }

    #[test]
    fn issue_activity_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(activity_cmd("GH-1", &t, &OutputMode::Json).is_ok());
    }

    #[test]
    fn issue_push_returns_err() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = push_cmd("GH-1", "linear", &t, &text_mode());
        assert!(result.is_err());
    }

    #[test]
    fn issue_ready_returns_err() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = ready_cmd("GH-1", &t, &text_mode());
        assert!(result.is_err());
    }

    #[test]
    fn board_config_show_ok() {
        assert!(board_config_cmd(BoardConfigAction::Show, &text_mode()).is_ok());
    }

    #[test]
    fn board_config_show_json_ok() {
        assert!(board_config_cmd(BoardConfigAction::Show, &OutputMode::Json).is_ok());
    }

    #[test]
    fn board_config_set_ok() {
        assert!(
            board_config_cmd(
                BoardConfigAction::Set {
                    file: "board.toml".into()
                },
                &text_mode()
            )
            .is_ok()
        );
    }

    #[test]
    fn board_config_reset_ok() {
        assert!(board_config_cmd(BoardConfigAction::Reset, &text_mode()).is_ok());
    }

    #[test]
    fn connect_provider_returns_err() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = connect_provider_cmd("github", Some("tok"), None, None, &t, &text_mode());
        assert!(result.is_err());
    }

    #[test]
    fn connect_disconnect_returns_err() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = connect_disconnect_cmd("github", &t, &text_mode());
        assert!(result.is_err());
    }

    #[test]
    fn fix_cmd_returns_err_when_run_fails() {
        use crate::cli::FixArgs;
        let t = GroveTransport::Test(TestTransport::default());
        let a = FixArgs {
            issue_id: Some("GH-1".into()),
            prompt: None,
            ready: false,
            max: None,
            parallel: false,
        };
        // TestTransport::start_run returns Err
        assert!(fix_cmd(a, t, text_mode()).is_err());
    }

    #[test]
    fn lint_cmd_returns_err() {
        use crate::cli::LintArgs;
        let t = GroveTransport::Test(TestTransport::default());
        let a = LintArgs {
            fix: false,
            model: None,
        };
        assert!(lint_cmd(a, t, text_mode()).is_err());
    }

    #[test]
    fn ci_cmd_returns_err() {
        use crate::cli::CiArgs;
        let t = GroveTransport::Test(TestTransport::default());
        let a = CiArgs {
            branch: None,
            wait: false,
            timeout: None,
            fix: false,
            model: None,
        };
        assert!(ci_cmd(a, t, text_mode()).is_err());
    }
}
