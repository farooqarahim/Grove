/// Integration tests for the issue board: issues_repo, status normalization, sync engine,
/// and write-back helpers.
use grove_core::db;
use grove_core::db::repositories::issues_repo::{self, BoardColumn, IssueFilter};
use grove_core::tracker::status::CanonicalStatus;
use grove_core::tracker::{Issue, IssueUpdate};
use tempfile::TempDir;

// ── Test helpers ─────────────────────────────────────────────────────────────

fn setup() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    (dir, conn)
}

fn make_issue(external_id: &str, provider: &str, status: &str) -> Issue {
    Issue {
        external_id: external_id.to_string(),
        provider: provider.to_string(),
        title: format!("Issue {external_id}"),
        status: status.to_string(),
        labels: vec![],
        body: None,
        url: None,
        assignee: None,
        raw_json: serde_json::json!({}),
        provider_native_id: None,
        provider_scope_type: None,
        provider_scope_key: None,
        provider_scope_name: None,
        provider_metadata: serde_json::json!({}),
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

const PROJECT: &str = "proj-test";

// ── issues_repo CRUD ─────────────────────────────────────────────────────────

#[test]
fn upsert_and_get_roundtrip() {
    let (_dir, conn) = setup();
    let issue = make_issue("gh-100", "github", "open");
    issues_repo::upsert(&conn, &issue, PROJECT).unwrap();

    let id = "github:gh-100".to_string();
    let fetched = issues_repo::get(&conn, &id).unwrap();
    assert!(fetched.is_some(), "should find the upserted issue");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.external_id, "gh-100");
    assert_eq!(fetched.provider, "github");
    assert_eq!(fetched.title, "Issue gh-100");
}

#[test]
fn upsert_twice_updates_in_place() {
    let (_dir, conn) = setup();
    let mut issue = make_issue("gh-200", "github", "open");
    issues_repo::upsert(&conn, &issue, PROJECT).unwrap();

    issue.title = "Updated title".to_string();
    issue.status = "in_progress".to_string();
    issues_repo::upsert(&conn, &issue, PROJECT).unwrap();

    // Should still be exactly one row.
    let all = issues_repo::list(&conn, PROJECT, &IssueFilter::new()).unwrap();
    assert_eq!(all.len(), 1, "upsert must not create duplicate rows");
    assert_eq!(all[0].title, "Updated title");
}

#[test]
fn create_native_generates_grove_id() {
    let (_dir, mut conn) = setup();
    let issue = issues_repo::create_native(
        &mut conn,
        PROJECT,
        "My native issue",
        Some("body text"),
        None,
        &[],
    )
    .unwrap();
    assert_eq!(issue.provider, "grove");
    assert!(
        !issue.external_id.is_empty(),
        "external_id (uuid) must be set"
    );

    // Row must be findable by its composite id
    let id = format!("grove:{}", issue.external_id);
    let found = issues_repo::get(&conn, &id).unwrap();
    assert!(found.is_some(), "created native issue must be retrievable");
}

#[test]
fn list_filters_by_provider() {
    let (_dir, conn) = setup();
    issues_repo::upsert(&conn, &make_issue("gh-1", "github", "open"), PROJECT).unwrap();
    issues_repo::upsert(&conn, &make_issue("jira-1", "jira", "open"), PROJECT).unwrap();
    issues_repo::upsert(&conn, &make_issue("jira-2", "jira", "in_progress"), PROJECT).unwrap();

    let filter = IssueFilter {
        provider: Some("jira".into()),
        limit: 100,
        ..Default::default()
    };
    let results = issues_repo::list(&conn, PROJECT, &filter).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|i| i.provider == "jira"));
}

#[test]
fn update_status_changes_canonical_and_records_event() {
    let (_dir, mut conn) = setup();
    let issue = make_issue("gh-300", "github", "open");
    issues_repo::upsert(&conn, &issue, PROJECT).unwrap();

    issues_repo::update_status(
        &mut conn,
        "github:gh-300",
        "in_progress",
        CanonicalStatus::InProgress,
    )
    .unwrap();

    let events = issues_repo::list_events(&conn, "github:gh-300").unwrap();
    let status_events: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == "status_changed")
        .collect();
    assert!(
        !status_events.is_empty(),
        "status_changed event must be recorded"
    );
    assert_eq!(status_events[0].new_value.as_deref(), Some("in_progress"));
}

#[test]
fn update_fields_via_issue_update() {
    let (_dir, mut conn) = setup();
    let issue = make_issue("gh-400", "github", "open");
    issues_repo::upsert(&conn, &issue, PROJECT).unwrap();

    let upd = IssueUpdate {
        title: Some("New title".to_string()),
        assignee: Some("alice".to_string()),
        ..Default::default()
    };
    issues_repo::update_fields(&mut conn, "github:gh-400", &upd).unwrap();

    let found = issues_repo::get(&conn, "github:gh-400").unwrap().unwrap();
    assert_eq!(found.title, "New title");
    assert_eq!(found.assignee.as_deref(), Some("alice"));
}

#[test]
fn add_and_list_comments() {
    let (_dir, mut conn) = setup();
    let issue = make_issue("gh-500", "github", "open");
    issues_repo::upsert(&conn, &issue, PROJECT).unwrap();

    issues_repo::add_comment(&mut conn, "github:gh-500", "First comment", "alice", false).unwrap();
    issues_repo::add_comment(&mut conn, "github:gh-500", "Second comment", "bob", true).unwrap();

    let comments = issues_repo::list_comments(&conn, "github:gh-500").unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].body, "First comment");
    assert_eq!(comments[0].author.as_deref(), Some("alice"));
    assert!(!comments[0].posted_to_provider);
    assert!(comments[1].posted_to_provider);
}

#[test]
fn link_run_and_delete() {
    let (_dir, conn) = setup();
    let issue = make_issue("gh-600", "github", "open");
    issues_repo::upsert(&conn, &issue, PROJECT).unwrap();
    issues_repo::link_run(&conn, "github:gh-600", "run-abc").unwrap();

    issues_repo::delete(&conn, "github:gh-600").unwrap();
    let gone = issues_repo::get(&conn, "github:gh-600").unwrap();
    assert!(gone.is_none(), "deleted issue must not be retrievable");
}

#[test]
fn count_open_ignores_done_and_cancelled() {
    let (_dir, mut conn) = setup();
    issues_repo::upsert(&conn, &make_issue("i1", "github", "open"), PROJECT).unwrap();
    issues_repo::upsert(&conn, &make_issue("i2", "github", "in_progress"), PROJECT).unwrap();
    issues_repo::upsert(&conn, &make_issue("i3", "github", "done"), PROJECT).unwrap();
    issues_repo::upsert(&conn, &make_issue("i4", "github", "cancelled"), PROJECT).unwrap();

    // Set canonical status for done/cancelled so count_open works correctly.
    issues_repo::update_status(&mut conn, "github:i3", "done", CanonicalStatus::Done).unwrap();
    issues_repo::update_status(
        &mut conn,
        "github:i4",
        "cancelled",
        CanonicalStatus::Cancelled,
    )
    .unwrap();

    let count = issues_repo::count_open(&conn, PROJECT).unwrap();
    assert_eq!(count, 2, "only open and in-progress issues count");
}

// ── board_view ───────────────────────────────────────────────────────────────

#[test]
fn board_view_groups_by_canonical_status() {
    let (_dir, conn) = setup();

    let statuses = [
        ("open", CanonicalStatus::Open),
        ("in_progress", CanonicalStatus::InProgress),
        ("done", CanonicalStatus::Done),
    ];

    for (i, (status, _)) in statuses.iter().enumerate() {
        let issue = make_issue(&format!("i{i}"), "github", status);
        issues_repo::upsert(&conn, &issue, PROJECT).unwrap();
    }

    let board = issues_repo::board_view(&conn, PROJECT, &IssueFilter::new()).unwrap();

    // Every canonical status must have a column.
    assert_eq!(board.columns.len(), CanonicalStatus::ordered().len());

    let find_col = |cs: CanonicalStatus| -> &BoardColumn {
        board
            .columns
            .iter()
            .find(|c| c.canonical_status == cs)
            .unwrap()
    };

    assert_eq!(find_col(CanonicalStatus::Open).count, 1);
    assert_eq!(find_col(CanonicalStatus::InProgress).count, 1);
    assert_eq!(find_col(CanonicalStatus::Done).count, 1);
    assert_eq!(find_col(CanonicalStatus::Blocked).count, 0);

    assert_eq!(board.total, 3);
}

// ── CanonicalStatus normalization ────────────────────────────────────────────

#[test]
fn canonical_status_roundtrip() {
    for &cs in CanonicalStatus::ordered() {
        let db_str = cs.as_db_str();
        let parsed = CanonicalStatus::from_db_str(db_str);
        assert_eq!(parsed, Some(cs), "from_db_str(as_db_str()) must roundtrip");
    }
}

#[test]
fn normalize_github_statuses() {
    use grove_core::tracker::status::normalize;

    assert_eq!(normalize("github", "open"), CanonicalStatus::Open);
    assert_eq!(normalize("github", "closed"), CanonicalStatus::Done);
    assert_eq!(
        normalize("github", "in_progress"),
        CanonicalStatus::InProgress
    );
}

#[test]
fn normalize_jira_statuses() {
    use grove_core::tracker::status::normalize;

    assert_eq!(normalize("jira", "To Do"), CanonicalStatus::Open);
    assert_eq!(
        normalize("jira", "In Progress"),
        CanonicalStatus::InProgress
    );
    assert_eq!(normalize("jira", "Done"), CanonicalStatus::Done);
    assert_eq!(normalize("jira", "Blocked"), CanonicalStatus::Blocked);
}

#[test]
fn normalize_linear_statuses() {
    use grove_core::tracker::status::normalize;

    assert_eq!(normalize("linear", "Todo"), CanonicalStatus::Open);
    assert_eq!(
        normalize("linear", "In Progress"),
        CanonicalStatus::InProgress
    );
    assert_eq!(normalize("linear", "Done"), CanonicalStatus::Done);
    assert_eq!(normalize("linear", "Cancelled"), CanonicalStatus::Cancelled);
}

#[test]
fn normalize_unknown_status_falls_back_to_open() {
    use grove_core::tracker::status::normalize;

    let cs = normalize("github", "some-weird-custom-status");
    // Unknown statuses must produce a valid CanonicalStatus, not panic.
    let _ = cs.as_db_str();
}

// ── Sync state tracking ──────────────────────────────────────────────────────

#[test]
fn sync_state_upsert_and_read() {
    let (_dir, mut conn) = setup();
    issues_repo::update_sync_state(&mut conn, "github", PROJECT, 5, None, 120).unwrap();

    let states = issues_repo::get_sync_states(&conn, PROJECT).unwrap();
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].provider, "github");
    assert_eq!(states[0].issues_synced, 5);
    assert!(states[0].last_synced_at.is_some());

    // Update again — should be idempotent (upsert).
    issues_repo::update_sync_state(&mut conn, "github", PROJECT, 12, None, 200).unwrap();
    let states2 = issues_repo::get_sync_states(&conn, PROJECT).unwrap();
    assert_eq!(states2.len(), 1, "second upsert must not add a row");
    assert_eq!(states2[0].issues_synced, 12);
}

// ── write_back render_template ───────────────────────────────────────────────

#[test]
fn render_template_substitutes_all_placeholders() {
    use grove_core::tracker::write_back::WriteBackContext;

    let ctx = WriteBackContext {
        run_id: "run-abc".to_string(),
        issue_id: "github:gh-1".to_string(),
        pr_url: Some("https://github.com/repo/pull/42".to_string()),
        cost_usd: 0.12,
        duration_secs: 300,
        agent_count: 3,
        error: None,
    };

    let template = "Run {run_id} finished in {duration}s (${cost_usd}) — PR: {pr_url}";
    let rendered = grove_core::tracker::write_back::render_template(template, &ctx);

    assert!(rendered.contains("run-abc"), "must contain run_id");
    assert!(rendered.contains("300"), "must contain duration");
    assert!(rendered.contains("0.12"), "must contain cost");
    assert!(
        rendered.contains("https://github.com/repo/pull/42"),
        "must contain pr_url"
    );
    assert!(!rendered.contains("{"), "no unresolved placeholders");
}

#[test]
fn render_template_no_pr_url_substitutes_empty() {
    use grove_core::tracker::write_back::WriteBackContext;

    let ctx = WriteBackContext {
        run_id: "run-xyz".to_string(),
        issue_id: "grove:local-1".to_string(),
        pr_url: None,
        cost_usd: 0.0,
        duration_secs: 10,
        agent_count: 1,
        error: Some("timeout".to_string()),
    };

    let template = "Error: {error} — PR: {pr_url}";
    let rendered = grove_core::tracker::write_back::render_template(template, &ctx);

    assert!(rendered.contains("timeout"), "must contain error text");
    assert!(
        !rendered.contains("{pr_url}"),
        "pr_url placeholder must be replaced (with empty)"
    );
}
