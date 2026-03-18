use anyhow::Result;

use crate::cli::{ConnectAction, ConnectArgs};
use crate::command_context::CommandContext;

use super::CommandOutput;
use grove_core::tracker::credentials::CredentialStorage;

pub fn handle(ctx: &CommandContext, args: &ConnectArgs) -> Result<CommandOutput> {
    match &args.action {
        ConnectAction::Github(ga) => handle_github(ctx, ga),
        ConnectAction::Jira(ja) => handle_jira(ctx, ja),
        ConnectAction::Linear(la) => handle_linear(la),
        ConnectAction::Status => handle_status(ctx),
        ConnectAction::Disconnect(da) => handle_disconnect(da),
    }
}

fn handle_github(
    ctx: &CommandContext,
    args: &crate::cli::ConnectGithubArgs,
) -> Result<CommandOutput> {
    let cfg = grove_core::config::loader::load_config(&ctx.project_root)?;
    let tracker =
        grove_core::tracker::github::GitHubTracker::new(&ctx.project_root, &cfg.tracker.github);

    if let Some(ref token) = args.token {
        tracker.authenticate(token)?;
        eprintln!("[grove] GitHub authenticated via token");
    }

    let status = tracker.check_connection();
    let text = if status.connected {
        format!(
            "GitHub: connected ({})",
            status.user_display.as_deref().unwrap_or("authenticated")
        )
    } else {
        format!(
            "GitHub: not connected — {}",
            status
                .error
                .as_deref()
                .unwrap_or("run `grove connect github --token <PAT>`")
        )
    };

    let json = serde_json::to_value(&status)?;
    Ok(CommandOutput { text, json })
}

fn handle_jira(_ctx: &CommandContext, args: &crate::cli::ConnectJiraArgs) -> Result<CommandOutput> {
    let config = grove_core::config::JiraTrackerConfig {
        site_url: args.site.clone(),
        email: args.email.clone(),
        ..Default::default()
    };

    let tracker = grove_core::tracker::jira::JiraTracker::new(&config);
    tracker.save_credentials(&args.email, &args.token, CredentialStorage::Keychain)?;

    let status = tracker.check_connection();
    let text = format!(
        "Jira: connected to {} ({})",
        args.site,
        status.user_display.as_deref().unwrap_or(&args.email)
    );

    let json = serde_json::to_value(&status)?;
    Ok(CommandOutput { text, json })
}

fn handle_linear(args: &crate::cli::ConnectLinearArgs) -> Result<CommandOutput> {
    let config = grove_core::config::LinearTrackerConfig::default();
    let tracker = grove_core::tracker::linear::LinearTracker::new(&config);
    tracker.save_token(&args.token, CredentialStorage::Keychain)?;

    let status = tracker.check_connection();
    let text = format!(
        "Linear: connected ({})",
        status.user_display.as_deref().unwrap_or("authenticated")
    );

    let json = serde_json::to_value(&status)?;
    Ok(CommandOutput { text, json })
}

fn handle_status(ctx: &CommandContext) -> Result<CommandOutput> {
    let cfg = grove_core::config::loader::load_config(&ctx.project_root)?;
    let mut lines = Vec::new();
    let mut statuses = Vec::new();

    // GitHub
    let gh =
        grove_core::tracker::github::GitHubTracker::new(&ctx.project_root, &cfg.tracker.github);
    let gh_status = gh.check_connection();
    lines.push(format_status_line(&gh_status));
    statuses.push(serde_json::to_value(&gh_status)?);

    // Jira
    let jira = grove_core::tracker::jira::JiraTracker::new(&cfg.tracker.jira);
    let jira_status = jira.check_connection();
    lines.push(format_status_line(&jira_status));
    statuses.push(serde_json::to_value(&jira_status)?);

    // Linear
    let linear = grove_core::tracker::linear::LinearTracker::new(&cfg.tracker.linear);
    let linear_status = linear.check_connection();
    lines.push(format_status_line(&linear_status));
    statuses.push(serde_json::to_value(&linear_status)?);

    let text = lines.join("\n");
    let json = serde_json::Value::Array(statuses);
    Ok(CommandOutput { text, json })
}

fn handle_disconnect(args: &crate::cli::ConnectDisconnectArgs) -> Result<CommandOutput> {
    match args.provider.to_lowercase().as_str() {
        "github" => {
            grove_core::tracker::credentials::CredentialStore::delete("github", "oauth-token")?;
            Ok(CommandOutput {
                text: "GitHub: disconnected".into(),
                json: serde_json::json!({"provider": "github", "disconnected": true}),
            })
        }
        "jira" => {
            grove_core::tracker::credentials::CredentialStore::delete("jira", "api-token")?;
            grove_core::tracker::credentials::CredentialStore::delete("jira", "email")?;
            Ok(CommandOutput {
                text: "Jira: disconnected".into(),
                json: serde_json::json!({"provider": "jira", "disconnected": true}),
            })
        }
        "linear" => {
            grove_core::tracker::credentials::CredentialStore::delete("linear", "api-token")?;
            Ok(CommandOutput {
                text: "Linear: disconnected".into(),
                json: serde_json::json!({"provider": "linear", "disconnected": true}),
            })
        }
        other => Err(anyhow::anyhow!(
            "unknown provider '{other}' — use github, jira, or linear"
        )),
    }
}

fn format_status_line(status: &grove_core::tracker::credentials::ConnectionStatus) -> String {
    let icon = if status.connected { "+" } else { "-" };
    let detail = if status.connected {
        status
            .user_display
            .as_deref()
            .unwrap_or("connected")
            .to_string()
    } else {
        status
            .error
            .as_deref()
            .unwrap_or("not connected")
            .to_string()
    };
    format!("  [{icon}] {}: {detail}", status.provider)
}
