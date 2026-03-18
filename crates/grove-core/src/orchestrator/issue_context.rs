use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::db::repositories::messages_repo::{self, MessageRow};
use crate::errors::GroveResult;
use crate::tracker::Issue;

/// Format an Issue into rich context text suitable for agent consumption.
///
/// Produces a structured block with all available metadata so the agent
/// understands the full problem description, not just an ID.
pub fn format_issue_context(issue: &Issue) -> String {
    let mut lines = Vec::new();

    lines.push("---".to_string());
    lines.push(format!(
        "Linked Issue: {} - {}",
        issue.external_id, issue.title
    ));

    let mut meta_parts = vec![format!("Provider: {}", issue.provider)];
    meta_parts.push(format!("Status: {}", issue.status));
    if let Some(ref assignee) = issue.assignee {
        meta_parts.push(format!("Assignee: {assignee}"));
    }
    lines.push(meta_parts.join(" | "));

    if let Some(ref url) = issue.url {
        lines.push(format!("URL: {url}"));
    }

    if !issue.labels.is_empty() {
        lines.push(format!("Labels: {}", issue.labels.join(", ")));
    }

    if let Some(ref body) = issue.body {
        let trimmed = body.trim();
        if !trimmed.is_empty() {
            lines.push(String::new());
            lines.push("Description:".to_string());
            lines.push(trimmed.to_string());
        }
    }

    lines.push("---".to_string());
    lines.join("\n")
}

/// Build an enriched objective that prepends issue context to the user's prompt.
///
/// If `user_objective` is empty, the objective is simply "Fix the following issue"
/// with the full issue context. Otherwise the user's instructions are appended.
pub fn enrich_objective(issue: &Issue, user_objective: &str) -> String {
    let context = format_issue_context(issue);
    let trimmed = user_objective.trim();

    if trimmed.is_empty() {
        format!("Fix the following issue:\n\n{context}")
    } else {
        format!("Fix the following issue:\n\n{context}\n\nAdditional instructions: {trimmed}")
    }
}

/// Record issue context as a "system" role message in the conversation.
///
/// This seeds the conversation with the full issue details so that agents
/// see the problem description as part of the conversation history.
pub fn seed_issue_context(
    conn: &mut Connection,
    conversation_id: &str,
    run_id: &str,
    issue: &Issue,
) -> GroveResult<()> {
    let context = format_issue_context(issue);
    let msg = MessageRow {
        id: format!("msg_{}", Uuid::new_v4().simple()),
        conversation_id: conversation_id.to_string(),
        run_id: Some(run_id.to_string()),
        role: "system".to_string(),
        agent_type: None,
        session_id: None,
        content: format!("[Issue Context]\n\n{context}"),
        created_at: Utc::now().to_rfc3339(),
        user_id: None,
    };
    messages_repo::insert(conn, &msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn full_issue() -> Issue {
        Issue {
            external_id: "PROJ-123".into(),
            provider: "jira".into(),
            title: "Fix authentication timeout".into(),
            status: "In Progress".into(),
            labels: vec!["bug".into(), "high-priority".into()],
            body: Some("Login fails after 30 seconds of inactivity.\n\nSteps to reproduce:\n1. Open app\n2. Wait 30s\n3. Try to navigate".into()),
            url: Some("https://mycompany.atlassian.net/browse/PROJ-123".into()),
            assignee: Some("John Doe".into()),
            raw_json: json!({}),
            provider_native_id: Some("100123".into()),
            provider_scope_type: Some("project".into()),
            provider_scope_key: Some("PROJ".into()),
            provider_scope_name: Some("Project Alpha".into()),
            provider_metadata: json!({"issue_type": "Bug"}),
            id: None, project_id: None, canonical_status: None, priority: None,
            is_native: false, created_at: None, updated_at: None, synced_at: None,
            run_id: None,
        }
    }

    fn minimal_issue() -> Issue {
        Issue {
            external_id: "42".into(),
            provider: "github".into(),
            title: "Button doesn't work".into(),
            status: "open".into(),
            labels: vec![],
            body: None,
            url: None,
            assignee: None,
            raw_json: json!({}),
            provider_native_id: None,
            provider_scope_type: None,
            provider_scope_key: None,
            provider_scope_name: None,
            provider_metadata: json!({}),
            id: None,
            project_id: None,
            canonical_status: None,
            priority: None,
            is_native: false,
            created_at: None,
            updated_at: None,
            synced_at: None,
            run_id: None,
        }
    }

    #[test]
    fn format_issue_context_all_fields() {
        let ctx = format_issue_context(&full_issue());
        assert!(ctx.contains("Linked Issue: PROJ-123"));
        assert!(ctx.contains("Fix authentication timeout"));
        assert!(ctx.contains("Provider: jira"));
        assert!(ctx.contains("Status: In Progress"));
        assert!(ctx.contains("Assignee: John Doe"));
        assert!(ctx.contains("https://mycompany.atlassian.net/browse/PROJ-123"));
        assert!(ctx.contains("Labels: bug, high-priority"));
        assert!(ctx.contains("Login fails after 30 seconds"));
        assert!(ctx.starts_with("---"));
        assert!(ctx.ends_with("---"));
    }

    #[test]
    fn format_issue_context_minimal() {
        let ctx = format_issue_context(&minimal_issue());
        assert!(ctx.contains("Linked Issue: 42"));
        assert!(ctx.contains("Button doesn't work"));
        assert!(ctx.contains("Provider: github"));
        assert!(!ctx.contains("Assignee:"));
        assert!(!ctx.contains("URL:"));
        assert!(!ctx.contains("Labels:"));
        assert!(!ctx.contains("Description:"));
    }

    #[test]
    fn enrich_objective_with_prompt() {
        let enriched = enrich_objective(&full_issue(), "Also add unit tests");
        assert!(enriched.starts_with("Fix the following issue:"));
        assert!(enriched.contains("PROJ-123"));
        assert!(enriched.contains("Additional instructions: Also add unit tests"));
    }

    #[test]
    fn enrich_objective_without_prompt() {
        let enriched = enrich_objective(&full_issue(), "");
        assert!(enriched.starts_with("Fix the following issue:"));
        assert!(enriched.contains("PROJ-123"));
        assert!(!enriched.contains("Additional instructions"));
    }

    #[test]
    fn enrich_objective_whitespace_only_prompt() {
        let enriched = enrich_objective(&minimal_issue(), "   ");
        assert!(!enriched.contains("Additional instructions"));
    }

    #[test]
    fn seed_issue_context_records_message() {
        let dir = tempfile::TempDir::new().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        let mut conn = crate::db::DbHandle::new(dir.path()).connect().unwrap();

        // Create workspace + project + conversation
        let conv_id = super::super::conversation::resolve_conversation(
            &mut conn,
            dir.path(),
            None,
            false,
            None,
            None,
            super::super::conversation::RUN_CONVERSATION_KIND,
        )
        .unwrap();

        // Create a run
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at, conversation_id)
             VALUES ('run_test', 'test', 'created', 1.0, 0.0, ?1, ?1, ?2)",
            rusqlite::params![now, conv_id],
        )
        .unwrap();

        seed_issue_context(&mut conn, &conv_id, "run_test", &full_issue()).unwrap();

        let msgs = messages_repo::list_for_conversation(&conn, &conv_id, 100).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "system");
        assert!(msgs[0].content.contains("[Issue Context]"));
        assert!(msgs[0].content.contains("PROJ-123"));
        assert!(msgs[0].content.contains("Fix authentication timeout"));
    }
}
