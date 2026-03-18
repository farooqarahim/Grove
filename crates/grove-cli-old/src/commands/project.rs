use std::process::{Command, Stdio};

use anyhow::Result;
use grove_core::db;
use grove_core::db::repositories::projects_repo::{ProjectRow, ProjectSettings};
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::{
    ProjectAction, ProjectArgs, ProjectCloneArgs, ProjectCreateRepoArgs, ProjectForkFolderArgs,
    ProjectForkRepoArgs, ProjectOpenFolderArgs, ProjectSetArgs, ProjectSshArgs,
};
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &ProjectArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;

    match &args.action {
        ProjectAction::Show => handle_show(ctx),
        ProjectAction::List => handle_list(ctx),
        ProjectAction::OpenFolder(a) => handle_open_folder(ctx, a),
        ProjectAction::Clone(a) => handle_clone(ctx, a),
        ProjectAction::CreateRepo(a) => handle_create_repo(ctx, a),
        ProjectAction::ForkRepo(a) => handle_fork_repo(ctx, a),
        ProjectAction::ForkFolder(a) => handle_fork_folder(ctx, a),
        ProjectAction::Ssh(a) => handle_ssh(ctx, a),
        ProjectAction::SshShell(a) => handle_ssh_shell(ctx, a.id.as_deref()),
        ProjectAction::SetName(a) => handle_set_name(ctx, &a.name),
        ProjectAction::Set(a) => handle_set(ctx, a),
        ProjectAction::Archive(a) => handle_archive(ctx, a.id.as_deref()),
        ProjectAction::Delete(a) => handle_delete(ctx, a.id.as_deref()),
    }
}

fn project_json(row: &ProjectRow) -> serde_json::Value {
    json!({
        "id": row.id,
        "workspace_id": row.workspace_id,
        "name": row.name,
        "root_path": row.root_path,
        "state": row.state,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "source_kind": row.source_kind,
        "source_details": row.source_details,
    })
}

fn project_label(row: &ProjectRow) -> &str {
    row.name.as_deref().unwrap_or("(none)")
}

fn handle_show(ctx: &CommandContext) -> Result<CommandOutput> {
    let row = orchestrator::get_project(&ctx.project_root)?;

    let text = format!(
        "Project\n  id:           {}\n  workspace_id: {}\n  name:         {}\n  root_path:    {}\n  source_kind:  {}\n  state:        {}\n  created_at:   {}\n  updated_at:   {}",
        row.id,
        row.workspace_id,
        row.name.as_deref().unwrap_or("(none)"),
        row.root_path,
        row.source_kind,
        row.state,
        row.created_at,
        row.updated_at,
    );

    Ok(to_text_or_json(ctx.format, text, project_json(&row)))
}

fn handle_list(ctx: &CommandContext) -> Result<CommandOutput> {
    let projects = orchestrator::list_projects(&ctx.project_root)?;

    if projects.is_empty() {
        let text = "No projects registered in this workspace.".to_string();
        let json_val = json!({ "projects": [] });
        return Ok(to_text_or_json(ctx.format, text, json_val));
    }

    let mut lines = Vec::new();
    lines.push(format!("{} project(s):", projects.len()));
    for p in &projects {
        lines.push(format!(
            "  {}  {}  {}  {}",
            p.id,
            project_label(p),
            p.source_kind,
            p.state,
        ));
    }

    let json_val = json!({
        "projects": projects.iter().map(project_json).collect::<Vec<_>>(),
    });

    Ok(to_text_or_json(ctx.format, lines.join("\n"), json_val))
}

fn handle_open_folder(ctx: &CommandContext, args: &ProjectOpenFolderArgs) -> Result<CommandOutput> {
    let row = orchestrator::create_project_from_source(
        &ctx.project_root,
        orchestrator::ProjectCreateRequest::OpenFolder {
            root_path: args.path.to_string_lossy().to_string(),
            name: args.name.clone(),
        },
    )?;
    let text = format!(
        "Registered local project: {} ({})",
        project_label(&row),
        row.root_path
    );
    Ok(to_text_or_json(ctx.format, text, project_json(&row)))
}

fn handle_clone(ctx: &CommandContext, args: &ProjectCloneArgs) -> Result<CommandOutput> {
    let row = orchestrator::create_project_from_source(
        &ctx.project_root,
        orchestrator::ProjectCreateRequest::CloneGitRepo {
            repo_url: args.repo.clone(),
            target_path: args.path.to_string_lossy().to_string(),
            name: args.name.clone(),
        },
    )?;
    let text = format!(
        "Cloned and registered project: {} ({})",
        project_label(&row),
        row.root_path
    );
    Ok(to_text_or_json(ctx.format, text, project_json(&row)))
}

fn handle_create_repo(ctx: &CommandContext, args: &ProjectCreateRepoArgs) -> Result<CommandOutput> {
    let row = orchestrator::create_project_from_source(
        &ctx.project_root,
        orchestrator::ProjectCreateRequest::CreateRepo {
            provider: args.provider.clone(),
            repo_name: args.repo.clone(),
            target_path: args.path.to_string_lossy().to_string(),
            owner: args.owner.clone(),
            visibility: args.visibility.clone(),
            gitignore_template: args.gitignore.clone(),
            gitignore_entries: args.gitignore_entries.clone(),
            name: args.name.clone(),
        },
    )?;
    let text = format!(
        "Created {} project: {} ({}, {})",
        args.provider,
        project_label(&row),
        row.root_path,
        args.visibility
    );
    Ok(to_text_or_json(ctx.format, text, project_json(&row)))
}

fn handle_fork_repo(ctx: &CommandContext, args: &ProjectForkRepoArgs) -> Result<CommandOutput> {
    let row = orchestrator::create_project_from_source(
        &ctx.project_root,
        orchestrator::ProjectCreateRequest::ForkRepoToRemote {
            provider: args.provider.clone(),
            source_path: args.source_path.to_string_lossy().to_string(),
            target_path: args.target_path.to_string_lossy().to_string(),
            repo_name: args.repo.clone(),
            owner: args.owner.clone(),
            visibility: args.visibility.clone(),
            remote_name: args.remote_name.clone(),
            name: args.name.clone(),
        },
    )?;
    let text = format!(
        "Forked repo into {} project: {} ({})",
        args.provider,
        project_label(&row),
        row.root_path
    );
    Ok(to_text_or_json(ctx.format, text, project_json(&row)))
}

fn handle_fork_folder(ctx: &CommandContext, args: &ProjectForkFolderArgs) -> Result<CommandOutput> {
    let row = orchestrator::create_project_from_source(
        &ctx.project_root,
        orchestrator::ProjectCreateRequest::ForkFolderToFolder {
            source_path: args.source_path.to_string_lossy().to_string(),
            target_path: args.target_path.to_string_lossy().to_string(),
            preserve_git: args.preserve_git,
            name: args.name.clone(),
        },
    )?;
    let text = format!(
        "Forked folder into project: {} ({})",
        project_label(&row),
        row.root_path
    );
    Ok(to_text_or_json(ctx.format, text, project_json(&row)))
}

fn handle_ssh(ctx: &CommandContext, args: &ProjectSshArgs) -> Result<CommandOutput> {
    let row = orchestrator::create_project_from_source(
        &ctx.project_root,
        orchestrator::ProjectCreateRequest::Ssh {
            host: args.host.clone(),
            remote_path: args.remote_path.clone(),
            user: args.user.clone(),
            port: args.port,
            name: args.name.clone(),
        },
    )?;
    let text = format!(
        "Registered SSH project: {} ({})",
        project_label(&row),
        row.root_path
    );
    Ok(to_text_or_json(ctx.format, text, project_json(&row)))
}

fn handle_set_name(ctx: &CommandContext, name: &str) -> Result<CommandOutput> {
    let row = orchestrator::get_project(&ctx.project_root)?;
    orchestrator::update_project_name(&ctx.project_root, &row.id, name)?;
    let text = format!("Project name set to: {name}");
    let json_val = json!({ "id": row.id, "name": name, "updated": true });
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_set(ctx: &CommandContext, args: &ProjectSetArgs) -> Result<CommandOutput> {
    let row = orchestrator::get_project(&ctx.project_root)?;

    let settings = if args.reset {
        ProjectSettings::default()
    } else {
        // Load current settings and apply only the flags that were provided.
        let mut s = orchestrator::get_project_settings(&ctx.project_root, &row.id)?;
        if let Some(ref v) = args.provider {
            s.default_provider = Some(v.clone());
        }
        if let Some(ref v) = args.project_key {
            s.default_project_key = Some(v.clone());
        }
        if let Some(v) = args.parallel {
            s.max_parallel_agents = Some(v);
        }
        if let Some(ref v) = args.pipeline {
            s.default_pipeline = Some(v.clone());
        }
        if let Some(v) = args.budget {
            s.default_budget_usd = Some(v);
        }
        if let Some(ref v) = args.permission_mode {
            s.default_permission_mode = Some(v.clone());
        }
        s
    };

    orchestrator::update_project_settings(&ctx.project_root, &row.id, &settings)?;

    let text = format!(
        "Project settings updated:\n  provider:        {}\n  project_key:     {}\n  parallel:        {}\n  pipeline:        {}\n  budget_usd:      {}\n  permission_mode: {}",
        settings.default_provider.as_deref().unwrap_or("(inherit)"),
        settings
            .default_project_key
            .as_deref()
            .unwrap_or("(inherit)"),
        settings
            .max_parallel_agents
            .map(|n| n.to_string())
            .as_deref()
            .unwrap_or("(inherit)")
            .to_string(),
        settings.default_pipeline.as_deref().unwrap_or("(inherit)"),
        settings
            .default_budget_usd
            .map(|n| format!("${n:.2}"))
            .as_deref()
            .unwrap_or("(inherit)")
            .to_string(),
        settings
            .default_permission_mode
            .as_deref()
            .unwrap_or("(inherit)"),
    );

    let json_val = json!({
        "id": row.id,
        "settings": {
            "default_provider": settings.default_provider,
            "default_project_key": settings.default_project_key,
            "max_parallel_agents": settings.max_parallel_agents,
            "default_pipeline": settings.default_pipeline,
            "default_budget_usd": settings.default_budget_usd,
            "default_permission_mode": settings.default_permission_mode,
        }
    });

    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn resolve_project_row(ctx: &CommandContext, id: Option<&str>) -> Result<ProjectRow> {
    if let Some(id) = id {
        let projects = orchestrator::list_projects(&ctx.project_root)?;
        let row = projects
            .into_iter()
            .find(|project| project.id == id)
            .ok_or_else(|| anyhow::anyhow!("project {id} not found"))?;
        return Ok(row);
    }
    Ok(orchestrator::get_project(&ctx.project_root)?)
}

fn handle_ssh_shell(ctx: &CommandContext, id: Option<&str>) -> Result<CommandOutput> {
    let row = resolve_project_row(ctx, id)?;
    if row.source_kind != "ssh" {
        anyhow::bail!("project {} is not an SSH project", row.id);
    }
    let details = row
        .source_details
        .clone()
        .ok_or_else(|| anyhow::anyhow!("SSH project metadata is missing"))?;
    let host = details
        .ssh_host
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("SSH host is missing"))?;
    let remote_path = details
        .ssh_remote_path
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("SSH remote path is missing"))?;

    let target = match details.ssh_user.as_deref() {
        Some(user) if !user.is_empty() => format!("{user}@{host}"),
        _ => host.to_string(),
    };

    let mut cmd = Command::new("ssh");
    if let Some(port) = details.ssh_port {
        cmd.arg("-p").arg(port.to_string());
    }
    cmd.arg("-t")
        .arg(&target)
        .arg(format!(
            "cd {} && exec $SHELL -l",
            shell_escape(remote_path)
        ))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd.status()?;
    let text = format!("SSH shell exited with status {}", status);
    let json_val = json!({
        "id": row.id,
        "ssh_target": target,
        "remote_path": remote_path,
        "status": status.code(),
        "success": status.success(),
    });
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn shell_escape(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "/._-".contains(ch))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn handle_archive(ctx: &CommandContext, id: Option<&str>) -> Result<CommandOutput> {
    let project_id = match id {
        Some(i) => i.to_string(),
        None => orchestrator::get_project(&ctx.project_root)?.id,
    };
    orchestrator::archive_project(&ctx.project_root, &project_id)?;
    let text = format!("Project {project_id} archived.");
    let json_val = json!({ "id": project_id, "state": "archived" });
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_delete(ctx: &CommandContext, id: Option<&str>) -> Result<CommandOutput> {
    let project_id = match id {
        Some(i) => i.to_string(),
        None => orchestrator::get_project(&ctx.project_root)?.id,
    };
    orchestrator::delete_project(&ctx.project_root, &project_id)?;
    let text = format!("Project {project_id} deleted.");
    let json_val = json!({ "id": project_id, "deleted": true });
    Ok(to_text_or_json(ctx.format, text, json_val))
}
