use crate::cli::{ConversationAction, ConversationArgs};
use crate::error::{CliError, CliResult};
use crate::output::{OutputMode, json as json_out, text};
use crate::transport::{GroveTransport, Transport};

// ── arg structs ───────────────────────────────────────────────────────────────

pub struct ConversationListArgs {
    pub limit: i64,
}

pub struct ConversationShowArgs {
    pub id: String,
}

pub struct ConversationArchiveArgs {
    pub id: String,
}

pub struct ConversationDeleteArgs {
    pub id: String,
}

pub struct ConversationRebaseArgs {
    pub id: String,
}

pub struct ConversationMergeArgs {
    pub id: String,
}

// ── dispatch ──────────────────────────────────────────────────────────────────

pub fn dispatch(a: ConversationArgs, t: GroveTransport, m: OutputMode) -> CliResult<()> {
    match a.action {
        ConversationAction::List { limit } => list_cmd(
            ConversationListArgs {
                limit: i64::from(limit),
            },
            t,
            m,
        ),
        ConversationAction::Show { id, limit: _limit } => {
            // _limit: reserved for future message pagination
            show_cmd(ConversationShowArgs { id }, t, m)
        }
        ConversationAction::Archive { id } => archive_cmd(ConversationArchiveArgs { id }, t, m),
        ConversationAction::Delete { id } => delete_cmd(ConversationDeleteArgs { id }, t, m),
        ConversationAction::Rebase { id } => rebase_cmd(ConversationRebaseArgs { id }, t, m),
        ConversationAction::Merge { id } => merge_cmd(ConversationMergeArgs { id }, t, m),
    }
}

// ── list ──────────────────────────────────────────────────────────────────────

pub fn list_cmd(
    args: ConversationListArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    let convs = transport.list_conversations(args.limit)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::to_value(&convs).map_err(|e| CliError::Other(e.to_string()))?;
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if convs.is_empty() {
                println!("{}", text::dim("no conversations"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = convs
                .iter()
                .map(|c| {
                    vec![
                        c.id.chars().take(8).collect(),
                        c.title.as_deref().unwrap_or("").chars().take(40).collect(),
                        c.state.clone(),
                        c.conversation_kind.clone(),
                        c.branch_name.as_deref().unwrap_or("").to_string(),
                        c.created_at.chars().take(19).collect(),
                    ]
                })
                .collect();
            println!(
                "{}",
                text::render_table(
                    &["ID", "TITLE", "STATE", "KIND", "BRANCH", "CREATED"],
                    &rows
                )
            );
        }
    }
    Ok(())
}

// ── show ──────────────────────────────────────────────────────────────────────

pub fn show_cmd(
    args: ConversationShowArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    let row = transport.get_conversation(&args.id)?;

    match mode {
        OutputMode::Json => {
            let val = match row {
                Some(r) => serde_json::to_value(&r).map_err(|e| CliError::Other(e.to_string()))?,
                None => serde_json::Value::Null,
            };
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => match row {
            None => {
                let msg = format!("conversation {} not found", args.id);
                println!("{}", text::dim(&msg));
            }
            Some(r) => {
                println!("id:     {}", r.id);
                println!("title:  {}", r.title.as_deref().unwrap_or("<unset>"));
                println!("state:  {}", r.state);
                println!("kind:   {}", r.conversation_kind);
                println!("branch: {}", r.branch_name.as_deref().unwrap_or("<none>"));
                println!("created:{}", r.created_at);
            }
        },
    }
    Ok(())
}

// ── archive ───────────────────────────────────────────────────────────────────

pub fn archive_cmd(
    args: ConversationArchiveArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.archive_conversation(&args.id)?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true, "id": args.id }))
            );
        }
        OutputMode::Text { .. } => {
            println!("archived {}", args.id);
        }
    }
    Ok(())
}

// ── delete ────────────────────────────────────────────────────────────────────

pub fn delete_cmd(
    args: ConversationDeleteArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.delete_conversation(&args.id)?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true, "id": args.id }))
            );
        }
        OutputMode::Text { .. } => {
            println!("deleted {}", args.id);
        }
    }
    Ok(())
}

// ── rebase ────────────────────────────────────────────────────────────────────

pub fn rebase_cmd(
    args: ConversationRebaseArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.rebase_conversation(&args.id)?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true, "id": args.id }))
            );
        }
        OutputMode::Text { .. } => {
            println!("rebased {}", args.id);
        }
    }
    Ok(())
}

// ── merge ─────────────────────────────────────────────────────────────────────

pub fn merge_cmd(
    args: ConversationMergeArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.merge_conversation(&args.id)?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true, "id": args.id }))
            );
        }
        OutputMode::Text { .. } => {
            println!("merged {}", args.id);
        }
    }
    Ok(())
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{GroveTransport, TestTransport};

    #[test]
    fn conversation_list_ok() {
        let t = GroveTransport::Test(TestTransport);
        assert!(
            list_cmd(
                ConversationListArgs { limit: 20 },
                t,
                crate::output::OutputMode::Text { no_color: true }
            )
            .is_ok()
        );
    }

    #[test]
    fn conversation_list_json_ok() {
        let t = GroveTransport::Test(TestTransport);
        assert!(
            list_cmd(
                ConversationListArgs { limit: 20 },
                t,
                crate::output::OutputMode::Json
            )
            .is_ok()
        );
    }

    #[test]
    fn conversation_show_ok_when_not_found() {
        let t = GroveTransport::Test(TestTransport);
        assert!(
            show_cmd(
                ConversationShowArgs {
                    id: "conv_1".into()
                },
                t,
                crate::output::OutputMode::Text { no_color: true }
            )
            .is_ok()
        );
    }

    #[test]
    fn conversation_archive_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport);
        let result = archive_cmd(
            ConversationArchiveArgs {
                id: "conv_1".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }

    #[test]
    fn conversation_delete_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport);
        let result = delete_cmd(
            ConversationDeleteArgs {
                id: "conv_1".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }

    #[test]
    fn conversation_rebase_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport);
        let result = rebase_cmd(
            ConversationRebaseArgs {
                id: "conv_1".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }

    #[test]
    fn conversation_merge_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport);
        let result = merge_cmd(
            ConversationMergeArgs {
                id: "conv_1".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }
}
