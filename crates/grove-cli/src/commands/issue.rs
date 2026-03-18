use std::fs;

use anyhow::Result;
use grove_core::db::repositories::issues_repo::{self, IssueFilter};
use grove_core::db::repositories::projects_repo::IssueBoardConfig;
use grove_core::tracker::status::CanonicalStatus;
use grove_core::tracker::{self, IssueUpdate};
use serde_json::json;

use crate::cli::IssueArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &IssueArgs) -> Result<CommandOutput> {
    match &args.action {
        crate::cli::IssueAction::List(a) => handle_list(ctx, a),
        crate::cli::IssueAction::Show(a) => handle_show(ctx, a),
        crate::cli::IssueAction::Create(a) => handle_create(ctx, a),
        crate::cli::IssueAction::Close(a) => handle_close(ctx, a),
        crate::cli::IssueAction::Ready => handle_ready(ctx),
        crate::cli::IssueAction::Sync(a) => handle_sync(ctx, a),
        crate::cli::IssueAction::Board(a) => handle_board(ctx, a),
        crate::cli::IssueAction::BoardConfig(a) => handle_board_config(ctx, a),
        crate::cli::IssueAction::Search(a) => handle_search(ctx, a),
        crate::cli::IssueAction::Update(a) => handle_update(ctx, a),
        crate::cli::IssueAction::Comment(a) => handle_comment(ctx, a),
        crate::cli::IssueAction::Assign(a) => handle_assign(ctx, a),
        crate::cli::IssueAction::Move(a) => handle_move(ctx, a),
        crate::cli::IssueAction::Reopen(a) => handle_reopen(ctx, a),
        crate::cli::IssueAction::Push(a) => handle_push(ctx, a),
        crate::cli::IssueAction::Activity(a) => handle_activity(ctx, a),
        crate::cli::IssueAction::Lint(a) => handle_lint(ctx, a),
    }
}

fn project_id_for(ctx: &CommandContext) -> String {
    grove_core::orchestrator::conversation::derive_project_id(&ctx.project_root)
}

fn db_connect(ctx: &CommandContext) -> Result<rusqlite::Connection> {
    let handle = grove_core::db::DbHandle::new(&ctx.project_root);
    Ok(handle.connect()?)
}

fn current_project_settings(
    ctx: &CommandContext,
) -> Result<grove_core::db::repositories::projects_repo::ProjectSettings> {
    let row = grove_core::orchestrator::get_project(&ctx.project_root)?;
    Ok(grove_core::orchestrator::get_project_settings(
        &ctx.project_root,
        &row.id,
    )?)
}

fn save_current_project_settings(
    ctx: &CommandContext,
    settings: &grove_core::db::repositories::projects_repo::ProjectSettings,
) -> Result<()> {
    let row = grove_core::orchestrator::get_project(&ctx.project_root)?;
    Ok(grove_core::orchestrator::update_project_settings(
        &ctx.project_root,
        &row.id,
        settings,
    )?)
}

// ── Existing commands (updated to use issues table) ───────────────────────────

fn handle_list(ctx: &CommandContext, args: &crate::cli::IssueListArgs) -> Result<CommandOutput> {
    let project_id = project_id_for(ctx);
    if args.cached {
        let conn = db_connect(ctx)?;
        let issues = issues_repo::list(&conn, &project_id, &IssueFilter::new())?;
        let json_val = json!({ "issues": issues, "source": "cache" });
        let mut text = format!("Cached issues: {}", issues.len());
        for issue in &issues {
            text.push_str(&format!(
                "\n  #{} [{}] {}",
                issue.external_id, issue.status, issue.title
            ));
        }
        return Ok(to_text_or_json(ctx.format, text, json_val));
    }

    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;
    let backend = tracker::build_backend(&cfg, &ctx.project_root)?;
    let issues = backend.list()?;

    let conn = db_connect(ctx)?;
    for issue in &issues {
        let _ = issues_repo::upsert(&conn, issue, &project_id);
    }

    let json_val = json!({ "issues": issues, "source": "remote" });
    let mut text = format!("Issues: {}", issues.len());
    for issue in &issues {
        text.push_str(&format!(
            "\n  #{} [{}] {}",
            issue.external_id, issue.status, issue.title
        ));
    }
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_show(ctx: &CommandContext, args: &crate::cli::IssueShowArgs) -> Result<CommandOutput> {
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;
    let backend = tracker::build_backend(&cfg, &ctx.project_root)?;
    let issue = backend.show(&args.id)?;

    let conn = db_connect(ctx)?;
    let _ = issues_repo::upsert(&conn, &issue, &project_id_for(ctx));

    let json_val = json!(issue);
    let mut text = format!(
        "#{} [{}] {}\n",
        issue.external_id, issue.status, issue.title
    );
    if let Some(ref body) = issue.body {
        text.push_str(body);
    }
    if !issue.labels.is_empty() {
        text.push_str(&format!("\nLabels: {}", issue.labels.join(", ")));
    }
    if let Some(native_id) = &issue.provider_native_id {
        text.push_str(&format!("\nProvider Native ID: {native_id}"));
    }
    if let Some(scope_key) = &issue.provider_scope_key {
        let scope_type = issue.provider_scope_type.as_deref().unwrap_or("scope");
        let scope_name = issue.provider_scope_name.as_deref().unwrap_or(scope_key);
        text.push_str(&format!("\nScope: {scope_type} {scope_name} ({scope_key})"));
    }
    if issue.provider_metadata != serde_json::json!({}) {
        text.push_str(&format!(
            "\nMetadata: {}",
            serde_json::to_string_pretty(&issue.provider_metadata)?
        ));
    }
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_create(
    ctx: &CommandContext,
    args: &crate::cli::IssueCreateArgs,
) -> Result<CommandOutput> {
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;
    let backend = tracker::build_backend(&cfg, &ctx.project_root)?;
    let body = args.body.as_deref().unwrap_or("");
    let issue = backend.create(&args.title, body)?;

    let conn = db_connect(ctx)?;
    let _ = issues_repo::upsert(&conn, &issue, &project_id_for(ctx));

    let text = format!("Created issue #{}: {}", issue.external_id, issue.title);
    let json_val = json!(issue);
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_close(ctx: &CommandContext, args: &crate::cli::IssueCloseArgs) -> Result<CommandOutput> {
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;
    let backend = tracker::build_backend(&cfg, &ctx.project_root)?;
    backend.close(&args.id)?;
    let text = format!("Closed issue #{}", args.id);
    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({ "closed": args.id }),
    ))
}

fn handle_ready(ctx: &CommandContext) -> Result<CommandOutput> {
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;
    let backend = tracker::build_backend(&cfg, &ctx.project_root)?;
    let issues = backend.ready()?;
    let json_val = json!({ "ready_issues": issues });
    let mut text = format!("Ready issues: {}", issues.len());
    for issue in &issues {
        text.push_str(&format!(
            "\n  #{} [{}] {}",
            issue.external_id, issue.status, issue.title
        ));
    }
    Ok(to_text_or_json(ctx.format, text, json_val))
}

// ── New commands ──────────────────────────────────────────────────────────────

fn handle_sync(ctx: &CommandContext, args: &crate::cli::IssueSyncArgs) -> Result<CommandOutput> {
    let project_id = project_id_for(ctx);
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;
    let handle = grove_core::db::DbHandle::new(&ctx.project_root);
    let mut conn = handle.connect()?;
    let incremental = !args.full;
    let debounce = cfg.tracker.sync.debounce_secs;

    let results = if let Some(provider_name) = &args.provider {
        // Sync a single provider.
        let backend = tracker::build_backend(&cfg, &ctx.project_root)?;
        if backend.provider_name() != provider_name.as_str() {
            anyhow::bail!("provider '{}' not configured in grove.yaml", provider_name);
        }
        vec![grove_core::tracker::sync::sync_provider(
            &mut conn,
            backend.as_ref(),
            &project_id,
            incremental,
            debounce,
        )]
    } else {
        let mr = grove_core::tracker::sync::sync_all(
            &mut conn,
            &cfg,
            &ctx.project_root,
            &project_id,
            incremental,
        );
        mr.results
    };

    let mut text_parts = Vec::new();
    for r in &results {
        let status = if r.errors.is_empty() { "ok" } else { "errors" };
        text_parts.push(format!(
            "{}: +{} new, {} updated, {} closed [{}] ({}ms)",
            r.provider, r.new_count, r.updated_count, r.closed_count, status, r.duration_ms
        ));
        for e in &r.errors {
            text_parts.push(format!("  error: {e}"));
        }
    }
    let text = text_parts.join("\n");
    let json_val = json!({ "sync_results": results });
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_board(ctx: &CommandContext, args: &crate::cli::IssueBoardArgs) -> Result<CommandOutput> {
    let project_id = project_id_for(ctx);
    let conn = db_connect(ctx)?;

    let canonical_status = args
        .status
        .as_deref()
        .and_then(|s| CanonicalStatus::from_db_str(s));

    let filter = IssueFilter {
        provider: args.provider.clone(),
        canonical_status,
        assignee: args.assignee.clone(),
        priority: args.priority.clone(),
        limit: 500,
        ..Default::default()
    };

    let board = issues_repo::board_view(&conn, &project_id, &filter)?;
    let open_count = issues_repo::count_open(&conn, &project_id)?;

    // Text-mode kanban render.
    let col_width = 22usize;
    let mut text = format!("Issue Board — {} active issue(s)\n", open_count);
    text.push_str(&"─".repeat(col_width * board.columns.len() + board.columns.len() + 1));
    text.push('\n');

    // Column headers.
    let mut header = String::from("|");
    for col in &board.columns {
        let label = format!("{} ({})", col.label, col.count);
        header.push_str(&format!(" {:<width$}|", label, width = col_width - 2));
    }
    text.push_str(&header);
    text.push('\n');
    text.push_str(&"─".repeat(col_width * board.columns.len() + board.columns.len() + 1));
    text.push('\n');

    // Issues rows (up to 5 per column in text mode).
    let max_rows = board
        .columns
        .iter()
        .map(|c| c.issues.len().min(5))
        .max()
        .unwrap_or(0);
    for row in 0..max_rows {
        let mut line = String::from("|");
        for col in &board.columns {
            if let Some(issue) = col.issues.get(row) {
                let entry = format!(
                    "#{} {}",
                    issue.external_id,
                    issue.title.chars().take(col_width - 6).collect::<String>()
                );
                line.push_str(&format!(" {:<width$}|", entry, width = col_width - 2));
            } else {
                line.push_str(&format!(" {:<width$}|", "", width = col_width - 2));
            }
        }
        text.push_str(&line);
        text.push('\n');
    }

    let json_val = json!({ "board": board.columns.iter().map(|c| json!({
        "id": c.id,
        "status": c.canonical_status.as_db_str(),
        "label": c.label,
        "count": c.count,
        "issues": c.issues,
    })).collect::<Vec<_>>() });
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_board_config(
    ctx: &CommandContext,
    args: &crate::cli::IssueBoardConfigArgs,
) -> Result<CommandOutput> {
    match &args.action {
        crate::cli::IssueBoardConfigAction::Show => handle_board_config_show(ctx),
        crate::cli::IssueBoardConfigAction::Set(a) => handle_board_config_set(ctx, a),
        crate::cli::IssueBoardConfigAction::Reset => handle_board_config_reset(ctx),
    }
}

fn handle_board_config_show(ctx: &CommandContext) -> Result<CommandOutput> {
    let settings = current_project_settings(ctx)?;
    let effective = IssueBoardConfig::normalized_or_default(settings.issue_board.clone());
    Ok(to_text_or_json(
        ctx.format,
        format!("Issue board columns: {}", effective.columns.len()),
        json!({
            "custom": settings.issue_board.is_some(),
            "issue_board": effective,
        }),
    ))
}

fn handle_board_config_set(
    ctx: &CommandContext,
    args: &crate::cli::IssueBoardConfigSetArgs,
) -> Result<CommandOutput> {
    let raw = fs::read_to_string(&args.file)?;
    let config: IssueBoardConfig = serde_json::from_str(&raw)?;
    let normalized = IssueBoardConfig::normalized_or_default(Some(config));
    let mut settings = current_project_settings(ctx)?;
    settings.issue_board = Some(normalized.clone());
    save_current_project_settings(ctx, &settings)?;

    Ok(to_text_or_json(
        ctx.format,
        format!("Issue board config updated from {}", args.file),
        json!({ "issue_board": normalized, "custom": true }),
    ))
}

fn handle_board_config_reset(ctx: &CommandContext) -> Result<CommandOutput> {
    let mut settings = current_project_settings(ctx)?;
    settings.issue_board = None;
    save_current_project_settings(ctx, &settings)?;

    Ok(to_text_or_json(
        ctx.format,
        "Issue board config reset to defaults".to_string(),
        json!({
            "custom": false,
            "issue_board": IssueBoardConfig::canonical_default(),
        }),
    ))
}

fn handle_search(
    ctx: &CommandContext,
    args: &crate::cli::IssueSearchArgs,
) -> Result<CommandOutput> {
    let project_id = project_id_for(ctx);
    let conn = db_connect(ctx)?;

    // First, search the local DB.
    let local = issues_repo::list(
        &conn,
        &project_id,
        &IssueFilter {
            label: Some(args.query.clone()),
            limit: args.limit,
            ..Default::default()
        },
    )?;

    // If a provider is active, also search remotely.
    let mut remote: Vec<grove_core::tracker::Issue> = Vec::new();
    if let Ok(cfg) = grove_core::config::GroveConfig::load_or_create(&ctx.project_root) {
        if let Ok(backend) = tracker::build_backend(&cfg, &ctx.project_root) {
            if args
                .provider
                .as_deref()
                .map(|p| p == backend.provider_name())
                .unwrap_or(true)
            {
                remote = backend.search(&args.query, args.limit).unwrap_or_default();
            }
        }
    }

    let json_val = json!({ "local": local, "remote": remote, "query": args.query });
    let mut text = format!(
        "Search: '{}'\nLocal: {} result(s), Remote: {} result(s)\n",
        args.query,
        local.len(),
        remote.len()
    );
    for issue in local.iter().chain(remote.iter()).take(args.limit) {
        text.push_str(&format!(
            "  #{} [{}] {}\n",
            issue.external_id, issue.status, issue.title
        ));
    }
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_update(
    ctx: &CommandContext,
    args: &crate::cli::IssueUpdateArgs,
) -> Result<CommandOutput> {
    let project_id = project_id_for(ctx);
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;

    let update = IssueUpdate {
        title: args.title.clone(),
        body: None,
        status: args.status.clone(),
        labels: args
            .label
            .as_deref()
            .map(|l| l.split(',').map(|s| s.trim().to_string()).collect()),
        assignee: args.assignee.clone(),
        priority: args.priority.clone(),
    };

    // Push to provider if we can.
    if let Ok(backend) = tracker::build_backend(&cfg, &ctx.project_root) {
        // Strip the provider prefix to get the external ID.
        let ext_id = args.id.splitn(2, ':').nth(1).unwrap_or(&args.id);
        let _ = backend.update(ext_id, &update);
    }

    // Update locally.
    let handle = grove_core::db::DbHandle::new(&ctx.project_root);
    let mut conn = handle.connect()?;
    issues_repo::update_fields(&mut conn, &args.id, &update)?;

    let _ = project_id;
    let text = format!("Updated issue {}", args.id);
    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({ "updated": args.id }),
    ))
}

fn handle_comment(
    ctx: &CommandContext,
    args: &crate::cli::IssueCommentArgs,
) -> Result<CommandOutput> {
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;
    let handle = grove_core::db::DbHandle::new(&ctx.project_root);
    let mut conn = handle.connect()?;

    let mut posted = false;
    if let Ok(backend) = tracker::build_backend(&cfg, &ctx.project_root) {
        let ext_id = args.id.splitn(2, ':').nth(1).unwrap_or(&args.id);
        posted = backend.comment(ext_id, &args.body).is_ok();
    }

    issues_repo::add_comment(&mut conn, &args.id, &args.body, "cli", posted)?;

    let text = if posted {
        format!("Comment posted to #{} (provider + local)", args.id)
    } else {
        format!(
            "Comment stored locally for #{} (provider not available)",
            args.id
        )
    };
    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({ "issue_id": args.id, "posted": posted }),
    ))
}

fn handle_assign(
    ctx: &CommandContext,
    args: &crate::cli::IssueAssignArgs,
) -> Result<CommandOutput> {
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;
    let handle = grove_core::db::DbHandle::new(&ctx.project_root);
    let mut conn = handle.connect()?;

    if let Ok(backend) = tracker::build_backend(&cfg, &ctx.project_root) {
        let ext_id = args.id.splitn(2, ':').nth(1).unwrap_or(&args.id);
        let _ = backend.assign(ext_id, &args.assignee);
    }

    let update = IssueUpdate {
        assignee: Some(args.assignee.clone()),
        ..Default::default()
    };
    issues_repo::update_fields(&mut conn, &args.id, &update)?;

    let text = format!("Assigned {} to #{}", args.assignee, args.id);
    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({ "issue_id": args.id, "assignee": args.assignee }),
    ))
}

fn handle_move(ctx: &CommandContext, args: &crate::cli::IssueMoveArgs) -> Result<CommandOutput> {
    let project_id = project_id_for(ctx);
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;
    let handle = grove_core::db::DbHandle::new(&ctx.project_root);
    let mut conn = handle.connect()?;

    let provider = args.id.splitn(2, ':').next().unwrap_or("github");
    let target_status =
        issues_repo::resolve_column_target_status(&conn, &project_id, &args.status, provider)?
            .unwrap_or_else(|| args.status.clone());

    if let Ok(backend) = tracker::build_backend(&cfg, &ctx.project_root) {
        let ext_id = args.id.splitn(2, ':').nth(1).unwrap_or(&args.id);
        let _ = backend.transition(ext_id, &target_status);
    }

    let canonical = grove_core::tracker::status::normalize(provider, &target_status);
    issues_repo::update_status(&mut conn, &args.id, &target_status, canonical)?;

    let text = format!("Moved #{} → {}", args.id, target_status);
    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({ "issue_id": args.id, "status": target_status }),
    ))
}

fn handle_reopen(
    ctx: &CommandContext,
    args: &crate::cli::IssueReopenArgs,
) -> Result<CommandOutput> {
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;
    let handle = grove_core::db::DbHandle::new(&ctx.project_root);
    let mut conn = handle.connect()?;

    if let Ok(backend) = tracker::build_backend(&cfg, &ctx.project_root) {
        let ext_id = args.id.splitn(2, ':').nth(1).unwrap_or(&args.id);
        let _ = backend.reopen(ext_id);
    }

    let provider = args.id.splitn(2, ':').next().unwrap_or("github");
    let canonical = grove_core::tracker::status::normalize(provider, "open");
    issues_repo::update_status(&mut conn, &args.id, "open", canonical)?;

    let text = format!("Reopened issue #{}", args.id);
    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({ "issue_id": args.id, "status": "open" }),
    ))
}

fn handle_push(ctx: &CommandContext, args: &crate::cli::IssuePushArgs) -> Result<CommandOutput> {
    let project_id = project_id_for(ctx);
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;
    let conn = db_connect(ctx)?;

    let issue = issues_repo::get(&conn, &args.id)?
        .ok_or_else(|| anyhow::anyhow!("issue '{}' not found in local board", args.id))?;

    // Build a temporary config targeting the requested provider.
    let backend = tracker::build_backend(&cfg, &ctx.project_root)
        .map_err(|e| anyhow::anyhow!("provider '{}' not configured: {e}", args.to))?;

    if backend.provider_name() != args.to.as_str() {
        anyhow::bail!("provider '{}' not configured in grove.yaml", args.to);
    }

    let body = issue.body.as_deref().unwrap_or("");
    let created = backend.create(&issue.title, body)?;

    // Link the new external issue back to the local Grove issue.
    let _ = issues_repo::upsert(&conn, &created, &project_id);

    let text = format!(
        "Pushed '{}' to {} as #{}",
        issue.title, args.to, created.external_id
    );
    Ok(to_text_or_json(ctx.format, text, json!(created)))
}

fn handle_activity(
    ctx: &CommandContext,
    args: &crate::cli::IssueActivityArgs,
) -> Result<CommandOutput> {
    let conn = db_connect(ctx)?;
    let events = issues_repo::list_events(&conn, &args.id)?;
    let comments = issues_repo::list_comments(&conn, &args.id)?;

    let json_val = json!({ "events": events, "comments": comments });
    let mut text = format!("Activity for {}\n", args.id);
    for e in &events {
        text.push_str(&format!(
            "[{}] {} by {}\n",
            e.created_at,
            e.event_type,
            e.actor.as_deref().unwrap_or("grove")
        ));
    }
    for c in &comments {
        text.push_str(&format!(
            "[{}] Comment by {}: {}\n",
            c.created_at,
            c.author.as_deref().unwrap_or("unknown"),
            c.body.chars().take(80).collect::<String>()
        ));
    }
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_lint(ctx: &CommandContext, args: &crate::cli::IssueLintArgs) -> Result<CommandOutput> {
    let project_id = project_id_for(ctx);
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;

    if !cfg.linter.enabled || cfg.linter.commands.is_empty() {
        return Ok(to_text_or_json(
            ctx.format,
            "No linters configured. Add linter.commands to grove.yaml.".into(),
            json!({ "linters": 0 }),
        ));
    }

    let handle = grove_core::db::DbHandle::new(&ctx.project_root);
    let mut conn = handle.connect()?;

    let result = grove_core::tracker::sync::sync_lint_issues(
        &mut conn,
        &cfg,
        &ctx.project_root,
        &project_id,
    );

    let text = format!(
        "Linter sync: +{} new, {} updated, {} error(s) ({}ms)",
        result.new_count,
        result.updated_count,
        result.errors.len(),
        result.duration_ms
    );

    if args.fix && result.new_count + result.updated_count > 0 {
        let linter_issues = issues_repo::list(
            &conn,
            &project_id,
            &IssueFilter {
                provider: Some("linter".into()),
                canonical_status: Some(CanonicalStatus::Open),
                limit: 50,
                ..Default::default()
            },
        )?;
        if !linter_issues.is_empty() {
            let mut objective = "Fix the following lint issues:\n\n".to_string();
            for issue in &linter_issues {
                objective.push_str(&format!("- {}\n", issue.title));
            }
            let json_val =
                json!({ "fix_objective": objective, "issue_count": linter_issues.len() });
            let fix_text = format!(
                "{text}\nUse `grove run '<objective>'` to fix {len} issue(s).\n\nObjective:\n{objective}",
                len = linter_issues.len()
            );
            return Ok(to_text_or_json(ctx.format, fix_text, json_val));
        }
    }

    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({ "result": result }),
    ))
}
