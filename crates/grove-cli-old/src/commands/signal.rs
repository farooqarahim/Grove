use anyhow::Result;
use grove_core::signals;
use serde_json::json;

use crate::cli::SignalArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &SignalArgs) -> Result<CommandOutput> {
    match &args.action {
        crate::cli::SignalAction::Send(send_args) => handle_send(ctx, send_args),
        crate::cli::SignalAction::Check(check_args) => handle_check(ctx, check_args),
        crate::cli::SignalAction::List(list_args) => handle_list(ctx, list_args),
    }
}

fn handle_send(ctx: &CommandContext, args: &crate::cli::SignalSendArgs) -> Result<CommandOutput> {
    let handle = grove_core::db::DbHandle::new(&ctx.project_root);
    let conn = handle.connect()?;

    let signal_type = signals::SignalType::parse(&args.signal_type)
        .ok_or_else(|| anyhow::anyhow!("unknown signal type: {}", args.signal_type))?;

    let priority = signals::SignalPriority::parse(args.priority.as_deref().unwrap_or("normal"));

    let payload: serde_json::Value = args
        .payload
        .as_deref()
        .map(|s| serde_json::from_str(s).unwrap_or(json!({"message": s})))
        .unwrap_or(json!({}));

    let id = signals::send_signal(
        &conn,
        &args.run_id,
        &args.from,
        &args.to,
        signal_type,
        priority,
        payload,
    )?;

    let text = format!("Signal sent: {id}");
    let json = json!({ "id": id });
    Ok(to_text_or_json(ctx.format, text, json))
}

fn handle_check(ctx: &CommandContext, args: &crate::cli::SignalCheckArgs) -> Result<CommandOutput> {
    let handle = grove_core::db::DbHandle::new(&ctx.project_root);
    let conn = handle.connect()?;

    let signals = signals::check_signals(&conn, &args.run_id, &args.agent_name)?;

    let json = json!({ "signals": signals });
    let mut text = format!(
        "Unread signals for {} in {}: {}",
        args.agent_name,
        args.run_id,
        signals.len()
    );
    for s in &signals {
        text.push_str(&format!(
            "\n  [{} -> {}] type={} priority={} id={}",
            s.from_agent, s.to_agent, s.signal_type, s.priority, s.id
        ));
    }

    Ok(to_text_or_json(ctx.format, text, json))
}

fn handle_list(ctx: &CommandContext, args: &crate::cli::SignalListArgs) -> Result<CommandOutput> {
    let handle = grove_core::db::DbHandle::new(&ctx.project_root);
    let conn = handle.connect()?;

    let signals = signals::list_for_run(&conn, &args.run_id)?;

    let json = json!({ "signals": signals });
    let mut text = format!("Signals for {}: {}", args.run_id, signals.len());
    for s in &signals {
        let read_marker = if s.read { " [read]" } else { "" };
        text.push_str(&format!(
            "\n  [{} -> {}] type={} priority={}{read_marker}",
            s.from_agent, s.to_agent, s.signal_type, s.priority
        ));
    }

    Ok(to_text_or_json(ctx.format, text, json))
}
