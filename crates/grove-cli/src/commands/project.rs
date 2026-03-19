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

pub fn dispatch(a: ProjectArgs, p: &Path, t: GroveTransport, m: OutputMode) -> CliResult<()> {
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
        ProjectAction::OpenFolder { path, name } => open_folder_cmd(p, path, name, m),
        ProjectAction::Clone { repo, path, name } => clone_cmd(p, repo, path, name, m),
        ProjectAction::CreateRepo {
            repo,
            path,
            provider,
            visibility,
            gitignore,
        } => create_repo_cmd(p, repo, path, provider, visibility, gitignore, m),
        ProjectAction::ForkRepo {
            src,
            target,
            repo,
            provider,
        } => fork_repo_cmd(p, src, target, repo, provider, m),
        ProjectAction::ForkFolder {
            src,
            target,
            preserve_git,
        } => fork_folder_cmd(p, src, target, preserve_git, m),
        ProjectAction::Ssh {
            host,
            remote_path,
            user,
            port,
        } => ssh_cmd(p, host, remote_path, user, port, m),
        ProjectAction::SshShell { id } => ssh_shell_cmd(p, id, m),
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

// ── open-folder ───────────────────────────────────────────────────────────────

pub fn open_folder_cmd(
    project: &Path,
    path: String,
    name: Option<String>,
    mode: OutputMode,
) -> CliResult<()> {
    let row = grove_core::orchestrator::create_project_from_source(
        project,
        grove_core::orchestrator::ProjectCreateRequest::OpenFolder {
            root_path: path,
            name,
        },
    )
    .map_err(CliError::Core)?;

    emit_project_created(&row, mode)
}

// ── clone ─────────────────────────────────────────────────────────────────────

pub fn clone_cmd(
    project: &Path,
    repo: String,
    target_path: String,
    name: Option<String>,
    mode: OutputMode,
) -> CliResult<()> {
    let row = grove_core::orchestrator::create_project_from_source(
        project,
        grove_core::orchestrator::ProjectCreateRequest::CloneGitRepo {
            repo_url: repo,
            target_path,
            name,
        },
    )
    .map_err(CliError::Core)?;

    emit_project_created(&row, mode)
}

// ── create-repo ───────────────────────────────────────────────────────────────

pub fn create_repo_cmd(
    project: &Path,
    repo: String,
    path: String,
    provider: Option<String>,
    visibility: Option<String>,
    gitignore: Option<String>,
    mode: OutputMode,
) -> CliResult<()> {
    let row = grove_core::orchestrator::create_project_from_source(
        project,
        grove_core::orchestrator::ProjectCreateRequest::CreateRepo {
            provider: provider.unwrap_or_else(|| "github".to_string()),
            repo_name: repo,
            target_path: path,
            owner: None,
            visibility: visibility.unwrap_or_else(|| "private".to_string()),
            gitignore_template: gitignore,
            gitignore_entries: vec![],
            name: None,
        },
    )
    .map_err(CliError::Core)?;

    emit_project_created(&row, mode)
}

// ── fork-repo ─────────────────────────────────────────────────────────────────

pub fn fork_repo_cmd(
    project: &Path,
    src: String,
    target: String,
    repo: String,
    provider: Option<String>,
    mode: OutputMode,
) -> CliResult<()> {
    let row = grove_core::orchestrator::create_project_from_source(
        project,
        grove_core::orchestrator::ProjectCreateRequest::ForkRepoToRemote {
            provider: provider.unwrap_or_else(|| "github".to_string()),
            source_path: src,
            target_path: target,
            repo_name: repo,
            owner: None,
            visibility: "private".to_string(),
            remote_name: None,
            name: None,
        },
    )
    .map_err(CliError::Core)?;

    emit_project_created(&row, mode)
}

// ── fork-folder ───────────────────────────────────────────────────────────────

pub fn fork_folder_cmd(
    project: &Path,
    src: String,
    target: String,
    preserve_git: bool,
    mode: OutputMode,
) -> CliResult<()> {
    let row = grove_core::orchestrator::create_project_from_source(
        project,
        grove_core::orchestrator::ProjectCreateRequest::ForkFolderToFolder {
            source_path: src,
            target_path: target,
            preserve_git,
            name: None,
        },
    )
    .map_err(CliError::Core)?;

    emit_project_created(&row, mode)
}

// ── ssh ───────────────────────────────────────────────────────────────────────

pub fn ssh_cmd(
    project: &Path,
    host: String,
    remote_path: String,
    user: Option<String>,
    port: Option<u16>,
    mode: OutputMode,
) -> CliResult<()> {
    let row = grove_core::orchestrator::create_project_from_source(
        project,
        grove_core::orchestrator::ProjectCreateRequest::Ssh {
            host,
            remote_path,
            user,
            port,
            name: None,
        },
    )
    .map_err(CliError::Core)?;

    emit_project_created(&row, mode)
}

// ── ssh-shell ─────────────────────────────────────────────────────────────────

pub fn ssh_shell_cmd(project: &Path, id: Option<String>, _mode: OutputMode) -> CliResult<()> {
    let project_row = match id {
        Some(ref pid) => grove_core::orchestrator::list_projects(project)
            .map_err(CliError::Core)?
            .into_iter()
            .find(|p| p.id == *pid || p.id.starts_with(pid.as_str()))
            .ok_or_else(|| CliError::NotFound(format!("project {pid}")))?,
        None => grove_core::orchestrator::get_project(project).map_err(CliError::Core)?,
    };

    if project_row.source_kind != "ssh" {
        return Err(CliError::BadArg("project is not an SSH project".into()));
    }

    let ssh_url = &project_row.root_path;
    let (user_host, remote_path) = parse_ssh_url(ssh_url)?;

    let status = std::process::Command::new("ssh")
        .arg(&user_host)
        .arg(&remote_path)
        .status()
        .map_err(|e| CliError::Other(format!("ssh: {e}")))?;

    if !status.success() {
        return Err(CliError::Other(format!("ssh exited with {status}")));
    }
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Parse an SSH URL of the form `ssh://[user@]host[:port]/remote_path`
/// into `("user@host:port", "/remote_path")`.
fn parse_ssh_url(url: &str) -> CliResult<(String, String)> {
    let rest = url
        .strip_prefix("ssh://")
        .ok_or_else(|| CliError::BadArg(format!("invalid ssh URL (expected ssh://...): {url}")))?;

    // Split authority from path at the first '/'.
    let (authority, path) = rest.split_once('/').ok_or_else(|| {
        CliError::BadArg(format!(
            "invalid ssh URL (no path component after authority): {url}"
        ))
    })?;

    // authority is [user@]host[:port]
    // We pass it verbatim to ssh as the destination and prefix '/' back onto path.
    Ok((authority.to_string(), format!("/{path}")))
}

fn emit_project_created(
    row: &grove_core::db::repositories::projects_repo::ProjectRow,
    mode: OutputMode,
) -> CliResult<()> {
    match mode {
        OutputMode::Json => {
            let val = serde_json::to_value(row).map_err(|e| CliError::Other(e.to_string()))?;
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            println!(
                "created project {} at {}",
                row.id.chars().take(8).collect::<String>(),
                &row.root_path
            );
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
    fn project_open_folder_nonexistent_returns_err() {
        let project_dir = tempfile::tempdir().unwrap();
        // Path that does not exist — grove-core must return an error.
        let result = open_folder_cmd(
            project_dir.path(),
            "/nonexistent/path/that/cannot/exist".to_string(),
            None,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }

    #[test]
    fn parse_ssh_url_full() {
        let (host, path) = parse_ssh_url("ssh://user@host:2222/home/user").unwrap();
        assert_eq!(host, "user@host:2222");
        assert_eq!(path, "/home/user");
    }

    #[test]
    fn parse_ssh_url_no_user_no_port() {
        let (host, path) = parse_ssh_url("ssh://myhost/var/www").unwrap();
        assert_eq!(host, "myhost");
        assert_eq!(path, "/var/www");
    }

    #[test]
    fn parse_ssh_url_invalid_scheme_returns_err() {
        assert!(parse_ssh_url("http://host/path").is_err());
    }

    #[test]
    fn parse_ssh_url_no_path_returns_err() {
        assert!(parse_ssh_url("ssh://host").is_err());
    }
}
