use std::path::Path;

use crate::cli::{ProjectAction, ProjectArgs};
use crate::error::{CliError, CliResult};
use crate::output::{OutputMode, json as json_out, text};
use crate::transport::{GroveTransport, Transport};

// ── arg structs ───────────────────────────────────────────────────────────────

pub struct ProjectSetNameArgs {
    pub name: String,
}

pub struct ProjectSetArgs {
    pub provider: Option<String>,
    pub parallel: Option<i64>,
    pub pipeline: Option<String>,
    pub permission_mode: Option<String>,
}

pub struct ProjectArchiveArgs {
    pub id: Option<String>,
}

pub struct ProjectDeleteArgs {
    pub id: Option<String>,
}

// ── dispatch ──────────────────────────────────────────────────────────────────

pub fn dispatch(a: ProjectArgs, _p: &Path, t: GroveTransport, m: OutputMode) -> CliResult<()> {
    // _p: reserved for open_folder/clone/ssh commands (future tasks)
    match a.action {
        ProjectAction::Show => show_cmd(t, m),
        ProjectAction::List => list_cmd(t, m),
        ProjectAction::SetName { name } => set_name_cmd(ProjectSetNameArgs { name }, t, m),
        ProjectAction::Set {
            provider,
            parallel,
            pipeline,
            permission_mode,
            reset: _,
        } => set_cmd(
            ProjectSetArgs {
                provider,
                parallel: parallel.map(i64::from),
                pipeline,
                permission_mode,
            },
            t,
            m,
        ),
        ProjectAction::Archive { id } => archive_cmd(ProjectArchiveArgs { id }, t, m),
        ProjectAction::Delete { id } => delete_cmd(ProjectDeleteArgs { id }, t, m),
        ProjectAction::OpenFolder { .. } => open_folder_cmd(t, m),
        ProjectAction::Clone { .. } => clone_cmd(t, m),
        ProjectAction::CreateRepo { .. } => create_repo_cmd(t, m),
        ProjectAction::ForkRepo { .. } => fork_repo_cmd(t, m),
        ProjectAction::ForkFolder { .. } => fork_folder_cmd(t, m),
        ProjectAction::Ssh { .. } => ssh_cmd(t, m),
        ProjectAction::SshShell { .. } => ssh_shell_cmd(t, m),
    }
}

// ── show ──────────────────────────────────────────────────────────────────────

pub fn show_cmd(transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let row = transport.get_project()?;

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
                println!("{}", text::dim("no project found"));
            }
            Some(r) => {
                println!("id:          {}", r.id);
                println!("name:        {}", r.name.as_deref().unwrap_or("<unset>"));
                println!("root_path:   {}", r.root_path);
                println!("kind:        {}", r.source_kind);
                println!("state:       {}", r.state);
            }
        },
    }
    Ok(())
}

// ── list ──────────────────────────────────────────────────────────────────────

pub fn list_cmd(transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let projects = transport.list_projects()?;

    match mode {
        OutputMode::Json => {
            let val =
                serde_json::to_value(&projects).map_err(|e| CliError::Other(e.to_string()))?;
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if projects.is_empty() {
                println!("{}", text::dim("no projects"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = projects
                .iter()
                .map(|r| {
                    vec![
                        r.id.chars().take(8).collect(),
                        r.name.as_deref().unwrap_or("").to_string(),
                        r.root_path.chars().take(40).collect(),
                        r.source_kind.clone(),
                        r.state.clone(),
                    ]
                })
                .collect();
            println!(
                "{}",
                text::render_table(&["ID", "NAME", "PATH", "KIND", "STATE"], &rows)
            );
        }
    }
    Ok(())
}

// ── set-name ─────────────────────────────────────────────────────────────────

pub fn set_name_cmd(
    args: ProjectSetNameArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.set_project_name(&args.name)?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true, "name": args.name }))
            );
        }
        OutputMode::Text { .. } => {
            println!("project name set to '{}'", args.name);
        }
    }
    Ok(())
}

// ── set ───────────────────────────────────────────────────────────────────────

pub fn set_cmd(args: ProjectSetArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    transport.set_project_settings(
        args.provider.as_deref(),
        args.parallel,
        args.pipeline.as_deref(),
        args.permission_mode.as_deref(),
    )?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true }))
            );
        }
        OutputMode::Text { .. } => {
            println!("project settings updated");
        }
    }
    Ok(())
}

// ── archive ───────────────────────────────────────────────────────────────────

pub fn archive_cmd(
    args: ProjectArchiveArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.archive_project(args.id.as_deref())?;

    let id_display = args.id.as_deref().unwrap_or("current");
    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true, "id": id_display }))
            );
        }
        OutputMode::Text { .. } => {
            println!("archived project {id_display}");
        }
    }
    Ok(())
}

// ── delete ────────────────────────────────────────────────────────────────────

pub fn delete_cmd(
    args: ProjectDeleteArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.delete_project(args.id.as_deref())?;

    let id_display = args.id.as_deref().unwrap_or("current");
    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::json!({ "ok": true, "id": id_display }))
            );
        }
        OutputMode::Text { .. } => {
            println!("deleted project {id_display}");
        }
    }
    Ok(())
}

// ── stubs ─────────────────────────────────────────────────────────────────────

pub fn open_folder_cmd(_transport: GroveTransport, _mode: OutputMode) -> CliResult<()> {
    Err(CliError::Other("not yet implemented".into()))
}

pub fn clone_cmd(_transport: GroveTransport, _mode: OutputMode) -> CliResult<()> {
    Err(CliError::Other("not yet implemented".into()))
}

pub fn create_repo_cmd(_transport: GroveTransport, _mode: OutputMode) -> CliResult<()> {
    Err(CliError::Other("not yet implemented".into()))
}

pub fn fork_repo_cmd(_transport: GroveTransport, _mode: OutputMode) -> CliResult<()> {
    Err(CliError::Other("not yet implemented".into()))
}

pub fn fork_folder_cmd(_transport: GroveTransport, _mode: OutputMode) -> CliResult<()> {
    Err(CliError::Other("not yet implemented".into()))
}

pub fn ssh_cmd(_transport: GroveTransport, _mode: OutputMode) -> CliResult<()> {
    Err(CliError::Other("not yet implemented".into()))
}

pub fn ssh_shell_cmd(_transport: GroveTransport, _mode: OutputMode) -> CliResult<()> {
    Err(CliError::Other("not yet implemented".into()))
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{GroveTransport, TestTransport};

    #[test]
    fn project_list_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(list_cmd(t, crate::output::OutputMode::Text { no_color: true }).is_ok());
    }

    #[test]
    fn project_list_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(list_cmd(t, crate::output::OutputMode::Json).is_ok());
    }

    #[test]
    fn project_show_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(show_cmd(t, crate::output::OutputMode::Text { no_color: true }).is_ok());
    }

    #[test]
    fn project_set_name_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = set_name_cmd(
            ProjectSetNameArgs {
                name: "my-proj".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }

    #[test]
    fn project_archive_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = archive_cmd(
            ProjectArchiveArgs { id: None },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }

    #[test]
    fn project_delete_returns_err_on_test_transport() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = delete_cmd(
            ProjectDeleteArgs { id: None },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }

    #[test]
    fn project_open_folder_returns_not_implemented() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(open_folder_cmd(t, crate::output::OutputMode::Text { no_color: true }).is_err());
    }
}
