use crate::cli::{WorkspaceAction, WorkspaceArgs};
use crate::error::{CliError, CliResult};
use crate::output::{OutputMode, json as json_out, text};
use crate::transport::{GroveTransport, Transport};

// ── arg structs ───────────────────────────────────────────────────────────────

pub struct WorkspaceSetNameArgs {
    pub name: String,
}

pub struct WorkspaceArchiveArgs {
    pub id: String,
}

pub struct WorkspaceDeleteArgs {
    pub id: String,
}

// ── dispatch ──────────────────────────────────────────────────────────────────

pub fn dispatch(a: WorkspaceArgs, t: GroveTransport, m: OutputMode) -> CliResult<()> {
    match a.action {
        WorkspaceAction::Show => show_cmd(t, m),
        WorkspaceAction::SetName { name } => set_name_cmd(WorkspaceSetNameArgs { name }, t, m),
        WorkspaceAction::Archive { id } => archive_cmd(WorkspaceArchiveArgs { id }, t, m),
        WorkspaceAction::Delete { id } => delete_cmd(WorkspaceDeleteArgs { id }, t, m),
    }
}

// ── show ──────────────────────────────────────────────────────────────────────

pub fn show_cmd(transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let row = transport.get_workspace()?;

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
                println!("{}", text::dim("no workspace found"));
            }
            Some(r) => {
                println!("id:           {}", r.id);
                println!("name:         {}", r.name.as_deref().unwrap_or("<unset>"));
                println!("state:        {}", r.state);
                println!(
                    "llm_provider: {}",
                    r.llm_provider.as_deref().unwrap_or("<unset>")
                );
                println!(
                    "llm_model:    {}",
                    r.llm_model.as_deref().unwrap_or("<unset>")
                );
            }
        },
    }
    Ok(())
}

// ── set-name ─────────────────────────────────────────────────────────────────

pub fn set_name_cmd(
    args: WorkspaceSetNameArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.set_workspace_name(&args.name)?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true, "name": args.name }))
            );
        }
        OutputMode::Text { .. } => {
            println!("workspace name set to '{}'", args.name);
        }
    }
    Ok(())
}

// ── archive ───────────────────────────────────────────────────────────────────

pub fn archive_cmd(
    args: WorkspaceArchiveArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.archive_workspace(&args.id)?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true, "id": args.id }))
            );
        }
        OutputMode::Text { .. } => {
            println!("archived workspace {}", args.id);
        }
    }
    Ok(())
}

// ── delete ────────────────────────────────────────────────────────────────────

pub fn delete_cmd(
    args: WorkspaceDeleteArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.delete_workspace(&args.id)?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true, "id": args.id }))
            );
        }
        OutputMode::Text { .. } => {
            println!("deleted workspace {}", args.id);
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
    fn workspace_show_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(show_cmd(t, crate::output::OutputMode::Text { no_color: true }).is_ok());
    }

    #[test]
    fn workspace_show_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(show_cmd(t, crate::output::OutputMode::Json).is_ok());
    }

    #[test]
    fn workspace_set_name_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = set_name_cmd(
            WorkspaceSetNameArgs {
                name: "my-ws".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }

    #[test]
    fn workspace_archive_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = archive_cmd(
            WorkspaceArchiveArgs { id: "ws_1".into() },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }

    #[test]
    fn workspace_delete_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = delete_cmd(
            WorkspaceDeleteArgs { id: "ws_1".into() },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }
}
