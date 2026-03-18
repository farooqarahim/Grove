use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use chrono::Utc;
use rusqlite::{Connection, TransactionBehavior, params};
use serde_json::json;
use uuid::Uuid;

use crate::agents::{AgentState, AgentType};
use crate::budget::policy::{self as budget_policy, BudgetStatus};
use crate::checkpoint::{self, BudgetSnapshot, CheckpointPayload};
use crate::config::GroveConfig;
use crate::errors::{GroveError, GroveResult};
use crate::events;
use crate::providers::{Provider, ProviderRequest, StreamSink, budget_meter};
use crate::worktree;
use crate::worktree::gitignore::GitignoreFilter;
use std::sync::Arc;

use super::task_decomposer::{self, TaskDecomposition, TaskSpec};
use super::transitions;
use super::verdict::{self, JudgeVerdict, ReviewVerdict};
use super::{GrovePlanStep, PlanStep, RunState, plan_steps_repo, spawn};
use crate::db::repositories::{
    ownership_repo, phase_checkpoints_repo, run_artifacts_repo, runs_repo,
};

// ── DB path resolution ─────────────────────────────────────────────────────

/// Resolve the actual DB file path from a live connection.
///
/// Uses `PRAGMA database_list` to get the path SQLite has open, falling back
/// to `config::db_path` if the pragma fails. This is the single source of
/// truth for background threads that need their own DB connection.
fn resolve_db_path(conn: &rusqlite::Connection, project_root: &Path) -> PathBuf {
    conn.query_row("PRAGMA database_list", [], |r| r.get::<_, String>(2))
        .map(PathBuf::from)
        .unwrap_or_else(|_| crate::config::db_path(project_root))
}

// ── Self-heartbeat ─────────────────────────────────────────────────────────

/// How often the heartbeat thread wakes and updates `sessions.last_heartbeat`.
/// 30 s is well under the default `stale_threshold_secs` (300 s).
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// RAII guard that drives a per-session self-heartbeat during `provider.execute`.
///
/// A background thread wakes every [`HEARTBEAT_INTERVAL_SECS`] and calls
/// [`crate::watchdog::touch_heartbeat`] on its own DB connection.  The thread
/// stops atomically when this guard is dropped (i.e., when `execute` returns).
struct HeartbeatGuard {
    stop: Arc<AtomicBool>,
}

impl Drop for HeartbeatGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Maximum number of times to retry opening the DB connection before giving up.
const HEARTBEAT_MAX_CONNECT_RETRIES: u32 = 3;

/// Backoff durations for each retry attempt (5s, 10s, 20s).
const HEARTBEAT_RETRY_BACKOFFS: [u64; 3] = [5, 10, 20];

/// Try to open a DB connection with retry and backoff.
///
/// Ensures the parent directory exists before each attempt (handles the case
/// where the directory was removed after initial DB creation). Returns `None`
/// only after all retries are exhausted.
fn open_heartbeat_connection(
    db_path: &Path,
    session_id: &str,
    stop: &AtomicBool,
) -> Option<rusqlite::Connection> {
    for attempt in 0..=HEARTBEAT_MAX_CONNECT_RETRIES {
        // Ensure the parent directory exists — SQLite cannot create the file
        // if the directory is missing (returns SQLITE_CANTOPEN).
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let handle = crate::db::DbHandle::from_db_path(db_path.to_path_buf());
        match handle.connect() {
            Ok(conn) => return Some(conn),
            Err(e) => {
                if attempt < HEARTBEAT_MAX_CONNECT_RETRIES {
                    let backoff = HEARTBEAT_RETRY_BACKOFFS[attempt as usize];
                    tracing::warn!(
                        error = %e,
                        session_id = %session_id,
                        attempt = attempt + 1,
                        max_retries = HEARTBEAT_MAX_CONNECT_RETRIES,
                        backoff_secs = backoff,
                        "heartbeat thread: DB open failed — retrying"
                    );
                    // Sleep in short increments so we can bail quickly on stop signal.
                    for _ in 0..backoff {
                        if stop.load(Ordering::Relaxed) {
                            return None;
                        }
                        std::thread::sleep(Duration::from_secs(1));
                    }
                } else {
                    tracing::error!(
                        error = %e,
                        session_id = %session_id,
                        db_path = %db_path.display(),
                        "heartbeat thread: DB open failed after all retries — \
                         session will have no heartbeat and may be falsely reaped by watchdog"
                    );
                }
            }
        }
    }
    None
}

fn spawn_heartbeat_guard(db_path: PathBuf, session_id: String) -> HeartbeatGuard {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);
    std::thread::spawn(move || {
        let mut conn = match open_heartbeat_connection(&db_path, &session_id, &stop_clone) {
            Some(c) => c,
            None => return,
        };
        loop {
            std::thread::sleep(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
            if stop_clone.load(Ordering::Relaxed) {
                break;
            }
            if let Err(e) = crate::watchdog::touch_heartbeat(&conn, &session_id) {
                tracing::warn!(
                    error = %e,
                    session_id = %session_id,
                    "heartbeat touch failed — attempting reconnect"
                );
                // Connection may have gone stale; try to reconnect once.
                match open_heartbeat_connection(&db_path, &session_id, &stop_clone) {
                    Some(new_conn) => {
                        conn = new_conn;
                        tracing::info!(
                            session_id = %session_id,
                            "heartbeat thread: reconnected to DB"
                        );
                    }
                    None => {
                        tracing::error!(
                            session_id = %session_id,
                            "heartbeat thread: reconnect failed — stopping heartbeat"
                        );
                        break;
                    }
                }
            }
        }
    });
    HeartbeatGuard { stop }
}

// ── End self-heartbeat ─────────────────────────────────────────────────────

/// Run each stage in `plan` in sequence.
///
/// Each stage is a `Vec<AgentType>`:
/// - Single-element stages run sequentially with the passed `provider`.
/// - Multi-element stages fork the current worktree, run all agents in parallel
///   threads, then merge their outputs.
///
/// Returns `Ok(())` when every stage completes. Returns `Err` on unrecoverable
/// errors. A budget hard-stop writes the run to `Failed` and returns
/// `Err(Runtime("budget exceeded"))`.
#[allow(clippy::too_many_arguments)]
pub fn run_agents(
    conn: &mut Connection,
    run_id: &str,
    objective: &str,
    plan: &[Vec<AgentType>],
    provider: Arc<dyn Provider>,
    cfg: &GroveConfig,
    project_root: &Path,
    model: Option<&str>,
    shared_execution_context: Option<&str>,
    agent_briefs: Option<&HashMap<AgentType, String>>,
    interactive: bool,
    pause_after: &[AgentType],
    plan_steps: Option<&[PlanStep]>,
    conversation_id: Option<&str>,
    abort_handle: Option<&super::abort_handle::AbortHandle>,
    initial_provider_session_id: Option<String>,
    sink: &dyn StreamSink,
    input_handle_callback: Option<
        &std::sync::Arc<dyn Fn(crate::providers::agent_input::AgentInputHandle) + Send + Sync>,
    >,
    run_control_callback: Option<
        &std::sync::Arc<
            dyn Fn(String, crate::providers::claude_code_persistent::PersistentRunControlHandle)
                + Send
                + Sync,
        >,
    >,
) -> GroveResult<()> {
    // Per-conversation worktree: get or create the conversation's persistent workspace.
    // Idempotent — safe to call on every run start. The worktree persists until
    // the conversation is archived (see archive_conversation in mod.rs).
    let is_git_for_wt = worktree::git_ops::is_git_repo(project_root)
        && worktree::git_ops::has_commits(project_root);
    let conv_wt_path: Option<PathBuf> = if let Some(cid) = conversation_id {
        if is_git_for_wt {
            Some(worktree::conversation::ensure_conversation_worktree(
                project_root,
                cid,
                &cfg.worktree.branch_prefix,
            )?)
        } else {
            None
        }
    } else {
        None
    };

    run_agents_inner(
        conn,
        run_id,
        objective,
        plan,
        Arc::clone(&provider),
        cfg,
        project_root,
        model,
        shared_execution_context,
        agent_briefs,
        interactive,
        pause_after,
        plan_steps,
        conversation_id,
        abort_handle,
        conv_wt_path.as_deref(),
        initial_provider_session_id,
        sink,
        input_handle_callback,
        run_control_callback,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_agents_inner(
    conn: &mut Connection,
    run_id: &str,
    objective: &str,
    plan: &[Vec<AgentType>],
    provider: Arc<dyn Provider>,
    cfg: &GroveConfig,
    project_root: &Path,
    model: Option<&str>,
    shared_execution_context: Option<&str>,
    agent_briefs: Option<&HashMap<AgentType, String>>,
    interactive: bool,
    pause_after: &[AgentType],
    plan_steps: Option<&[PlanStep]>,
    conversation_id: Option<&str>,
    abort_handle: Option<&super::abort_handle::AbortHandle>,
    conv_wt_path: Option<&Path>,
    initial_provider_session_id: Option<String>,
    sink: &dyn StreamSink,
    input_handle_callback: Option<
        &std::sync::Arc<dyn Fn(crate::providers::agent_input::AgentInputHandle) + Send + Sync>,
    >,
    run_control_callback: Option<
        &std::sync::Arc<
            dyn Fn(String, crate::providers::claude_code_persistent::PersistentRunControlHandle)
                + Send
                + Sync,
        >,
    >,
) -> GroveResult<()> {
    // Ensure DB schema is up-to-date. When a non-"auto" provider is selected,
    // the provider builder path skips db::initialize, so new tables (like
    // pipeline_stages) may not exist yet.
    let _ = crate::db::initialize(project_root);

    // Load project configs once — used for instruction building and scope/allowed_tools
    // across all agents in this run, avoiding redundant disk reads per agent.
    let project_configs = crate::config::agent_config::load_all(project_root).ok();

    // Load .gitignore once — used for all worktree operations in this run.
    let _gitignore = GitignoreFilter::load(project_root);

    // Resolve the actual DB file path from the live connection. This is the
    // single source of truth used by both the watchdog and heartbeat threads,
    // avoiding path-recomputation bugs when project_root differs from the
    // centralized DB location.
    let db_path = resolve_db_path(conn, project_root);

    // Watchdog: spawn background thread to detect stalled/zombie agents.
    let _watchdog_tx = if cfg.watchdog.enabled {
        match crate::watchdog::spawn_watchdog(&db_path, run_id.to_string(), &cfg.watchdog) {
            Ok(tx) => Some(tx),
            Err(e) => {
                tracing::warn!(error = %e, "failed to start watchdog — continuing without");
                None
            }
        }
    } else {
        None
    };

    let is_git = worktree::git_ops::is_git_repo(project_root)
        && worktree::git_ops::has_commits(project_root);

    // §1.1 [2]-A: Fetch upstream before run so agents work on latest code.
    // Non-fatal — if offline or remote is unavailable we log a warning and continue.
    if is_git && cfg.worktree.fetch_before_run {
        if let Some((remote, branch)) = worktree::git_ops::git_resolve_tracking_branch(project_root)
        {
            if let Err(e) = worktree::git_ops::git_fetch_branch(project_root, &remote, &branch) {
                tracing::warn!(
                    remote = %remote,
                    branch = %branch,
                    error = %e,
                    "git fetch failed — continuing with local HEAD (offline mode)"
                );
            } else {
                tracing::info!(
                    remote = %remote,
                    branch = %branch,
                    "fetched upstream — agents will work on latest code"
                );
            }
        }
    }

    // Derive conversation branch name using the configured branch_prefix.
    let prefix = &cfg.worktree.branch_prefix;
    let conv_branch = conversation_id.map(|cid| worktree::paths::conv_branch_name_p(prefix, cid));

    // Lazy conversation branch creation: create grove/conv-<id> from
    // the project's default branch (or HEAD) if it doesn't already exist.
    // Done here rather than in resolve_conversation() to avoid creating
    // orphan branches for runs that are rejected by acquire_run_slot().
    if is_git {
        if let Some(ref cb) = conv_branch {
            let default_branch = &cfg.project.default_branch;
            let start_point = if worktree::git_ops::git_branch_exists(project_root, default_branch)
                .unwrap_or(false)
            {
                default_branch.as_str()
            } else {
                "HEAD"
            };
            worktree::git_ops::git_create_branch(project_root, cb, start_point)?;
            tracing::debug!(branch = %cb, "conversation branch ensured");

            if let Some(conv_id) = conversation_id {
                if let Err(err) = ensure_conversation_branch_registered(
                    conn,
                    cfg,
                    project_root,
                    run_id,
                    conv_id,
                    cb,
                ) {
                    tracing::warn!(
                        conversation_id = %conv_id,
                        branch = %cb,
                        error = %err,
                        "failed to persist conversation branch registration state"
                    );
                }
            }

            // Tag checkpoint and clean worktree BEFORE merge/sync so we have
            // a clean working tree for the merge attempt.
            if let Some(wt_path) = conv_wt_path {
                let prefix = &cfg.worktree.branch_prefix;
                let tag = format!("{prefix}/checkpoint-{run_id}");
                if let Err(e) = worktree::git_ops::run_git(wt_path, &["tag", "-f", &tag, "HEAD"]) {
                    tracing::warn!(run_id, error = %e, "pre-run checkpoint tag failed");
                }
                if let Err(e) = worktree::git_ops::run_git(wt_path, &["checkout", "."]) {
                    tracing::warn!(run_id, error = %e, "pre-run checkout clean failed — agent may see stale files");
                }
                if let Err(e) = worktree::git_ops::run_git(wt_path, &["clean", "-fd"]) {
                    tracing::warn!(run_id, error = %e, "pre-run clean failed — untracked files may remain");
                }
            }

            // 2.10: Sync conversation branch with default branch before run.
            // Strategy is controlled by `sync_before_run` config.
            use crate::config::SyncBeforeRun;

            match cfg.worktree.sync_before_run {
                SyncBeforeRun::Merge => {
                    if let Some(stale) = worktree::git_ops::git_detect_stale_base(
                        project_root,
                        cb,
                        &cfg.project.default_branch,
                    ) {
                        tracing::info!(
                            conv_branch = %cb,
                            default_branch = %cfg.project.default_branch,
                            commits_behind = stale.commits_behind,
                            merge_base = %stale.merge_base,
                            "conversation branch is stale — merging main"
                        );
                        let _ = events::emit(
                            conn,
                            run_id,
                            None,
                            "conv_branch_stale",
                            json!({
                                "conv_branch": cb,
                                "default_branch": cfg.project.default_branch,
                                "sync_strategy": "merge",
                                "commits_behind": stale.commits_behind,
                                "merge_base": stale.merge_base,
                                "upstream_head": stale.upstream_head,
                            }),
                        );

                        if let Some(wt_path) = conv_wt_path {
                            match super::merge_main_into_conv_branch(
                                wt_path,
                                &cfg.project.default_branch,
                            )? {
                                worktree::git_ops::MergeUpstreamOutcome::UpToDate => {
                                    tracing::debug!(
                                        conv_branch = %cb,
                                        "merge check: already up-to-date (race with stale detection)"
                                    );
                                }
                                worktree::git_ops::MergeUpstreamOutcome::Success {
                                    merge_commit_sha,
                                } => {
                                    tracing::info!(
                                        conv_branch = %cb,
                                        default_branch = %cfg.project.default_branch,
                                        merge_commit = %merge_commit_sha,
                                        "clean merge of main into conversation branch"
                                    );
                                    let _ = events::emit(
                                        conn,
                                        run_id,
                                        None,
                                        crate::events::event_types::PRE_RUN_MERGE_CLEAN,
                                        json!({
                                            "conversation_id": conversation_id,
                                            "conv_branch": cb,
                                            "default_branch": cfg.project.default_branch,
                                            "merge_commit_sha": merge_commit_sha,
                                        }),
                                    );
                                }
                                worktree::git_ops::MergeUpstreamOutcome::Conflict {
                                    conflicting_files,
                                } => {
                                    tracing::warn!(
                                        conv_branch = %cb,
                                        default_branch = %cfg.project.default_branch,
                                        file_count = conflicting_files.len(),
                                        files = %conflicting_files.join(", "),
                                        "merge conflict — invoking conflict resolution agent"
                                    );
                                    let _ = events::emit(
                                        conn,
                                        run_id,
                                        None,
                                        crate::events::event_types::PRE_RUN_MERGE_CONFLICT,
                                        json!({
                                            "conversation_id": conversation_id,
                                            "conv_branch": cb,
                                            "default_branch": cfg.project.default_branch,
                                            "conflicting_files": conflicting_files,
                                            "file_count": conflicting_files.len(),
                                        }),
                                    );

                                    // Invoke conflict resolution agent.
                                    let cr_instructions = build_conflict_resolution_instructions(
                                        &conflicting_files,
                                        &cfg.project.default_branch,
                                    );
                                    let cr_objective = format!(
                                        "Resolve {} merge conflict(s) with {}",
                                        conflicting_files.len(),
                                        cfg.project.default_branch,
                                    );
                                    let cr_timeout = agent_timeout_secs(AgentType::Builder, cfg);
                                    let cr_request = ProviderRequest {
                                        objective: cr_objective,
                                        role: "builder".to_string(),
                                        worktree_path: wt_path.to_string_lossy().to_string(),
                                        instructions: cr_instructions,
                                        model: model.map(|m| m.to_string()),
                                        allowed_tools: AgentType::Builder.allowed_tools(),
                                        timeout_override: Some(cr_timeout),
                                        provider_session_id: None,
                                        log_dir: None,
                                        grove_session_id: None,
                                        input_handle_callback: None,
                                        mcp_config_path: None,
                                    };

                                    let cr_result = provider.execute(&cr_request);

                                    // Record cost regardless of success/failure.
                                    if let Ok(ref response) = cr_result {
                                        let _ = budget_meter::record(conn, run_id, response);
                                    }

                                    // Validate resolution and finalize merge.
                                    let merge_finalized = cr_result.is_ok()
                                        && worktree::git_ops::git_merge_continue(wt_path).is_ok();

                                    if merge_finalized {
                                        let merge_sha =
                                            worktree::git_ops::git_rev_parse_head(wt_path)
                                                .unwrap_or_default();
                                        tracing::info!(
                                            conv_branch = %cb,
                                            merge_commit = %merge_sha,
                                            resolved_files = %conflicting_files.join(", "),
                                            "conflict resolution agent succeeded"
                                        );
                                        let cost = cr_result.as_ref().ok().and_then(|r| r.cost_usd);
                                        let _ = events::emit(
                                            conn,
                                            run_id,
                                            None,
                                            crate::events::event_types::PRE_RUN_CONFLICT_RESOLVED,
                                            json!({
                                                "conversation_id": conversation_id,
                                                "conv_branch": cb,
                                                "merge_commit_sha": merge_sha,
                                                "resolved_files": conflicting_files,
                                                "resolution_cost_usd": cost,
                                            }),
                                        );
                                    } else {
                                        // Resolution failed — abort the merge and fail the run.
                                        let reason = if let Err(ref e) = cr_result {
                                            format!("conflict resolution agent failed: {e}")
                                        } else {
                                            "conflict markers remain after resolution attempt"
                                                .to_string()
                                        };
                                        let _ = worktree::git_ops::git_merge_abort(wt_path);
                                        tracing::error!(
                                            conv_branch = %cb,
                                            reason = %reason,
                                            "conflict resolution failed — merge aborted"
                                        );
                                        let _ = events::emit(
                                            conn,
                                            run_id,
                                            None,
                                            crate::events::event_types::PRE_RUN_CONFLICT_FAILED,
                                            json!({
                                                "conversation_id": conversation_id,
                                                "conv_branch": cb,
                                                "unresolved_files": conflicting_files,
                                                "reason": reason,
                                            }),
                                        );
                                        return Err(GroveError::MergeConflict {
                                            files: conflicting_files.join(", "),
                                            file_count: conflicting_files.len(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                SyncBeforeRun::Rebase => {
                    if let Some(stale) = worktree::git_ops::git_detect_stale_base(
                        project_root,
                        cb,
                        &cfg.project.default_branch,
                    ) {
                        tracing::info!(
                            conv_branch = %cb,
                            default_branch = %cfg.project.default_branch,
                            commits_behind = stale.commits_behind,
                            merge_base = %stale.merge_base,
                            "conversation branch is stale — rebasing onto main"
                        );
                        let _ = events::emit(
                            conn,
                            run_id,
                            None,
                            "conv_branch_stale",
                            json!({
                                "conv_branch": cb,
                                "default_branch": cfg.project.default_branch,
                                "sync_strategy": "rebase",
                                "commits_behind": stale.commits_behind,
                                "merge_base": stale.merge_base,
                                "upstream_head": stale.upstream_head,
                            }),
                        );

                        match super::rebase_conv_branch(
                            project_root,
                            cb,
                            &cfg.project.default_branch,
                        )? {
                            worktree::git_ops::RebaseOutcome::Success => {
                                tracing::info!(
                                    conv_branch = %cb,
                                    default_branch = %cfg.project.default_branch,
                                    "auto-rebased stale conversation branch before run"
                                );
                                let _ = events::emit(
                                    conn,
                                    run_id,
                                    None,
                                    crate::events::event_types::CONV_REBASED,
                                    json!({
                                        "conversation_id": conversation_id,
                                        "conv_branch": cb,
                                        "default_branch": cfg.project.default_branch,
                                        "auto": true,
                                    }),
                                );
                                if let Some(wt_path) = conv_wt_path {
                                    worktree::git_ops::run_git(wt_path, &["reset", "--hard", cb])?;
                                }
                            }
                            worktree::git_ops::RebaseOutcome::Conflict { conflicting_files } => {
                                return Err(GroveError::MergeConflict {
                                    files: conflicting_files.join(", "),
                                    file_count: conflicting_files.len(),
                                });
                            }
                        }
                    }
                }
                SyncBeforeRun::None => {
                    // Still detect and log staleness so users know, but don't act on it.
                    if let Some(stale) = worktree::git_ops::git_detect_stale_base(
                        project_root,
                        cb,
                        &cfg.project.default_branch,
                    ) {
                        tracing::info!(
                            conv_branch = %cb,
                            default_branch = %cfg.project.default_branch,
                            commits_behind = stale.commits_behind,
                            "sync_before_run is disabled — branch is stale, skipping sync"
                        );
                        let _ = events::emit(
                            conn,
                            run_id,
                            None,
                            "conv_branch_stale",
                            json!({
                                "conv_branch": cb,
                                "default_branch": cfg.project.default_branch,
                                "sync_strategy": "none",
                                "commits_behind": stale.commits_behind,
                                "merge_base": stale.merge_base,
                                "upstream_head": stale.upstream_head,
                            }),
                        );
                    }
                }
            }
        }
    }

    // Determine run worktree path: use the persistent conversation worktree when
    // available (git + conv_id), otherwise fall back to a per-run worktree (non-git
    // or no conversation context).
    let run_worktree_path: PathBuf = if let Some(wt_path) = conv_wt_path {
        // Conversation worktree already cleaned and synced above.
        tracing::info!(path = %wt_path.display(), "using conversation worktree");
        wt_path.to_path_buf()
    } else {
        // Non-git fallback: use project root as workspace (no worktree isolation).
        tracing::info!("no conversation worktree — using project root as workspace");
        project_root.to_path_buf()
    };

    // Compute artifacts directory for this run. Agents write pipeline artifacts
    // (PRD, Design, Review, Verdict docs) here instead of polluting the worktree.
    let run_artifacts_dir = if let Some(cid) = conversation_id {
        let dir = crate::config::paths::run_artifacts_dir(project_root, cid, run_id);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(error = %e, "failed to create artifacts dir — agents will use worktree");
        }
        dir
    } else {
        // No conversation context: fall back to a temp artifacts dir in .grove/
        let dir = crate::config::paths::grove_dir(project_root)
            .join("artifacts")
            .join("_no_conv")
            .join(run_id);
        let _ = std::fs::create_dir_all(&dir);
        dir
    };

    // §1.2: Copy gitignored/untracked files (.env, .envrc, etc.) into the
    // worktree so agents have access to local credentials and overrides.
    if !cfg.worktree.copy_ignored.is_empty() {
        let preserve_result = worktree::preserve::preserve_files(
            project_root,
            &run_worktree_path,
            &cfg.worktree.copy_ignored,
        );
        tracing::debug!(
            copied = preserve_result.copied,
            skipped = preserve_result.skipped,
            errors = preserve_result.errors,
            "preserve_files complete"
        );
    }

    // Checkpoint SHA of the last successful agent — initialized to worktree HEAD
    // so the first agent's parent_checkpoint_sha is the branch starting point.
    let mut last_good_checkpoint: Option<String> = if is_git {
        worktree::git_ops::git_rev_parse_head(&run_worktree_path).ok()
    } else {
        None
    };

    let work_plan: Vec<Vec<AgentType>> = plan.to_vec();
    let owned_steps: Option<Vec<PlanStep>> = plan_steps.map(|s| s.to_vec());

    let persistent = provider.persistent_phase_provider().ok_or_else(|| {
        GroveError::Runtime(format!(
            "provider '{}' does not expose a persistent phase strategy",
            provider.name()
        ))
    })?;

    let db_path = resolve_db_path(conn, project_root);
    let use_single_cli = should_use_single_cli_pipeline(&work_plan);

    if use_single_cli {
        tracing::info!(
            run_id = %run_id,
            total_agents = work_plan.iter().map(|s| s.len()).sum::<usize>(),
            "using single-CLI pipeline mode with MCP tools"
        );
        run_agents_single_cli(
            conn,
            run_id,
            objective,
            &work_plan,
            persistent,
            cfg,
            project_root,
            model,
            &run_worktree_path,
            pause_after,
            conversation_id,
            abort_handle,
            sink,
            project_configs.as_ref(),
            is_git,
            &mut last_good_checkpoint,
            &db_path,
        )?;
    } else {
        run_agents_persistent(
            conn,
            run_id,
            objective,
            &work_plan,
            persistent,
            Arc::clone(&provider),
            cfg,
            project_root,
            model,
            shared_execution_context,
            agent_briefs,
            &run_worktree_path,
            interactive,
            pause_after,
            owned_steps.as_deref(),
            conversation_id,
            abort_handle,
            initial_provider_session_id,
            sink,
            input_handle_callback,
            run_control_callback,
            project_configs.as_ref(),
            is_git,
            &mut last_good_checkpoint,
            &run_artifacts_dir,
        )?;
    }

    // All stages completed — advance through Verifying → Publishing → Completed.
    super::save_stage_checkpoint(conn, run_id, "before_verifying");
    transitions::apply_transition(conn, run_id, RunState::Executing, RunState::Verifying)?;

    // Pre-publish pull: integrate remote conv branch to ensure push is fast-forward.
    if is_git && cfg.publish.enabled && cfg.worktree.pull_before_publish {
        if let Some(ref cb) = conv_branch {
            if let Err(e) = pull_remote_before_publish(
                conn,
                run_id,
                &run_worktree_path,
                provider.as_ref(),
                cfg,
                model,
                cb,
            ) {
                tracing::warn!(
                    error = %e,
                    "pre-publish pull failed — push may be rejected"
                );
                // Non-fatal: let publish try anyway. recover_interrupted_publishes handles retries.
            }
        }
    }

    super::save_stage_checkpoint(conn, run_id, "before_publishing");
    transitions::apply_transition(conn, run_id, RunState::Verifying, RunState::Publishing)?;

    // Release all file-ownership locks for this run now that it is complete.
    // abort.rs handles this on abort; we mirror that here for clean completion.
    let _ = ownership_repo::release_all_for_run(conn, run_id);

    crate::publish::publish_run(
        conn,
        cfg,
        project_root,
        run_id,
        &run_worktree_path,
        Some(Arc::clone(&provider)),
        model,
    )?;

    super::save_stage_checkpoint(conn, run_id, "before_completed");
    transitions::apply_transition(conn, run_id, RunState::Publishing, RunState::Completed)?;

    // Hooks: PostRun
    let post_run_ctx = crate::hooks::HookContext {
        run_id: run_id.to_string(),
        session_id: None,
        agent_type: None,
        worktree_path: Some(run_worktree_path.to_string_lossy().to_string()),
        event: crate::config::HookEvent::PostRun,
    };
    let _ = crate::hooks::run_hooks(
        &cfg.hooks,
        crate::config::HookEvent::PostRun,
        &post_run_ctx,
        project_root,
    );

    // GROVE-023: flush WAL pages to the main DB file after a successful run.
    // PASSIVE mode is non-blocking — it yields to any active readers rather
    // than waiting for them, so this call never stalls the engine.
    let _ = crate::checkpoint::wal_controller::passive_checkpoint(conn);

    Ok(())
}

struct PersistentGateDecision {
    decision: String,
    notes: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn run_agents_persistent(
    conn: &mut Connection,
    run_id: &str,
    objective: &str,
    work_plan_input: &[Vec<AgentType>],
    provider: &dyn crate::providers::PersistentPhaseProvider,
    provider_arc: Arc<dyn Provider>,
    cfg: &GroveConfig,
    project_root: &Path,
    model: Option<&str>,
    shared_execution_context: Option<&str>,
    agent_briefs: Option<&HashMap<AgentType, String>>,
    run_worktree_path: &Path,
    _interactive: bool,
    pause_after: &[AgentType],
    plan_steps: Option<&[PlanStep]>,
    conversation_id: Option<&str>,
    abort_handle: Option<&super::abort_handle::AbortHandle>,
    initial_provider_session_id: Option<String>,
    sink: &dyn StreamSink,
    _input_handle_callback: Option<
        &std::sync::Arc<dyn Fn(crate::providers::agent_input::AgentInputHandle) + Send + Sync>,
    >,
    run_control_callback: Option<
        &std::sync::Arc<
            dyn Fn(String, crate::providers::claude_code_persistent::PersistentRunControlHandle)
                + Send
                + Sync,
        >,
    >,
    project_configs: Option<&crate::config::agent_config::ProjectConfigs>,
    is_git: bool,
    last_good_checkpoint: &mut Option<String>,
    run_artifacts_dir: &Path,
) -> GroveResult<()> {
    let session_log_dir = crate::config::paths::logs_dir(project_root)
        .join("runs")
        .join(run_id);
    let log_dir_str = session_log_dir.to_string_lossy().to_string();
    let run_mcp_config = if cfg.orchestration.enable_run_mcp {
        let db_path = resolve_db_path(conn, project_root);
        crate::providers::mcp_inject::prepare_run_mcp_config(&db_path)?
    } else {
        None
    };
    let run_mcp_config_str = run_mcp_config
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());

    let persistent_tools = persistent_allowed_tools_union(work_plan_input, project_configs);
    let mut host = provider.start_host(
        run_id,
        &run_worktree_path.to_string_lossy(),
        model,
        Some(&persistent_tools),
        Some(&log_dir_str),
        run_mcp_config_str.as_deref(),
    )?;
    let (decision_tx, decision_rx) = mpsc::channel();
    if let Some(cb) = run_control_callback {
        cb(
            run_id.to_string(),
            crate::providers::claude_code_persistent::PersistentRunControlHandle {
                run_id: run_id.to_string(),
                tx: decision_tx,
            },
        );
    }

    let _gitignore = crate::worktree::gitignore::GitignoreFilter::load(project_root);
    let continuity_policy = provider_arc.session_continuity_policy();
    let pending_resume: Option<String> =
        if continuity_policy == crate::providers::SessionContinuityPolicy::DetachedResume {
            initial_provider_session_id
        } else {
            None
        };
    // Seed the host with the resume session ID so the first turn can continue
    // an existing coding agent conversation.
    if let Some(ref sid) = pending_resume {
        host.set_provider_thread_id(sid.clone());
    }

    let mut feedback_prefix = String::new();
    let mut gate_context: Option<String> = None;
    let mut claimed_step_ids: Vec<String> = Vec::new();
    let mut previous_worktree: Option<PathBuf> = Some(run_worktree_path.to_path_buf());
    let mut last_good_worktree: Option<PathBuf> = None;
    let mut stored_decomposition: Option<(TaskDecomposition, PathBuf)> = None;
    let mut decomposition_executed = false;
    let mut spawn_wave_count: u32 = 0;

    let mut work_plan: Vec<Vec<AgentType>> = work_plan_input.to_vec();
    let mut owned_steps: Option<Vec<PlanStep>> = plan_steps.map(|s| s.to_vec());
    let mut stage_idx = 0usize;
    let auto_continue_phase_gates: bool = conn
        .query_row(
            "SELECT disable_phase_gates FROM runs WHERE id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .unwrap_or(false);

    while stage_idx < work_plan.len() {
        // --- Abort check between stages ---
        if let Some(h) = abort_handle {
            if h.is_aborted() {
                let _ = provider.abort_host(&mut host);
                return Err(GroveError::Aborted);
            }
        }
        let db_state: String = conn
            .query_row("SELECT state FROM runs WHERE id=?1", [run_id], |r| r.get(0))
            .unwrap_or_default();
        if db_state == "paused" {
            let _ = provider.abort_host(&mut host);
            return Err(GroveError::Aborted);
        }

        let stage = work_plan[stage_idx].clone();

        // --- Pre-flight budget check (first stage only) ---
        if stage_idx == 0 {
            if let Ok(BudgetStatus::Exceeded {
                used_usd,
                limit_usd,
            }) = budget_policy::check_budget(conn, run_id)
            {
                events::emit(
                    conn,
                    run_id,
                    None,
                    crate::events::event_types::BUDGET_EXCEEDED,
                    json!({ "used_usd": used_usd, "limit_usd": limit_usd }),
                )?;
                let _ = provider.abort_host(&mut host);
                fail_run(conn, run_id, objective, 0.0)?;
                return Err(GroveError::BudgetExceeded {
                    used_usd,
                    limit_usd,
                });
            }
        }

        // --- Task decomposition: if architect produced TASKS, replace builder stages ---
        let stage_has_builder = stage.contains(&AgentType::Builder);
        let stage_has_plan_system_design = stage.contains(&AgentType::PlanSystemDesign);

        if stage_has_builder && (stored_decomposition.is_some() || decomposition_executed) {
            if let Some((decomp, _base)) = stored_decomposition.take() {
                decomposition_executed = true;
                let max_agents = usize::from(cfg.runtime.max_agents);
                run_task_waves(
                    conn,
                    run_id,
                    objective,
                    &decomp,
                    cfg,
                    Arc::clone(&provider_arc),
                    model,
                    &mut previous_worktree,
                    &mut last_good_worktree,
                    last_good_checkpoint,
                    run_worktree_path,
                    is_git,
                    max_agents,
                    project_root,
                    project_configs,
                    run_artifacts_dir,
                )?;
            }
            stage_idx += 1;
            continue;
        }

        // --- Execute all agents in the stage sequentially ---
        let mut all_spawned: Vec<GrovePlanStep> = Vec::new();

        for &agent_type in &stage {
            // Clear the provider session ID between different agent roles to prevent
            // "session already in use" errors — each agent type gets a fresh session.
            host.provider_thread_id = None;

            'agent_run: loop {
                let _ = crate::db::repositories::phase_checkpoints_repo::update_run_phase(
                    conn,
                    run_id,
                    "",
                    agent_type.as_str(),
                );

                let pre_agent_checkpoint = last_good_checkpoint.clone();

                let session_id = format!("sess_{}", Uuid::new_v4().simple());
                insert_session(
                    conn,
                    &session_id,
                    run_id,
                    agent_type.as_str(),
                    AgentState::Queued.as_str(),
                    run_worktree_path.to_string_lossy().as_ref(),
                )?;
                events::emit(
                    conn,
                    run_id,
                    Some(&session_id),
                    crate::events::event_types::SESSION_SPAWNED,
                    json!({ "agent_type": agent_type.as_str(), "worktree": run_worktree_path }),
                )?;
                set_session_state(conn, &session_id, AgentState::Running)?;
                events::emit(
                    conn,
                    run_id,
                    Some(&session_id),
                    crate::events::event_types::SESSION_STATE_CHANGED,
                    json!({ "state": "running" }),
                )?;

                let hook_ctx = crate::hooks::HookContext {
                    run_id: run_id.to_string(),
                    session_id: Some(session_id.clone()),
                    agent_type: Some(agent_type.as_str().to_string()),
                    worktree_path: Some(run_worktree_path.to_string_lossy().to_string()),
                    event: crate::config::HookEvent::SessionStart,
                };
                let _ = crate::hooks::run_hooks(
                    &cfg.hooks,
                    crate::config::HookEvent::SessionStart,
                    &hook_ctx,
                    project_root,
                );

                let claimed_step = owned_steps
                    .as_deref()
                    .and_then(|s| claim_plan_step(s, agent_type, &mut claimed_step_ids));
                if let Some(step) = claimed_step.as_ref() {
                    plan_steps_repo::set_status(
                        conn,
                        &step.id,
                        "in_progress",
                        Some(&session_id),
                        None,
                    )?;
                }

                let loaded_agent_config: Option<crate::config::agent_config::AgentConfig> =
                    project_configs.and_then(|c| c.agents.get(agent_type.as_str()).cloned());
                let agent_scope: Option<crate::orchestrator::scope::ScopeConfig> =
                    loaded_agent_config.as_ref().and_then(|ac| ac.scope.clone());
                let pre_agent_head: Option<String> =
                    if agent_scope.as_ref().is_some_and(|s| s.has_restrictions()) {
                        crate::worktree::git_ops::git_rev_parse_head(run_worktree_path).ok()
                    } else {
                        None
                    };

                let base_instructions = build_instructions(
                    agent_type,
                    objective,
                    shared_execution_context,
                    agent_briefs.and_then(|briefs| briefs.get(&agent_type).map(String::as_str)),
                    run_id,
                    run_worktree_path,
                    None,
                    claimed_step,
                    None,
                    project_root,
                    project_configs,
                    run_artifacts_dir,
                );
                let mut instructions = String::new();
                if !feedback_prefix.is_empty() {
                    instructions.push_str(&std::mem::take(&mut feedback_prefix));
                }
                instructions.push_str(&base_instructions);
                if let Some(ref scope) = agent_scope {
                    let contract = scope.build_contract(run_id);
                    if !contract.is_empty() {
                        instructions.push_str("\n\n");
                        instructions.push_str(&contract);
                    }
                }

                sink.on_event(crate::providers::StreamOutputEvent::PhaseStart {
                    phase: agent_type.as_str().to_string(),
                    run_id: run_id.to_string(),
                });

                let qa_db_path = resolve_db_path(conn, project_root);
                let qa_source = {
                    let mut qs = super::qa_source::DbQaSource::new(&qa_db_path);
                    if let Some(h) = abort_handle {
                        qs = qs.with_abort_handle(h.clone());
                    }
                    qs
                };

                // Self-heartbeat: keep the watchdog from flagging this session
                // as stalled/zombie while the persistent turn is running.
                let hb_db_path = resolve_db_path(conn, project_root);
                let _hb_guard = spawn_heartbeat_guard(hb_db_path, session_id.clone());

                // --- Execute with retry logic ---
                let max_retries: u32 = u32::from(cfg.orchestration.max_retries_per_session);
                let mut attempt: u32 = 0;
                let outcome = loop {
                    let turn_result = provider.execute_persistent_turn(
                        &mut host,
                        &crate::providers::claude_code_persistent::PhaseTurn {
                            phase: agent_type.as_str().to_string(),
                            instructions: instructions.clone(),
                            gate_context: gate_context.take(),
                        },
                        sink,
                        &qa_source,
                        &session_id,
                    );

                    match turn_result {
                        Ok(outcome) => break outcome,
                        Err(ref e) if matches!(e, GroveError::Aborted) => {
                            return Err(GroveError::Aborted);
                        }
                        Err(e) => {
                            // HostDied is returned as Ok(HostDied), not Err, so this branch
                            // handles retryable provider errors (timeouts, transient failures).
                            if attempt < max_retries && is_retryable_session_error(&e) {
                                attempt += 1;
                                let backoff = Duration::from_secs(2u64.pow(attempt));
                                tracing::warn!(
                                    attempt,
                                    max_retries,
                                    error = %e,
                                    backoff_secs = backoff.as_secs(),
                                    "persistent turn failed — retrying"
                                );
                                std::thread::sleep(backoff);
                                continue;
                            }
                            record_session_failure(conn, run_id, &session_id, &e, None)?;
                            return Err(e);
                        }
                    }
                };

                match outcome {
                    crate::providers::claude_code_persistent::PhaseTurnOutcome::HostDied {
                        last_session_id,
                        partial_output,
                    } => {
                        let host_error = GroveError::Runtime(format!(
                            "persistent host exited during {} (thread {:?}): {}",
                            agent_type.as_str(),
                            last_session_id,
                            partial_output
                        ));
                        record_session_failure(
                            conn,
                            run_id,
                            &session_id,
                            &host_error,
                            last_session_id.as_deref(),
                        )?;
                        fail_run(conn, run_id, objective, 0.0)?;
                        return Err(host_error);
                    }
                    crate::providers::claude_code_persistent::PhaseTurnOutcome::TurnDone {
                        response_text,
                        cost_usd,
                        session_id: provider_session_id,
                        grove_control,
                    } => {
                        let canonical_thread = provider_session_id
                            .clone()
                            .or_else(|| host.provider_thread_id.clone());
                        if let Some(ref psid) = canonical_thread {
                            conn.execute(
                                "UPDATE sessions SET provider_session_id = ?1 WHERE id = ?2",
                                params![psid, session_id],
                            )?;
                            runs_repo::set_provider_thread_id(
                                conn,
                                run_id,
                                Some(psid),
                                &Utc::now().to_rfc3339(),
                            )?;
                        }
                        let _ = conn.execute(
                            "UPDATE sessions SET pid = ?1 WHERE id = ?2",
                            params![host.pid.unwrap_or(0) as i64, session_id],
                        );
                        if let Some(cost) = cost_usd {
                            if cost > 0.0 {
                                conn.execute(
                                    "UPDATE sessions SET cost_usd = ?1 WHERE id = ?2",
                                    params![cost, session_id],
                                )?;
                            }
                        }

                        let phase_summary = grove_control
                            .as_ref()
                            .map(|c| c.summary.clone())
                            .unwrap_or_else(|| response_text.clone());
                        let response = crate::providers::ProviderResponse {
                            summary: phase_summary.clone(),
                            changed_files: vec![],
                            cost_usd,
                            provider_session_id: canonical_thread.clone(),
                            pid: host.pid,
                        };
                        let status = budget_meter::record(conn, run_id, &response)?;

                        if let Some(cid) = conversation_id {
                            let _ = super::conversation::record_agent_message(
                                conn,
                                cid,
                                run_id,
                                agent_type.as_str(),
                                &session_id,
                                &phase_summary,
                            );
                        }

                        set_session_state(conn, &session_id, AgentState::Completed)?;
                        sink.on_event(crate::providers::StreamOutputEvent::PhaseEnd {
                            phase: agent_type.as_str().to_string(),
                            run_id: run_id.to_string(),
                            outcome: "completed".to_string(),
                        });

                        if let Some(step) = claimed_step.as_ref() {
                            let summary_trunc: String = phase_summary.chars().take(500).collect();
                            plan_steps_repo::set_status(
                                conn,
                                &step.id,
                                "completed",
                                None,
                                Some(&summary_trunc),
                            )?;
                        }
                        events::emit(
                            conn,
                            run_id,
                            Some(&session_id),
                            crate::events::event_types::SESSION_STATE_CHANGED,
                            json!({ "state": "completed", "summary": phase_summary }),
                        )?;
                        let _ = crate::signals::send_signal(
                            conn,
                            run_id,
                            agent_type.as_str(),
                            crate::signals::GROUP_LEADS,
                            crate::signals::SignalType::WorkerDone,
                            crate::signals::SignalPriority::Normal,
                            json!({ "agent": agent_type.as_str(), "session_id": session_id }),
                        );

                        if let Some(ref scope) = agent_scope {
                            if scope.has_restrictions() {
                                let changed_files = crate::worktree::git_ops::changed_files_since(
                                    run_worktree_path,
                                    pre_agent_head.as_deref(),
                                )
                                .unwrap_or_default();
                                let validation =
                                    crate::orchestrator::scope::ScopeValidator::validate(
                                        scope,
                                        &changed_files,
                                        run_id,
                                        run_worktree_path,
                                        run_artifacts_dir,
                                    );
                                if validation.passed {
                                    sink.on_event(
                                        crate::providers::StreamOutputEvent::ScopeCheckPassed {
                                            agent: agent_type.as_str().to_string(),
                                            artifact_count: scope
                                                .resolve_artifact_patterns(run_id)
                                                .len(),
                                        },
                                    );
                                } else {
                                    let violations_json: Vec<serde_json::Value> = validation
                                        .violations
                                        .iter()
                                        .map(|v| serde_json::to_value(v).unwrap_or_default())
                                        .collect();
                                    let action = if scope.on_violation
                                        == crate::orchestrator::scope::ViolationPolicy::Warn
                                    {
                                        "warn"
                                    } else {
                                        "hard_fail"
                                    };
                                    sink.on_event(
                                        crate::providers::StreamOutputEvent::ScopeViolation {
                                            agent: agent_type.as_str().to_string(),
                                            violations: violations_json,
                                            action: action.to_string(),
                                            attempt: 1,
                                        },
                                    );
                                    if action == "hard_fail" {
                                        let _ = provider.abort_host(&mut host);
                                        return Err(GroveError::Runtime(format!(
                                            "scope violation by {} in persistent mode",
                                            agent_type.as_str()
                                        )));
                                    }
                                }
                            }
                        }

                        let short_desc = claimed_step
                            .as_ref()
                            .map(|s| s.title.clone())
                            .or_else(|| {
                                let line = phase_summary.lines().next().unwrap_or("").trim();
                                if line.is_empty() {
                                    None
                                } else {
                                    Some(line.to_string())
                                }
                            })
                            .unwrap_or_else(|| "stage work".to_string());
                        let short_desc: String = short_desc.chars().take(60).collect();
                        let commit_msg = format!("grove({}): {}", agent_type.as_str(), short_desc);
                        if !commit_agent_work(run_worktree_path, &commit_msg) {
                            tracing::warn!(
                                session = %session_id,
                                agent = %agent_type.as_str(),
                                worktree = %run_worktree_path.display(),
                                "commit_agent_work failed in persistent mode"
                            );
                        }

                        // Handoff context for next agent
                        if is_git {
                            if let Some(ctx) = super::handoff::build_handoff_context(
                                run_worktree_path,
                                pre_agent_checkpoint.as_deref(),
                                last_good_checkpoint.as_deref(),
                            ) {
                                feedback_prefix = ctx;
                            }
                        }

                        if let Some(artifact_name) =
                            find_agent_artifact(run_artifacts_dir, agent_type, run_id)
                        {
                            let artifact_path = run_artifacts_dir.join(&artifact_name);
                            if let Ok(bytes) = std::fs::read(&artifact_path) {
                                let size_bytes = bytes.len() as i64;
                                let content_hash = format!("{:016x}", {
                                    let mut h: u64 = 0xcbf29ce484222325;
                                    for &b in &bytes {
                                        h ^= b as u64;
                                        h = h.wrapping_mul(0x100000001b3);
                                    }
                                    h
                                });
                                let _ = run_artifacts_repo::record_artifact(
                                    conn,
                                    run_id,
                                    agent_type.as_str(),
                                    &artifact_name,
                                    &content_hash,
                                    size_bytes,
                                );
                            }
                        }

                        let parent_sha = last_good_checkpoint.clone();
                        let checkpoint_sha = if is_git {
                            match worktree::git_ops::git_rev_parse_head(run_worktree_path) {
                                Ok(sha) => {
                                    conn.execute(
                                    "UPDATE sessions SET checkpoint_sha = ?1, parent_checkpoint_sha = ?2 WHERE id = ?3",
                                    params![sha, parent_sha, session_id],
                                )?;
                                    Some(sha)
                                }
                                Err(e) => {
                                    tracing::warn!(session = %session_id, error = %e, "failed to record checkpoint SHA in persistent mode");
                                    None
                                }
                            }
                        } else {
                            None
                        };
                        if checkpoint_sha.is_some() {
                            *last_good_checkpoint = checkpoint_sha;
                        }

                        if let BudgetStatus::Warning { percent_used, .. } = status {
                            events::emit(
                                conn,
                                run_id,
                                None,
                                crate::events::event_types::BUDGET_WARNING,
                                json!({ "percent_used": percent_used }),
                            )?;
                        }
                        if let BudgetStatus::Exceeded {
                            used_usd,
                            limit_usd,
                        } = status
                        {
                            events::emit(
                                conn,
                                run_id,
                                None,
                                crate::events::event_types::BUDGET_EXCEEDED,
                                json!({ "used_usd": used_usd, "limit_usd": limit_usd }),
                            )?;
                            let _ = provider.abort_host(&mut host);
                            fail_run(conn, run_id, objective, 0.0)?;
                            return Err(GroveError::Runtime(format!(
                                "budget exceeded in persistent session {session_id}: used ${used_usd:.4} of ${limit_usd:.4}"
                            )));
                        }

                        // GROVE_SPAWN support
                        let spawned_steps =
                            spawn::read_spawn_file(run_worktree_path).unwrap_or_default();
                        all_spawned.extend(spawned_steps);

                        // Phase gate (persistent channel-based)
                        if pause_after.contains(&agent_type) {
                            let artifact =
                                find_agent_artifact(run_artifacts_dir, agent_type, run_id);
                            let cp_id = phase_checkpoints_repo::insert(
                                conn,
                                run_id,
                                agent_type.as_str(),
                                artifact.as_deref(),
                            )?;
                            events::emit(
                                conn,
                                run_id,
                                None,
                                "phase_gate_pending",
                                json!({
                                    "agent": agent_type.as_str(),
                                    "checkpoint_id": cp_id,
                                    "artifact": artifact,
                                }),
                            )?;
                            sink.on_event(crate::providers::StreamOutputEvent::PhaseGate {
                                phase: agent_type.as_str().to_string(),
                                run_id: run_id.to_string(),
                                requires_approval: !auto_continue_phase_gates,
                                checkpoint_id: cp_id,
                            });
                            let decision = if auto_continue_phase_gates {
                                phase_checkpoints_repo::submit_decision(
                                    conn,
                                    cp_id,
                                    "approved",
                                    Some("auto-continued for this run"),
                                )?;
                                PersistentGateDecision {
                                    decision: "approved".to_string(),
                                    notes: Some("auto-continued for this run".to_string()),
                                }
                            } else {
                                transitions::apply_transition(
                                    conn,
                                    run_id,
                                    RunState::Executing,
                                    RunState::WaitingForGate,
                                )?;
                                host.transition(
                                    crate::providers::claude_code_persistent::HostState::WaitingForGate,
                                )?;
                                let decision = wait_for_persistent_gate_decision(
                                    conn,
                                    cp_id,
                                    run_id,
                                    &decision_rx,
                                    abort_handle,
                                )?;
                                transitions::apply_transition(
                                    conn,
                                    run_id,
                                    RunState::WaitingForGate,
                                    RunState::Executing,
                                )?;
                                host.transition(
                                    crate::providers::claude_code_persistent::HostState::Running,
                                )?;
                                decision
                            };
                            match decision.decision.as_str() {
                                "approved" | "skipped" => {
                                    gate_context = Some(match decision.notes.as_deref() {
                                        Some(note) if !note.is_empty() => format!(
                                            "Gate decision for checkpoint {}: {}\nNote: {}",
                                            cp_id, decision.decision, note
                                        ),
                                        _ => format!(
                                            "Gate decision for checkpoint {}: {}",
                                            cp_id, decision.decision
                                        ),
                                    });
                                }
                                "approved_with_note" => {
                                    let note = decision.notes.unwrap_or_default();
                                    gate_context = Some(format!(
                                        "Gate decision for checkpoint {}: approved_with_note\nUser note: {}",
                                        cp_id, note
                                    ));
                                    if !note.is_empty() {
                                        feedback_prefix =
                                            super::interactive::format_feedback_prefix(
                                                agent_type, &note,
                                            );
                                    }
                                }
                                "retry" => {
                                    gate_context = Some(match decision.notes {
                                        Some(note) if !note.is_empty() => format!(
                                            "Gate decision for checkpoint {}: retry\nUser note: {}\nRe-run the current phase ({}) with the same instructions.",
                                            cp_id,
                                            note,
                                            agent_type.as_str()
                                        ),
                                        _ => format!(
                                            "Gate decision for checkpoint {}: retry\nRe-run the current phase ({}) with the same instructions.",
                                            cp_id,
                                            agent_type.as_str()
                                        ),
                                    });
                                    continue 'agent_run;
                                }
                                "retry_resume" => {
                                    gate_context = Some(match decision.notes {
                                        Some(note) if !note.is_empty() => format!(
                                            "Gate decision for checkpoint {}: retry_resume\nUser feedback: {}\nRevise the current phase ({}) output, continuing the conversation. Address the user's feedback.",
                                            cp_id,
                                            note,
                                            agent_type.as_str()
                                        ),
                                        _ => format!(
                                            "Gate decision for checkpoint {}: retry_resume\nRevise the current phase ({}) output, continuing the conversation.",
                                            cp_id,
                                            agent_type.as_str()
                                        ),
                                    });
                                    continue 'agent_run;
                                }
                                "rejected" => {
                                    let _ = provider.abort_host(&mut host);
                                    fail_run(conn, run_id, objective, 0.0)?;
                                    return Err(GroveError::Runtime(format!(
                                        "phase gate rejected after {} — run stopped by user",
                                        agent_type.as_str()
                                    )));
                                }
                                other => {
                                    gate_context = Some(format!(
                                        "Gate decision for checkpoint {}: {}",
                                        cp_id, other
                                    ));
                                }
                            }
                        }

                        if is_git {
                            let _ =
                                worktree::git_ops::git_clean_worktree_verified(run_worktree_path);
                        }
                    }
                }

                break 'agent_run;
            } // end 'agent_run retry loop
        } // end for agent_type in stage

        // GROVE_SPAWN: inject dynamically spawned steps
        if !all_spawned.is_empty() {
            let max_depth = u32::from(cfg.orchestration.max_spawn_depth);
            if spawn_wave_count >= max_depth {
                tracing::warn!(
                    spawn_wave_count,
                    max_spawn_depth = max_depth,
                    rejected = all_spawned.len(),
                    "dynamic spawn rejected: max_spawn_depth reached"
                );
            } else {
                let wave_offset: i64 = owned_steps
                    .as_ref()
                    .and_then(|ps| ps.iter().map(|s| s.wave).max())
                    .map(|m| m + 1)
                    .unwrap_or(0);

                tracing::info!(
                    count = all_spawned.len(),
                    wave = wave_offset,
                    spawn_wave = spawn_wave_count + 1,
                    max_spawn_depth = max_depth,
                    "dynamic spawn: new step(s) queued"
                );
                plan_steps_repo::insert_steps(conn, run_id, &all_spawned, wave_offset)?;
                let new_steps = plan_steps_repo::list_for_run_wave(conn, run_id, wave_offset)?;

                let new_stage: Vec<AgentType> = new_steps
                    .iter()
                    .filter_map(|s| AgentType::from_str(&s.agent_type))
                    .collect();

                if !new_stage.is_empty() {
                    work_plan.push(new_stage);
                    match owned_steps {
                        Some(ref mut ps) => ps.extend(new_steps),
                        None => owned_steps = Some(new_steps),
                    }
                }
                spawn_wave_count += 1;
            }
        }

        // --- Verdict routing for Reviewer and Judge ---
        let stage_has_reviewer = stage.contains(&AgentType::Reviewer);

        if stage_has_reviewer {
            match verdict::parse_review_verdict(run_artifacts_dir, run_id) {
                Some(ReviewVerdict::Pass) => {
                    tracing::info!("reviewer verdict: PASS — continuing");
                    events::emit(
                        conn,
                        run_id,
                        None,
                        crate::events::event_types::SESSION_STATE_CHANGED,
                        json!({ "agent": "reviewer", "verdict": "PASS" }),
                    )?;
                    sink.on_event(crate::providers::StreamOutputEvent::System {
                        message: "Reviewer verdict: PASS".to_string(),
                        session_id: None,
                    });
                }
                Some(ReviewVerdict::Fail { ref feedback }) => {
                    let on_fail = cfg.agents.reviewer.on_fail.as_str();
                    tracing::warn!("reviewer verdict: FAIL (on_fail={})", on_fail);
                    events::emit(
                        conn,
                        run_id,
                        None,
                        crate::events::event_types::SESSION_STATE_CHANGED,
                        json!({ "agent": "reviewer", "verdict": "FAIL", "feedback": feedback }),
                    )?;
                    sink.on_event(crate::providers::StreamOutputEvent::System {
                        message: format!("Reviewer verdict: FAIL (on_fail={})", on_fail),
                        session_id: None,
                    });
                    match on_fail {
                        "warn" => {
                            eprintln!(
                                "[REVIEWER] FAIL verdict — continuing with warning (on_fail=warn)"
                            );
                        }
                        "retry" => {
                            if !feedback.is_empty() {
                                feedback_prefix.clear();
                                feedback_prefix.push_str(&format!(
                                        "REVIEWER FEEDBACK (you must address these issues before proceeding):\n\
                                         {feedback}\n\n"
                                    ));
                            }
                            work_plan.insert(stage_idx + 1, vec![AgentType::Builder]);
                            eprintln!("[REVIEWER] FAIL verdict — retrying builder with feedback");
                        }
                        _ => {
                            let _ = provider.abort_host(&mut host);
                            fail_run(conn, run_id, objective, 0.0)?;
                            return Err(GroveError::Runtime(format!(
                                "reviewer verdict: FAIL — run blocked. {}",
                                if feedback.is_empty() {
                                    "See REVIEW file for details.".to_string()
                                } else {
                                    feedback.chars().take(200).collect::<String>()
                                }
                            )));
                        }
                    }
                }
                None => {
                    if cfg.discipline.strict_verdicts {
                        tracing::warn!(
                            "reviewer: could not parse verdict — strict mode, treating as failure"
                        );
                        let _ = provider.abort_host(&mut host);
                        fail_run(conn, run_id, objective, 0.0)?;
                        return Err(GroveError::Runtime(
                            "Reviewer verdict unparseable — strict_verdicts enabled".to_string(),
                        ));
                    } else {
                        tracing::warn!("reviewer: could not parse verdict — treating as PASS");
                        events::emit(
                            conn,
                            run_id,
                            None,
                            crate::events::event_types::SESSION_STATE_CHANGED,
                            json!({ "agent": "reviewer", "verdict": "UNKNOWN", "note": "unparseable verdict treated as PASS" }),
                        )?;
                        sink.on_event(crate::providers::StreamOutputEvent::System {
                            message: "Reviewer verdict: UNKNOWN (treating as PASS)".to_string(),
                            session_id: None,
                        });
                    }
                }
            }
        }

        let stage_has_judge = stage.contains(&AgentType::Judge);

        if stage_has_judge {
            match verdict::parse_judge_verdict(run_artifacts_dir, run_id) {
                Some(JudgeVerdict::Approved) => {
                    tracing::info!("judge verdict: APPROVED — pipeline output accepted");
                    events::emit(
                        conn,
                        run_id,
                        None,
                        crate::events::event_types::SESSION_STATE_CHANGED,
                        json!({ "agent": "judge", "verdict": "APPROVED" }),
                    )?;
                    sink.on_event(crate::providers::StreamOutputEvent::System {
                        message: "Judge verdict: APPROVED".to_string(),
                        session_id: None,
                    });
                }
                Some(JudgeVerdict::NeedsWork { ref notes }) => {
                    let on_needs_work = cfg.agents.judge.on_needs_work.as_str();
                    tracing::warn!(
                        "judge verdict: NEEDS_WORK (on_needs_work={})",
                        on_needs_work
                    );
                    events::emit(
                        conn,
                        run_id,
                        None,
                        crate::events::event_types::SESSION_STATE_CHANGED,
                        json!({ "agent": "judge", "verdict": "NEEDS_WORK", "notes": notes }),
                    )?;
                    sink.on_event(crate::providers::StreamOutputEvent::System {
                        message: format!(
                            "Judge verdict: NEEDS_WORK (on_needs_work={})",
                            on_needs_work
                        ),
                        session_id: None,
                    });
                    if on_needs_work == "block" {
                        let _ = provider.abort_host(&mut host);
                        fail_run(conn, run_id, objective, 0.0)?;
                        return Err(GroveError::Runtime(format!(
                            "judge verdict: NEEDS_WORK — output requires rework. {}",
                            notes.chars().take(200).collect::<String>()
                        )));
                    } else {
                        eprintln!(
                            "[JUDGE] NEEDS_WORK verdict — see JUDGE_VERDICT file for details."
                        );
                    }
                }
                Some(JudgeVerdict::Rejected { ref notes }) => {
                    tracing::warn!("judge verdict: REJECTED");
                    events::emit(
                        conn,
                        run_id,
                        None,
                        crate::events::event_types::SESSION_STATE_CHANGED,
                        json!({ "agent": "judge", "verdict": "REJECTED", "notes": notes }),
                    )?;
                    sink.on_event(crate::providers::StreamOutputEvent::System {
                        message: "Judge verdict: REJECTED".to_string(),
                        session_id: None,
                    });
                    let _ = provider.abort_host(&mut host);
                    fail_run(conn, run_id, objective, 0.0)?;
                    return Err(GroveError::Runtime(format!(
                        "judge verdict: REJECTED — pipeline output does not meet the objective. {}",
                        notes.chars().take(200).collect::<String>()
                    )));
                }
                None => {
                    if cfg.discipline.strict_verdicts {
                        tracing::warn!(
                            "judge: could not parse verdict — strict mode, treating as failure"
                        );
                        let _ = provider.abort_host(&mut host);
                        fail_run(conn, run_id, objective, 0.0)?;
                        return Err(GroveError::Runtime(
                            "Judge verdict unparseable — strict_verdicts enabled".to_string(),
                        ));
                    } else {
                        tracing::warn!("judge: could not parse verdict — treating as APPROVED");
                        events::emit(
                            conn,
                            run_id,
                            None,
                            crate::events::event_types::SESSION_STATE_CHANGED,
                            json!({ "agent": "judge", "verdict": "UNKNOWN", "note": "unparseable verdict treated as APPROVED" }),
                        )?;
                        sink.on_event(crate::providers::StreamOutputEvent::System {
                            message: "Judge verdict: UNKNOWN (treating as APPROVED)".to_string(),
                            session_id: None,
                        });
                    }
                }
            }
        }

        // After a plan_system_design stage, check for TASKS decomposition file
        // in the artifacts directory (where the architect writes it).
        if stage_has_plan_system_design {
            if let Some(decomp) = task_decomposer::read_tasks_file(run_artifacts_dir, run_id) {
                task_decomposer::insert_subtasks(conn, run_id, &decomp.tasks)?;
                tracing::info!(
                    count = decomp.tasks.len(),
                    "sub-task(s) detected from architect's plan"
                );
                stored_decomposition = Some((
                    decomp,
                    previous_worktree
                        .clone()
                        .unwrap_or_else(|| run_worktree_path.to_path_buf()),
                ));
            }
        }

        stage_idx += 1;
    }

    host.shutdown(crate::providers::claude_code_persistent::HostState::Completed)?;
    Ok(())
}

// ── Single-CLI Pipeline Execution ───────────────────────────────────────────
//
// Instead of spawning N CLI processes (one per pipeline stage), pre-build all
// stage instructions in the DB and dispatch a single CLI with MCP tools.
// The CLI self-navigates through stages via grove_get_pipeline_stage /
// grove_complete_pipeline_stage / grove_check_pipeline_gate MCP calls.

/// Pre-build all pipeline stage instructions and insert them into the
/// `pipeline_stages` table so the single CLI can retrieve them via MCP.
#[allow(clippy::too_many_arguments)]
fn prebuild_pipeline_stages(
    conn: &Connection,
    run_id: &str,
    objective: &str,
    work_plan: &[Vec<AgentType>],
    pause_after: &[AgentType],
    run_worktree_path: &Path,
    project_root: &Path,
    project_configs: Option<&crate::config::agent_config::ProjectConfigs>,
    artifacts_dir: &Path,
) -> GroveResult<()> {
    let mut ordinal: i64 = 0;

    for stage in work_plan {
        for &agent_type in stage {
            let instructions = build_instructions(
                agent_type,
                objective,
                None, // shared_execution_context
                None, // agent_brief
                run_id,
                run_worktree_path,
                None, // task
                None, // plan_step
                None, // failure_context
                project_root,
                project_configs,
                artifacts_dir,
            );

            let gate_required: bool = pause_after.contains(&agent_type);
            let stage_id = format!(
                "ps_{}_{}",
                &run_id[..8.min(run_id.len())],
                &uuid::Uuid::new_v4().simple().to_string()[..8]
            );

            conn.execute(
                "INSERT INTO pipeline_stages \
                    (id, run_id, stage_name, ordinal, instructions, gate_required) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    stage_id,
                    run_id,
                    agent_type.as_str(),
                    ordinal,
                    instructions,
                    gate_required as i64,
                ],
            )?;

            tracing::info!(
                run_id = %run_id,
                stage = %agent_type.as_str(),
                ordinal,
                gate_required,
                "pre-built pipeline stage"
            );

            ordinal += 1;
        }
    }

    Ok(())
}

/// Build the pipeline-worker prompt with all stage instructions inlined.
///
/// Instead of telling the agent to fetch stages via MCP round-trips, we embed
/// the objective and every stage's instructions directly in the prompt. This
/// eliminates 8-10 exploratory tool calls the agent would otherwise make to
/// discover its context.
fn build_pipeline_worker_prompt(
    conn: &Connection,
    run_id: &str,
    objective: &str,
    project_root: &Path,
) -> String {
    let mut prompt = String::new();

    prompt.push_str("You are a pipeline worker agent for Grove.\n\n");
    prompt.push_str(&format!("## Objective\n\n{}\n\n", objective));
    prompt.push_str(&format!("Run ID: `{}`\n\n", run_id));

    // Load the pipeline-worker skill if it exists.
    let skill_path = project_root
        .join(".grove")
        .join("skills")
        .join("pipeline-worker")
        .join("SKILL.md");
    if let Ok(skill_content) = std::fs::read_to_string(&skill_path) {
        if !skill_content.is_empty() {
            prompt.push_str(&skill_content);
            prompt.push_str("\n\n");
        }
    }

    // Inline all pre-built stage instructions so the agent doesn't need to
    // fetch them via MCP tool calls.
    let stages: Vec<(String, String, String, i64)> = conn
        .prepare(
            "SELECT id, stage_name, instructions, gate_required \
             FROM pipeline_stages WHERE run_id = ?1 ORDER BY ordinal",
        )
        .and_then(|mut stmt| {
            stmt.query_map(params![run_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
        })
        .unwrap_or_default();

    if stages.is_empty() {
        // Fallback: tell agent to use MCP if stages couldn't be loaded.
        prompt.push_str(
            "Use MCP tools to execute all pipeline stages:\n\
             1. Call grove_get_pipeline_stage(run_id) to get the next stage\n\
             2. Execute the stage instructions\n\
             3. Call grove_complete_pipeline_stage(run_id, stage_id, summary) when done\n\
             4. Repeat until all stages complete\n\n",
        );
    } else {
        prompt.push_str(&format!("## Pipeline Stages ({} total)\n\n", stages.len()));
        prompt.push_str(
            "Execute each stage in order. For each stage: do the work described, \
             then move on to the next. Do NOT call grove_get_pipeline_stage — \
             all instructions are provided below.\n\n",
        );

        for (i, (stage_id, stage_name, instructions, gate_required)) in stages.iter().enumerate() {
            prompt.push_str(&format!(
                "### Stage {} — {} (id: `{}`)\n\n",
                i + 1,
                stage_name,
                stage_id
            ));
            prompt.push_str(instructions);
            prompt.push('\n');
            if *gate_required != 0 {
                prompt.push_str(
                    "\n> **Gate required**: After completing this stage, call \
                     `grove_check_pipeline_gate(run_id, stage_id)` and wait for approval.\n",
                );
            }
            prompt.push('\n');
        }
    }

    prompt.push_str(
        "## Execution Rules\n\n\
         - Start working immediately on Stage 1. Do NOT explore the worktree first.\n\
         - Do NOT call grove_get_pipeline_stage or any MCP tool to discover stages.\n\
         - Do NOT create graphs, phases, or steps via MCP — the engine handles tracking.\n\
         - After all stages are done, commit your work and stop.\n\
         - Do not output a grove_control block.\n",
    );

    prompt
}

/// Execute a pipeline run using a single CLI process with MCP tools.
///
/// Pre-builds all stage instructions in the DB, spawns one CLI with the
/// pipeline-worker skill and MCP config, then does batch post-processing
/// after the CLI finishes.
#[allow(clippy::too_many_arguments)]
fn run_agents_single_cli(
    conn: &mut Connection,
    run_id: &str,
    objective: &str,
    work_plan: &[Vec<AgentType>],
    provider: &dyn crate::providers::PersistentPhaseProvider,
    _cfg: &GroveConfig,
    project_root: &Path,
    model: Option<&str>,
    run_worktree_path: &Path,
    pause_after: &[AgentType],
    conversation_id: Option<&str>,
    abort_handle: Option<&super::abort_handle::AbortHandle>,
    sink: &dyn StreamSink,
    project_configs: Option<&crate::config::agent_config::ProjectConfigs>,
    is_git: bool,
    last_good_checkpoint: &mut Option<String>,
    db_path: &Path,
) -> GroveResult<()> {
    // Compute artifacts dir for the single-CLI pipeline.
    let artifacts_dir = if let Some(cid) = conversation_id {
        crate::config::paths::run_artifacts_dir(project_root, cid, run_id)
    } else {
        crate::config::paths::grove_dir(project_root)
            .join("artifacts")
            .join("_no_conv")
            .join(run_id)
    };
    let _ = std::fs::create_dir_all(&artifacts_dir);

    // 1. Pre-build all stage instructions in the DB.
    prebuild_pipeline_stages(
        conn,
        run_id,
        objective,
        work_plan,
        pause_after,
        run_worktree_path,
        project_root,
        project_configs,
        &artifacts_dir,
    )?;

    // 2. Prepare MCP config for the pipeline worker.
    // Use "grove-run" mode so the agent gets pipeline stage tools
    // (grove_get_pipeline_stage, grove_complete_pipeline_stage, grove_check_pipeline_gate)
    // instead of graph-only tools (grove_create_graph, grove_add_phase, etc.).
    let mcp_config_path = crate::providers::mcp_inject::write_mcp_config_file_named(
        "grove-run",
        &crate::providers::mcp_inject::resolve_mcp_server_binary().ok_or_else(|| {
            GroveError::Runtime(
                "grove-mcp-server binary not found — cannot use single-CLI pipeline mode".into(),
            )
        })?,
        db_path,
    )?;

    // 3. Set up the persistent host with MCP config.
    let session_log_dir = crate::config::paths::logs_dir(project_root)
        .join("runs")
        .join(run_id);
    let log_dir_str = session_log_dir.to_string_lossy().to_string();

    let persistent_tools = persistent_allowed_tools_union(work_plan, project_configs);
    let mcp_config_str = mcp_config_path.to_string_lossy().to_string();
    let mut host = provider.start_host(
        run_id,
        &run_worktree_path.to_string_lossy(),
        model,
        Some(&persistent_tools),
        Some(&log_dir_str),
        Some(&mcp_config_str),
    )?;

    // 4. Create the session record.
    let session_id = format!("sess_{}", Uuid::new_v4().simple());
    insert_session(
        conn,
        &session_id,
        run_id,
        "pipeline_worker",
        AgentState::Queued.as_str(),
        run_worktree_path.to_string_lossy().as_ref(),
    )?;
    set_session_state(conn, &session_id, AgentState::Running)?;
    events::emit(
        conn,
        run_id,
        Some(&session_id),
        crate::events::event_types::SESSION_SPAWNED,
        json!({ "agent_type": "pipeline_worker", "worktree": run_worktree_path, "mode": "single_cli" }),
    )?;

    sink.on_event(crate::providers::StreamOutputEvent::PhaseStart {
        phase: "pipeline_worker".to_string(),
        run_id: run_id.to_string(),
    });

    // 5. Build the pipeline-worker prompt with stage instructions inlined.
    let prompt = build_pipeline_worker_prompt(conn, run_id, objective, project_root);

    // 6. Self-heartbeat while the CLI is running.
    let hb_db_path = resolve_db_path(conn, project_root);
    let _hb_guard = spawn_heartbeat_guard(hb_db_path, session_id.clone());

    // 7. Execute the single persistent turn — the CLI self-navigates all stages.
    let qa_db_path = resolve_db_path(conn, project_root);
    let qa_source = {
        let mut qs = super::qa_source::DbQaSource::new(&qa_db_path);
        if let Some(h) = abort_handle {
            qs = qs.with_abort_handle(h.clone());
        }
        qs
    };

    let turn = crate::providers::claude_code_persistent::PhaseTurn {
        phase: "pipeline_worker".to_string(),
        instructions: prompt,
        gate_context: None,
    };

    let outcome =
        provider.execute_persistent_turn(&mut host, &turn, sink, &qa_source, &session_id)?;

    // 8. Process the outcome.
    match outcome {
        crate::providers::claude_code_persistent::PhaseTurnOutcome::HostDied {
            last_session_id,
            partial_output,
        } => {
            let host_error = GroveError::Runtime(format!(
                "pipeline worker host died (thread {:?}): {}",
                last_session_id, partial_output
            ));
            record_session_failure(
                conn,
                run_id,
                &session_id,
                &host_error,
                last_session_id.as_deref(),
            )?;
            fail_run(conn, run_id, objective, 0.0)?;
            return Err(host_error);
        }
        crate::providers::claude_code_persistent::PhaseTurnOutcome::TurnDone {
            response_text,
            cost_usd,
            session_id: provider_session_id,
            ..
        } => {
            // Record cost.
            if let Some(cost) = cost_usd {
                if cost > 0.0 {
                    conn.execute(
                        "UPDATE sessions SET cost_usd = ?1 WHERE id = ?2",
                        params![cost, session_id],
                    )?;
                }
            }
            let canonical_thread = provider_session_id
                .clone()
                .or_else(|| host.provider_thread_id.clone());
            if let Some(ref psid) = canonical_thread {
                conn.execute(
                    "UPDATE sessions SET provider_session_id = ?1 WHERE id = ?2",
                    params![psid, session_id],
                )?;
                crate::db::repositories::runs_repo::set_provider_thread_id(
                    conn,
                    run_id,
                    Some(psid),
                    &Utc::now().to_rfc3339(),
                )?;
            }

            // Record conversation message.
            if let Some(cid) = conversation_id {
                let summary: String = response_text.chars().take(500).collect();
                let _ = super::conversation::record_agent_message(
                    conn,
                    cid,
                    run_id,
                    "pipeline_worker",
                    &session_id,
                    &summary,
                );
            }

            // Budget recording.
            let response = crate::providers::ProviderResponse {
                summary: response_text,
                changed_files: vec![],
                cost_usd,
                provider_session_id: canonical_thread,
                pid: host.pid,
            };
            let _ = budget_meter::record(conn, run_id, &response)?;

            set_session_state(conn, &session_id, AgentState::Completed)?;
            sink.on_event(crate::providers::StreamOutputEvent::PhaseEnd {
                phase: "pipeline_worker".to_string(),
                run_id: run_id.to_string(),
                outcome: "completed".to_string(),
            });
        }
    }

    // 9. Post-processing: commit any remaining work, update checkpoint.
    if is_git {
        let commit_msg = format!(
            "grove(pipeline_worker): {}",
            &objective[..60.min(objective.len())]
        );
        commit_agent_work(run_worktree_path, &commit_msg);
        if let Ok(sha) = worktree::git_ops::git_rev_parse_head(run_worktree_path) {
            *last_good_checkpoint = Some(sha);
        }
    }

    // 9b. Mark all pending pipeline stages as completed.
    // In single-CLI pipeline mode the agent executes all stages in one turn but
    // cannot update the DB directly (sandbox restrictions / no MCP pipeline tools).
    // The engine is responsible for advancing stage status after a successful turn.
    let now = chrono::Utc::now().to_rfc3339();
    let stages_updated = conn
        .execute(
            "UPDATE pipeline_stages SET status = 'completed', completed_at = ?1 \
         WHERE run_id = ?2 AND status IN ('pending', 'inprogress')",
            params![now, run_id],
        )
        .unwrap_or(0);
    if stages_updated > 0 {
        tracing::info!(
            run_id = %run_id,
            stages_updated = stages_updated,
            "engine marked pipeline stages as completed after successful turn"
        );
    }

    // 10. Check pipeline stage results and emit events.
    let completed_stages: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pipeline_stages WHERE run_id = ?1 AND status = 'completed'",
            params![run_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let total_stages: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pipeline_stages WHERE run_id = ?1",
            params![run_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let failed_stages: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pipeline_stages WHERE run_id = ?1 AND status = 'failed'",
            params![run_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    tracing::info!(
        run_id = %run_id,
        completed = completed_stages,
        failed = failed_stages,
        total = total_stages,
        "single-CLI pipeline execution complete"
    );

    if failed_stages > 0 || completed_stages < total_stages {
        fail_run(conn, run_id, objective, 0.0)?;
        return Err(GroveError::Runtime(format!(
            "pipeline incomplete: {completed_stages}/{total_stages} stages completed, {failed_stages} failed"
        )));
    }

    // 11. Clean up MCP config file.
    crate::providers::mcp_inject::cleanup_mcp_config(&mcp_config_path);

    host.shutdown(crate::providers::claude_code_persistent::HostState::Completed)?;
    Ok(())
}

/// Check whether single-CLI pipeline mode is available and beneficial.
///
/// Returns `true` when:
/// - The work plan has multiple agents (pipeline mode, not single-agent)
/// - The MCP server binary can be found
/// - The plan has no task decomposition stages (builder-only waves)
fn should_use_single_cli_pipeline(work_plan: &[Vec<AgentType>]) -> bool {
    // Only use single-CLI for multi-agent pipelines.
    let total_agents: usize = work_plan.iter().map(|s| s.len()).sum();
    if total_agents <= 1 {
        return false;
    }

    // Must have the MCP server binary available.
    if crate::providers::mcp_inject::resolve_mcp_server_binary().is_none() {
        return false;
    }

    true
}

fn persistent_allowed_tools_union(
    work_plan: &[Vec<AgentType>],
    project_configs: Option<&crate::config::agent_config::ProjectConfigs>,
) -> Vec<String> {
    let mut tools = Vec::new();
    for stage in work_plan {
        for agent in stage {
            let candidate_tools = project_configs
                .and_then(|cfg| cfg.agents.get(agent.as_str()))
                .and_then(|cfg| cfg.allowed_tools.clone())
                .or_else(|| agent.allowed_tools());
            if let Some(agent_tools) = candidate_tools {
                for tool in agent_tools {
                    if !tools.contains(&tool) {
                        tools.push(tool);
                    }
                }
            }
        }
    }
    tools
}

/// Maximum time to wait for a phase gate decision before timing out (seconds).
const PHASE_GATE_TIMEOUT_SECS: u64 = 3600; // 1 hour

fn wait_for_persistent_gate_decision(
    conn: &Connection,
    checkpoint_id: i64,
    run_id: &str,
    decision_rx: &mpsc::Receiver<crate::providers::claude_code_persistent::RunControlMessage>,
    abort_handle: Option<&super::abort_handle::AbortHandle>,
) -> GroveResult<PersistentGateDecision> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(PHASE_GATE_TIMEOUT_SECS);
    let mut fallback_poll_ticks: u32 = 0;

    loop {
        if let Some(h) = abort_handle {
            if h.is_aborted() {
                let _ = phase_checkpoints_repo::submit_decision(
                    conn,
                    checkpoint_id,
                    "skipped",
                    Some("run aborted"),
                );
                return Err(GroveError::Aborted);
            }
        }

        let db_state: String = conn
            .query_row("SELECT state FROM runs WHERE id=?1", [run_id], |r| r.get(0))
            .unwrap_or_default();
        if db_state == "paused" {
            let _ = phase_checkpoints_repo::submit_decision(
                conn,
                checkpoint_id,
                "skipped",
                Some("run paused"),
            );
            return Err(GroveError::Aborted);
        }

        match decision_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(crate::providers::claude_code_persistent::RunControlMessage::GateDecision {
                checkpoint_id: decided_id,
                decision,
                notes,
            }) if decided_id == checkpoint_id => {
                return Ok(PersistentGateDecision { decision, notes });
            }
            Ok(crate::providers::claude_code_persistent::RunControlMessage::Abort) => {
                let _ = phase_checkpoints_repo::submit_decision(
                    conn,
                    checkpoint_id,
                    "skipped",
                    Some("run aborted"),
                );
                return Err(GroveError::Aborted);
            }
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {}
        }

        fallback_poll_ticks += 1;
        if fallback_poll_ticks >= 10 {
            fallback_poll_ticks = 0;
            let status: String = conn
                .query_row(
                    "SELECT status FROM phase_checkpoints WHERE id = ?1",
                    [checkpoint_id],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| "pending".to_string());
            if status != "pending" {
                let notes = phase_checkpoints_repo::get_notes(conn, checkpoint_id);
                return Ok(PersistentGateDecision {
                    decision: status,
                    notes,
                });
            }
        }

        if start.elapsed() > timeout {
            tracing::warn!(
                checkpoint_id,
                "persistent phase gate timed out after {} seconds — auto-approving",
                PHASE_GATE_TIMEOUT_SECS
            );
            let _ = phase_checkpoints_repo::submit_decision(
                conn,
                checkpoint_id,
                "approved",
                Some("auto-approved: timeout"),
            );
            return Ok(PersistentGateDecision {
                decision: "approved".to_string(),
                notes: Some("auto-approved: timeout".into()),
            });
        }
    }
}

// ── Task decomposition waves ──────────────────────────────────────────────────

/// Run all waves of decomposed sub-tasks produced by the architect.
/// Each wave's tasks run in parallel (bounded by `max_agents`); waves are sequential.
#[allow(clippy::too_many_arguments)]
fn run_task_waves(
    conn: &mut Connection,
    run_id: &str,
    objective: &str,
    decomp: &TaskDecomposition,
    cfg: &GroveConfig,
    provider: Arc<dyn Provider>,
    model: Option<&str>,
    previous_worktree: &mut Option<PathBuf>,
    last_good_worktree: &mut Option<PathBuf>,
    last_good_checkpoint: &mut Option<String>,
    run_worktree_path: &Path,
    is_git: bool,
    max_agents: usize,
    project_root: &Path,
    project_configs: Option<&crate::config::agent_config::ProjectConfigs>,
    run_artifacts_dir: &Path,
) -> GroveResult<()> {
    let waves = task_decomposer::compute_waves(&decomp.tasks)?;
    let total_waves = waves.len();

    for (wave_idx, task_indices) in waves.iter().enumerate() {
        let wave_tasks: Vec<&TaskSpec> = task_indices.iter().map(|&i| &decomp.tasks[i]).collect();

        tracing::info!(
            wave = wave_idx + 1,
            total_waves,
            tasks = wave_tasks.len(),
            "executing task wave"
        );

        // All tasks run sequentially — cap to max_agents per sub-wave.
        for chunk in wave_tasks.chunks(max_agents.max(1)) {
            run_task_wave(
                conn,
                run_id,
                objective,
                chunk,
                cfg,
                Arc::clone(&provider),
                model,
                previous_worktree,
                last_good_worktree,
                last_good_checkpoint,
                run_worktree_path,
                is_git,
                project_root,
                project_configs,
                run_artifacts_dir,
            )?;
        }
    }

    Ok(())
}

/// Run a single wave of tasks sequentially on the shared worktree.
#[allow(clippy::too_many_arguments)]
fn run_task_wave(
    conn: &mut Connection,
    run_id: &str,
    objective: &str,
    wave_tasks: &[&TaskSpec],
    cfg: &GroveConfig,
    provider: Arc<dyn Provider>,
    model: Option<&str>,
    previous_worktree: &mut Option<PathBuf>,
    last_good_worktree: &mut Option<PathBuf>,
    last_good_checkpoint: &mut Option<String>,
    run_worktree_path: &Path,
    is_git: bool,
    project_root: &Path,
    project_configs: Option<&crate::config::agent_config::ProjectConfigs>,
    artifacts_dir: &Path,
) -> GroveResult<()> {
    // All tasks run sequentially on the shared worktree — no parallel forks.
    for task in wave_tasks {
        let session_id = format!("sess_{}", Uuid::new_v4().simple());

        // Insert session first — subtasks.session_id FK references sessions(id).
        insert_session(
            conn,
            &session_id,
            run_id,
            "builder",
            "queued",
            run_worktree_path.to_string_lossy().as_ref(),
        )?;

        let subtask_id = format!("sub_{}_{}", run_id, task.id);
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE subtasks SET status='in_progress', session_id=?1, updated_at=?2 WHERE id=?3",
            params![session_id, now, subtask_id],
        )?;
        set_session_state(conn, &session_id, AgentState::Running)?;

        let resolved_model: Option<String> = if model.is_some() {
            model.map(str::to_string)
        } else if provider.name() == "claude_code" {
            cfg.agent_models.resolve("builder").map(str::to_string)
        } else {
            None
        };

        let instructions = build_instructions(
            AgentType::Builder,
            objective,
            None,
            None,
            run_id,
            run_worktree_path,
            Some(task),
            None,
            None,
            project_root,
            project_configs,
            artifacts_dir,
        );
        let timeout_secs = agent_timeout_secs(AgentType::Builder, cfg);
        let request = ProviderRequest {
            objective: objective.to_string(),
            role: "builder".to_string(),
            worktree_path: run_worktree_path.to_string_lossy().to_string(),
            instructions,
            model: resolved_model,
            allowed_tools: AgentType::Builder.allowed_tools(),
            timeout_override: Some(timeout_secs),
            provider_session_id: None,
            log_dir: None,
            grove_session_id: None,
            input_handle_callback: None,
            mcp_config_path: None,
        };

        let hb_db_path = resolve_db_path(conn, project_root);
        let _hb_guard = spawn_heartbeat_guard(hb_db_path, session_id.clone());
        let now_after = Utc::now().to_rfc3339();
        match provider.execute(&request) {
            Ok(response) => {
                if let Some(ref psid) = response.provider_session_id {
                    conn.execute(
                        "UPDATE sessions SET provider_session_id = ?1 WHERE id = ?2",
                        params![psid, session_id],
                    )?;
                }
                if let Some(pid) = response.pid {
                    let _ = conn.execute(
                        "UPDATE sessions SET pid = ?1 WHERE id = ?2",
                        params![pid as i64, session_id],
                    );
                }
                if let Some(cost) = response.cost_usd {
                    if cost > 0.0 {
                        conn.execute(
                            "UPDATE sessions SET cost_usd = ?1 WHERE id = ?2",
                            params![cost, session_id],
                        )?;
                    }
                }
                let _ = budget_meter::record(conn, run_id, &response)?;
                set_session_state(conn, &session_id, AgentState::Completed)?;
                events::emit(
                    conn,
                    run_id,
                    Some(&session_id),
                    crate::events::event_types::SESSION_STATE_CHANGED,
                    json!({ "state": "completed", "summary": response.summary }),
                )?;

                let summary_trunc: String = response.summary.chars().take(500).collect();
                conn.execute(
                    "UPDATE subtasks SET status='completed', result_summary=?1, updated_at=?2 WHERE id=?3",
                    params![summary_trunc, now_after, subtask_id],
                )?;

                // Commit agent work to the run worktree branch (non-fatal).
                let short_title: String = task.title.chars().take(60).collect();
                let commit_msg = format!("grove(builder): {short_title}");
                if !commit_agent_work(run_worktree_path, &commit_msg) {
                    tracing::warn!(
                        session = %session_id,
                        task = %task.id,
                        worktree = %run_worktree_path.display(),
                        "commit_agent_work failed for subtask — artifacts remain untracked"
                    );
                }

                // F11: Record checkpoint SHA for rollback.
                let parent_sha = last_good_checkpoint.clone();
                let checkpoint_sha = worktree::git_ops::git_rev_parse_head(run_worktree_path).ok();
                if let Some(ref sha) = checkpoint_sha {
                    conn.execute(
                        "UPDATE sessions SET checkpoint_sha = ?1, parent_checkpoint_sha = ?2 WHERE id = ?3",
                        params![sha, parent_sha, session_id],
                    )?;
                }
                *last_good_checkpoint = checkpoint_sha;
                *previous_worktree = Some(run_worktree_path.to_path_buf());
                *last_good_worktree = Some(run_worktree_path.to_path_buf());

                // F11: Clean worktree for next sequential task.
                if is_git {
                    let _ = worktree::git_ops::git_clean_worktree_verified(run_worktree_path);
                }
            }
            Err(err) => {
                set_session_state(conn, &session_id, AgentState::Failed)?;
                conn.execute(
                    "UPDATE subtasks SET status='failed', updated_at=?1 WHERE id=?2",
                    params![now_after, subtask_id],
                )?;
                events::emit(
                    conn,
                    run_id,
                    Some(&session_id),
                    crate::events::event_types::RUN_FAILED,
                    json!({ "error": err.to_string() }),
                )?;
                // F11: Reset to last good checkpoint on failure.
                if let Some(ref sha) = *last_good_checkpoint {
                    let _ = worktree::git_ops::git_reset_hard(run_worktree_path, sha);
                }
                return Err(err);
            }
        }
    } // end for task

    Ok(())
}

/// Commit agent work to the worktree branch (non-fatal).
///
/// Returns `true` if the commit succeeded, `false` otherwise. Callers should
/// not fail the run on a commit failure — the FS sync chain still works.
fn commit_agent_work(worktree_path: &Path, message: &str) -> bool {
    if !worktree::git_ops::is_git_repo(worktree_path) {
        return false;
    }
    worktree::git_ops::git_add_all(worktree_path)
        .and_then(|_| worktree::git_ops::git_commit(worktree_path, message))
        .is_ok()
}

fn ensure_conversation_branch_registered(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    run_id: &str,
    conversation_id: &str,
    branch: &str,
) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    let remote_branch = format!("origin/{branch}");
    let conv = crate::db::repositories::conversations_repo::get(conn, conversation_id)?;

    if !worktree::git_ops::git_remote_exists(project_root, "origin") {
        crate::db::repositories::conversations_repo::update_remote_registration(
            conn,
            conversation_id,
            "local_only",
            None,
            None,
            None,
        )?;
        return Ok(());
    }

    if worktree::git_ops::git_remote_branch_exists(project_root, "origin", branch) {
        crate::db::repositories::conversations_repo::update_remote_registration(
            conn,
            conversation_id,
            "registered",
            Some(&remote_branch),
            None,
            conv.remote_registered_at.as_deref().or(Some(&now)),
        )?;
        return Ok(());
    }

    match worktree::git_ops::git_register_branch_remote(project_root, "origin", branch) {
        Ok(_) => {
            crate::db::repositories::conversations_repo::update_remote_registration(
                conn,
                conversation_id,
                "registered",
                Some(&remote_branch),
                None,
                Some(&now),
            )?;
            let _ = events::emit(
                conn,
                run_id,
                None,
                "conv_branch_registered",
                json!({
                    "conversation_id": conversation_id,
                    "branch": branch,
                    "remote_branch": remote_branch,
                }),
            );
            let _ = post_branch_registration_write_back(
                conn,
                cfg,
                project_root,
                run_id,
                branch,
                &remote_branch,
            );
        }
        Err(err) => {
            let message = err.to_string();
            crate::db::repositories::conversations_repo::update_remote_registration(
                conn,
                conversation_id,
                "failed",
                Some(&remote_branch),
                Some(&message),
                conv.remote_registered_at.as_deref(),
            )?;
            let _ = events::emit(
                conn,
                run_id,
                None,
                "conv_branch_registration_failed",
                json!({
                    "conversation_id": conversation_id,
                    "branch": branch,
                    "remote_branch": remote_branch,
                    "error": message,
                }),
            );
        }
    }

    Ok(())
}

fn post_branch_registration_write_back(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    run_id: &str,
    branch: &str,
    remote_branch: &str,
) -> GroveResult<()> {
    if !cfg.tracker.write_back.enabled {
        return Ok(());
    }

    let issue_id: Option<String> = conn
        .query_row(
            "SELECT id FROM issues WHERE run_id = ?1 LIMIT 1",
            [run_id],
            |r| r.get(0),
        )
        .ok();
    let Some(issue_id) = issue_id else {
        return Ok(());
    };

    let ctx = crate::tracker::write_back::WriteBackContext {
        run_id: run_id.to_string(),
        issue_id,
        pr_url: None,
        cost_usd: 0.0,
        duration_secs: 0,
        agent_count: 0,
        error: None,
    };
    let body = format!("Conversation branch registered: `{branch}` (`{remote_branch}`).");
    let _ = crate::tracker::write_back::post_comment(
        conn,
        cfg,
        project_root,
        &ctx,
        &body,
        "branch_registered",
    );
    Ok(())
}
// ── helpers ───────────────────────────────────────────────────────────────────

/// Build a role-specific prompt for each agent type.
/// Each agent gets different instructions with run-scoped work file names
/// Build agent instructions using Markdown configs from `skills/agents/`.
///
/// Falls back to the hardcoded `instructions::build_agent_instructions()` if
/// no Markdown config exists for the agent. Appends file context and spawn
/// instructions to help agents orient in the worktree.
#[allow(clippy::too_many_arguments)]
fn build_instructions(
    agent_type: AgentType,
    objective: &str,
    shared_execution_context: Option<&str>,
    agent_brief: Option<&str>,
    run_id: &str,
    worktree: &Path,
    _task: Option<&TaskSpec>,
    _plan_step: Option<&PlanStep>,
    _failure_context: Option<&str>,
    project_root: &Path,
    preloaded_configs: Option<&crate::config::agent_config::ProjectConfigs>,
    artifacts_dir: &Path,
) -> String {
    // Try loading from Markdown configs first; falls back to hardcoded prompts.
    let mut prompt = crate::config::agent_config::build_instructions_from_config(
        agent_type,
        objective,
        run_id,
        artifacts_dir,
        None, // handoff_context — provided by the caller separately via feedback_prefix
        project_root,
        preloaded_configs,
    );

    // Append file listing so agents know what's in the worktree.
    let existing_files = walkdir_files(worktree);
    let file_context = if existing_files.is_empty() {
        String::from("\n\nThe working directory is currently empty.")
    } else {
        format!(
            "\n\nFiles currently in the working directory:\n{}",
            existing_files
                .iter()
                .map(|f| format!("  - {f}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    prompt.push_str(&file_context);
    if let Some(context) = shared_execution_context.filter(|s| !s.trim().is_empty()) {
        prompt.push_str("\n\nRun context:\n");
        prompt.push_str(context);
    }
    if let Some(brief) = agent_brief.filter(|s| !s.trim().is_empty()) {
        prompt.push_str("\n\nYour exact job in this phase:\n");
        prompt.push_str(brief);
        prompt.push_str("\nWork directly against the run objective and any prior handoff context.");
    }
    prompt.push_str(&agent_execution_guardrails(agent_type));
    prompt.push_str(spawn::spawn_instructions());
    prompt
}

fn agent_execution_guardrails(agent_type: AgentType) -> String {
    if !agent_type.can_run_commands() {
        return String::new();
    }

    format!(
        "\n\nExecution policy for {}:\n\
         - Use short-lived, one-shot commands only.\n\
         - Do not run Docker, docker-compose, podman, kubectl, minikube, colima, or similar container/orchestration commands.\n\
         - Do not start dev servers, watchers, background jobs, tailing commands, REPLs, or any long-running app/task.\n\
         - Avoid commands such as `npm run dev`, `vite`, `next dev`, `cargo watch`, `tail -f`, `sleep`, or anything that waits indefinitely.\n\
         - Prefer bounded verification commands that terminate on their own.\n\
         - If the task truly requires Docker or a long-running process, stop and report that limitation instead of running it.\n",
        agent_type.display_name()
    )
}

#[cfg(test)]
fn trim_prompt_text(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let char_count = normalized.chars().count();
    let mut trimmed: String = normalized.chars().take(max_chars).collect();
    if char_count > max_chars {
        trimmed.push_str("...");
    }
    trimmed
}

#[cfg(test)]
fn build_run_context_packet(
    conn: &Connection,
    run_id: &str,
    objective: &str,
    conversation_id: Option<&str>,
) -> Option<String> {
    let run_messages = conversation_id
        .and_then(|cid| {
            crate::db::repositories::messages_repo::list_for_conversation(conn, cid, 200).ok()
        })
        .unwrap_or_default();
    let completed_phases: Vec<String> = run_messages
        .iter()
        .filter(|msg| msg.run_id.as_deref() == Some(run_id) && msg.role == "agent")
        .filter_map(|msg| {
            msg.agent_type
                .as_ref()
                .map(|agent| format!("- {}: {}", agent, trim_prompt_text(&msg.content, 280)))
        })
        .collect();
    let recent_history: Vec<String> = run_messages
        .iter()
        .rev()
        .filter(|msg| {
            if msg.run_id.as_deref() == Some(run_id) {
                return false;
            }
            matches!(msg.role.as_str(), "user" | "agent" | "system")
        })
        .take(6)
        .map(|msg| {
            let speaker = match msg.role.as_str() {
                "agent" => msg
                    .agent_type
                    .as_deref()
                    .map(str::to_string)
                    .unwrap_or_else(|| "agent".to_string()),
                "system" => "system".to_string(),
                _ => "user".to_string(),
            };
            format!("- {}: {}", speaker, trim_prompt_text(&msg.content, 220))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let gate_decisions: Vec<String> = phase_checkpoints_repo::list_for_run(conn, run_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|cp| cp.status != "pending")
        .map(|cp| {
            let note = cp
                .decision
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(|s| format!(" note: {}", trim_prompt_text(s, 180)))
                .unwrap_or_default();
            format!("- {}: {}{}", cp.agent, cp.status, note)
        })
        .collect();
    let artifacts: Vec<String> = run_artifacts_repo::list_for_run(conn, run_id)
        .unwrap_or_default()
        .into_iter()
        .rev()
        .take(5)
        .map(|artifact| format!("- {}: {}", artifact.agent, artifact.filename))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if completed_phases.is_empty()
        && recent_history.is_empty()
        && gate_decisions.is_empty()
        && artifacts.is_empty()
    {
        return None;
    }

    let mut sections = vec![format!("Objective: {}", objective)];
    if !completed_phases.is_empty() {
        sections.push(format!(
            "Completed phases in this run:\n{}",
            completed_phases.join("\n")
        ));
    }
    if !gate_decisions.is_empty() {
        sections.push(format!("Gate decisions:\n{}", gate_decisions.join("\n")));
    }
    if !artifacts.is_empty() {
        sections.push(format!("Latest artifacts:\n{}", artifacts.join("\n")));
    }
    if !recent_history.is_empty() {
        sections.push(format!(
            "Recent conversation context:\n{}",
            recent_history.join("\n")
        ));
    }

    Some(format!(
        "Structured continuity context:\n{}\n\n",
        sections.join("\n\n")
    ))
}

/// List all file paths in a directory recursively, relative to that directory.
/// Directories that are always excluded from the file listing shown to agents,
/// regardless of whether a `.gitignore` exists in the worktree.
const ALWAYS_EXCLUDE_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".cargo",
    "vendor",
];

fn walkdir_files(root: &Path) -> Vec<String> {
    let filter = crate::worktree::gitignore::GitignoreFilter::load(root);
    walkdir_files_inner(root, root, &filter)
}

fn walkdir_files_inner(
    dir: &Path,
    root: &Path,
    filter: &crate::worktree::gitignore::GitignoreFilter,
) -> Vec<String> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = path.is_dir();
        if is_dir && ALWAYS_EXCLUDE_DIRS.contains(&name.as_str()) {
            continue;
        }
        // Check gitignore (relative path from root for correct pattern matching).
        let rel = path.strip_prefix(root).unwrap_or(&path);
        if filter.is_ignored(rel, is_dir) {
            continue;
        }
        if is_dir {
            for sub in walkdir_files_inner(&path, root, filter) {
                files.push(format!("{name}/{sub}"));
            }
        } else {
            files.push(name);
        }
    }
    files.sort();
    files
}

/// Find and claim the first unclaimed plan_step matching `agent_type`.
/// Appends the claimed step's ID to `claimed`, preventing double-claiming.
/// Returns `None` when no matching pending unclaimed step exists.
fn claim_plan_step<'a>(
    plan_steps: &'a [PlanStep],
    agent_type: AgentType,
    claimed: &mut Vec<String>,
) -> Option<&'a PlanStep> {
    let step = plan_steps.iter().find(|s| {
        s.agent_type == agent_type.as_str() && s.status == "pending" && !claimed.contains(&s.id)
    })?;
    claimed.push(step.id.clone());
    Some(step)
}

pub(super) fn insert_session(
    conn: &mut Connection,
    session_id: &str,
    run_id: &str,
    agent_type: &str,
    state: &str,
    worktree_path: &str,
) -> GroveResult<()> {
    // Best-effort: read the current git branch of the worktree. Falls back to
    // None when git is unavailable or the path is a detached HEAD.
    let branch: Option<String> =
        worktree::git_ops::git_current_branch(std::path::Path::new(worktree_path))
            .ok()
            .filter(|b| b != "HEAD");

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let now = Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO sessions
         (id, run_id, agent_type, state, worktree_path, started_at, ended_at, created_at, updated_at, provider_session_id, branch)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6, ?6, NULL, ?7)",
        params![session_id, run_id, agent_type, state, worktree_path, now, branch],
    )?;
    tx.commit()?;
    Ok(())
}

pub(super) fn set_session_state(
    conn: &Connection,
    session_id: &str,
    next: AgentState,
) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE sessions
         SET state = ?1,
             started_at = CASE WHEN ?1 = 'running' AND started_at IS NULL THEN ?2 ELSE started_at END,
             ended_at   = CASE WHEN ?1 IN ('completed', 'failed', 'killed') THEN ?2 ELSE ended_at END,
             updated_at = ?2
         WHERE id = ?3",
        params![next.as_str(), now, session_id],
    )?;
    Ok(())
}

/// Resolve the effective timeout for `agent_type`.
///
/// Priority: per-agent config timeout → global `providers.claude_code.timeout_seconds`.
pub(crate) fn agent_timeout_secs(
    agent_type: crate::agents::AgentType,
    cfg: &crate::config::GroveConfig,
) -> u64 {
    use crate::agents::AgentType as A;
    let global = cfg.providers.claude_code.timeout_seconds;
    match agent_type {
        A::BuildPrd => cfg.agents.prd.timeout_secs.max(global),
        A::PlanSystemDesign => cfg.agents.architect.timeout_secs.max(global),
        A::Builder => cfg.agents.builder.timeout_secs.max(global),
        A::Reviewer => cfg.agents.reviewer.timeout_secs.max(global),
        A::Judge => cfg.agents.judge.timeout_secs.max(global),
        // Graph agents use the global timeout as their default.
        A::PrePlanner | A::GraphCreator | A::Verdict | A::PhaseValidator | A::PhaseJudge => global,
    }
}

/// Build the full prompt for the conflict resolution agent.
///
/// Includes the embedded SKILL.md content and the specific context for this
/// resolution (which branch, which files).
fn build_conflict_resolution_instructions(
    conflicting_files: &[String],
    default_branch: &str,
) -> String {
    let file_list = conflicting_files
        .iter()
        .map(|f| format!("- {f}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"{CONFLICT_RESOLUTION_SKILL}

## Context for This Resolution

- **Upstream branch being merged:** `origin/{default_branch}`
- **Number of conflicting files:** {file_count}
- **Conflicting files:**
{file_list}

Resolve every conflict in the files listed above. Follow the process in the skill document exactly:
Assess → Resolve → Handle special cases → Validate → Stage.

Do NOT run `git commit` or `git merge --continue` — the engine handles that.
"#,
        file_count = conflicting_files.len(),
    )
}

/// Embedded conflict resolution agent skill content.
///
/// This is loaded as a constant rather than read from disk so it's always
/// available regardless of the working directory or file layout.
const CONFLICT_RESOLUTION_SKILL: &str = r#"# Merge Conflict Resolution Agent

The orchestrator merged `origin/main` into this conversation's branch and hit conflicts. Resolve every conflicted file, validate, and stage. The task agent is blocked until you succeed.

## Principles

1. **Preserve both sides.** Default is to keep changes from both the conversation branch and main, integrated cleanly.
2. **Never silently drop code.** If both sides added different things, keep all of them.
3. **Conversation branch wins ties.** When changes are genuinely incompatible and cannot be combined, prefer the conversation branch — it's the user's active work.
4. **Understand before editing.** Read the full file, not just the markers. Conflicts make sense only in context.
5. **The result must build.** A resolved file with broken syntax is worse than an unresolved conflict.

## Process

### Step 1: Assess

Before touching any file:

```
git diff --name-only --diff-filter=U           # all conflicted files
git log --oneline HEAD...MERGE_HEAD -- <file>  # what main changed
git diff <file>                                # full diff per file
```

Read each conflicted file fully. Categorize each conflict:

- **Additive**: Both sides added different things → keep both.
- **Divergent edit**: Same lines modified differently → combine intent, prefer conv branch if impossible.
- **Structural**: One side refactored, other made content changes → apply content onto new structure.
- **Delete vs modify**: Conv branch deleted → honor deletion unless main depends on it. Main deleted → conv branch likely still needs it, keep it.

### Step 2: Resolve

For each file: read entirely including markers, understand both sides' intent, write resolved version removing ALL markers, then check for syntax correctness, duplicate/missing imports, and orphaned references. Stage with `git add <file>`.

### Step 3: Special Cases

**Package files** (package.json, Cargo.toml): Merge dependency lists from both sides. Conflicting versions → prefer higher version.

**Lock files** (package-lock.json, Cargo.lock): Do NOT manually merge. Run `git checkout MERGE_HEAD -- <lockfile> && git add <lockfile>`. It regenerates on next install.

**Generated/binary files**: Accept main's version. They'll be regenerated.

**Config files**: Keep all keys from both sides. Same key, different values → prefer conv branch.

**Migration files**: Never merge contents. Keep both files, check sequence ordering.

**Whitespace-only conflicts**: Accept conv branch's version.

### Step 4: Validate

```
# No conflict markers remain (must return empty)
grep -rn '<<<<<<< \|=======$\|>>>>>>> ' --include='*.rs' --include='*.ts' --include='*.tsx' --include='*.js' --include='*.jsx' --include='*.py' --include='*.toml' --include='*.json' --include='*.yaml' --include='*.yml' --include='*.css' --include='*.html' --include='*.md' .

# No unresolved files remain (must return empty)
git diff --name-only --diff-filter=U

# Syntax check (run what's available)
cargo check 2>&1 | head -50        # Rust
npx tsc --noEmit 2>&1 | head -50   # TypeScript
python -m py_compile <file>         # Python
```

If ANY conflict markers or syntax errors remain, go back and fix them.

### Step 5: Stage

```
git add -A
git status  # should show no conflicts
```

Do NOT run `git commit` or `git merge --continue` — the engine handles that.

## Hard Boundaries

- **Never** run `git merge --abort` or `git commit`
- **Never** modify non-conflicted files
- **Never** delete a file to "resolve" a conflict
- **Never** leave any conflict marker in any file, not even in comments

## Reporting

State clearly: how many files resolved, one-line summary per file, whether all checks passed, and any concerns. If you cannot resolve a file, state which and why so the engine can surface it for manual resolution.
"#;

// ── Pre-publish pull ──────────────────────────────────────────────────────────

/// Pull the remote conversation branch before publishing, ensuring the push
/// is always a fast-forward. On conflict, invokes the conflict resolution
/// agent to resolve automatically.
#[allow(clippy::too_many_arguments)]
fn pull_remote_before_publish(
    conn: &mut Connection,
    run_id: &str,
    worktree_path: &Path,
    provider: &dyn Provider,
    cfg: &GroveConfig,
    model: Option<&str>,
    conv_branch: &str,
) -> GroveResult<()> {
    use crate::events::event_types;
    use crate::worktree::git_ops::{PullOutcome, git_pull_conv_branch};

    let remote = &cfg.publish.remote;
    let pull_result = match git_pull_conv_branch(worktree_path, remote, conv_branch) {
        Ok(outcome) => outcome,
        Err(e) => {
            // Fetch failure — non-fatal. Push may fail and be retried on startup.
            tracing::warn!(
                error = %e,
                remote = %remote,
                branch = %conv_branch,
                "pre-publish fetch failed — push may be rejected"
            );
            let _ = events::emit(
                conn,
                run_id,
                None,
                event_types::PRE_PUBLISH_PULL_SKIPPED,
                serde_json::json!({
                    "conv_branch": conv_branch,
                    "reason": format!("fetch failed: {e}"),
                }),
            );
            return Ok(());
        }
    };

    match pull_result {
        PullOutcome::NoRemote => {
            tracing::info!(
                branch = %conv_branch,
                "pre-publish pull: remote branch doesn't exist yet (first push)"
            );
            let _ = events::emit(
                conn,
                run_id,
                None,
                event_types::PRE_PUBLISH_PULL_SKIPPED,
                serde_json::json!({
                    "conv_branch": conv_branch,
                    "reason": "no_remote",
                }),
            );
            Ok(())
        }
        PullOutcome::UpToDate => {
            tracing::debug!(
                branch = %conv_branch,
                "pre-publish pull: already up-to-date with remote"
            );
            Ok(())
        }
        PullOutcome::Merged { merge_commit_sha } => {
            tracing::info!(
                branch = %conv_branch,
                merge_commit = %merge_commit_sha,
                "pre-publish pull: cleanly merged remote conv branch"
            );
            let _ = events::emit(
                conn,
                run_id,
                None,
                event_types::PRE_PUBLISH_PULL_CLEAN,
                serde_json::json!({
                    "conv_branch": conv_branch,
                    "merge_commit_sha": merge_commit_sha,
                }),
            );
            Ok(())
        }
        PullOutcome::Conflict { conflicting_files } => {
            tracing::warn!(
                branch = %conv_branch,
                file_count = conflicting_files.len(),
                files = %conflicting_files.join(", "),
                "pre-publish pull conflict — invoking conflict resolution agent"
            );
            let _ = events::emit(
                conn,
                run_id,
                None,
                event_types::PRE_PUBLISH_PULL_CONFLICT,
                serde_json::json!({
                    "conv_branch": conv_branch,
                    "conflicting_files": conflicting_files,
                    "file_count": conflicting_files.len(),
                }),
            );

            // Invoke conflict resolution agent (reuse pattern from pre-run merge).
            let cr_instructions =
                build_pre_publish_conflict_instructions(&conflicting_files, conv_branch);
            let cr_objective = format!(
                "Resolve {} merge conflict(s) with remote {}",
                conflicting_files.len(),
                conv_branch,
            );
            let cr_timeout = cfg.worktree.pull_before_publish_timeout_secs;
            let cr_request = ProviderRequest {
                objective: cr_objective,
                role: "builder".to_string(),
                worktree_path: worktree_path.to_string_lossy().to_string(),
                instructions: cr_instructions,
                model: model.map(|m| m.to_string()),
                allowed_tools: crate::agents::AgentType::Builder.allowed_tools(),
                timeout_override: Some(cr_timeout),
                provider_session_id: None,
                log_dir: None,
                grove_session_id: None,
                input_handle_callback: None,
                mcp_config_path: None,
            };

            let cr_result = provider.execute(&cr_request);

            // Record cost regardless of success/failure.
            if let Ok(ref response) = cr_result {
                let _ = budget_meter::record(conn, run_id, response);
            }

            // Validate resolution and finalize merge.
            let merge_finalized =
                cr_result.is_ok() && worktree::git_ops::git_merge_continue(worktree_path).is_ok();

            if merge_finalized {
                let merge_sha =
                    worktree::git_ops::git_rev_parse_head(worktree_path).unwrap_or_default();
                tracing::info!(
                    branch = %conv_branch,
                    merge_commit = %merge_sha,
                    resolved_files = %conflicting_files.join(", "),
                    "pre-publish conflict resolution succeeded"
                );
                let cost = cr_result.as_ref().ok().and_then(|r| r.cost_usd);
                let _ = events::emit(
                    conn,
                    run_id,
                    None,
                    event_types::PRE_PUBLISH_PULL_RESOLVED,
                    serde_json::json!({
                        "conv_branch": conv_branch,
                        "merge_commit_sha": merge_sha,
                        "resolved_files": conflicting_files,
                        "resolution_cost_usd": cost,
                    }),
                );
                Ok(())
            } else {
                let reason = if let Err(ref e) = cr_result {
                    format!("conflict resolution agent failed: {e}")
                } else {
                    "conflict markers remain after resolution attempt".to_string()
                };
                let _ = worktree::git_ops::git_merge_abort(worktree_path);
                tracing::error!(
                    branch = %conv_branch,
                    reason = %reason,
                    "pre-publish conflict resolution failed — merge aborted"
                );
                let _ = events::emit(
                    conn,
                    run_id,
                    None,
                    event_types::PRE_PUBLISH_PULL_FAILED,
                    serde_json::json!({
                        "conv_branch": conv_branch,
                        "unresolved_files": conflicting_files,
                        "reason": reason,
                    }),
                );
                Err(GroveError::Runtime(format!(
                    "pre-publish pull conflict resolution failed: {reason}"
                )))
            }
        }
    }
}

/// Build instructions for the conflict resolution agent during pre-publish pull.
///
/// Similar to `build_conflict_resolution_instructions` but the context says
/// "remote conversation branch" instead of "origin/main".
fn build_pre_publish_conflict_instructions(
    conflicting_files: &[String],
    conv_branch: &str,
) -> String {
    let file_list = conflicting_files
        .iter()
        .map(|f| format!("- {f}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"{CONFLICT_RESOLUTION_SKILL}

## Context for This Resolution

- **Source of conflict:** remote conversation branch `origin/{conv_branch}` has diverged from local
- **Number of conflicting files:** {file_count}
- **Conflicting files:**
{file_list}

Resolve every conflict in the files listed above. Follow the process in the skill document exactly:
Assess → Resolve → Handle special cases → Validate → Stage.

Do NOT run `git commit` or `git merge --continue` — the engine handles that.
"#,
        file_count = conflicting_files.len(),
    )
}

/// Build instructions for the push recovery agent.
pub(crate) fn build_push_recovery_instructions(
    failed_command: &str,
    error_output: &str,
    worktree_path: &Path,
    branch: &str,
) -> String {
    format!(
        r#"{PUSH_RECOVERY_SKILL}

## Context for This Recovery

- **Failed command:** `{failed_command}`
- **Error output:**
```
{error_output}
```
- **Worktree path:** `{wt}`
- **Branch:** `{branch}`

Diagnose why the push failed, apply a minimal fix, then the engine will retry the push.
Do NOT run the push yourself — the engine handles that.
"#,
        wt = worktree_path.display(),
    )
}

/// Embedded push recovery agent skill content.
const PUSH_RECOVERY_SKILL: &str = r#"# Git Push Recovery Agent

The engine's push to remote failed. Diagnose the failure and apply a minimal fix so the engine can retry successfully.

## Scope

You are restricted to diagnosing and fixing push failures ONLY. Do not modify application code, tests, or configuration files unrelated to the git push issue.

## Common Failures & Fixes

### Non-fast-forward (after ff-only pull also failed)
1. Check `git status` for merge state
2. Check `git log --oneline -10` vs `git log --oneline origin/<branch> -10` to understand divergence
3. If a merge is in progress, resolve conflicts, stage, and commit
4. If detached HEAD, reattach: `git checkout <branch>`

### Detached HEAD
1. `git branch` to find the target branch
2. `git checkout <branch>` to reattach
3. If commits were made on detached HEAD, `git branch temp-recovery` then `git checkout <branch> && git merge temp-recovery`

### Stale Lock
1. Check for `.git/index.lock` or `.git/refs/heads/<branch>.lock`
2. Verify no other git process is running
3. Remove stale lock file

### Upstream Tracking
1. `git branch -vv` to check tracking
2. If no upstream: `git branch --set-upstream-to=origin/<branch>`

## Hard Boundaries

- **Never** use `git push --force` or `git push --force-with-lease`
- **Never** use `git reset --hard`
- **Never** modify credentials or auth configuration
- **Never** modify application source code
- **Never** delete branches

## Reporting

State clearly: what the root cause was, what fix was applied, and whether the push should succeed on retry.
"#;

/// Try to locate the artifact file produced by a given agent in the artifacts directory.
///
/// Agents write artifacts to `.grove/artifacts/{conversation_id}/{run_id}/` with
/// run-id-stamped names (e.g. `GROVE_PRD_{short_id}.md`). We check the canonical
/// name first, then fall back to legacy / ad-hoc names for backward compatibility.
fn find_agent_artifact(
    artifacts_dir: &Path,
    agent_type: AgentType,
    run_id: &str,
) -> Option<String> {
    let short_id = if run_id.len() >= 8 {
        &run_id[..8]
    } else {
        run_id
    };

    // Canonical name from AgentType::artifact_filename, plus legacy fallbacks.
    let owned_candidates: Vec<String>;
    let candidates: Vec<&str> = match agent_type {
        AgentType::BuildPrd => {
            owned_candidates = vec![format!("GROVE_PRD_{short_id}.md")];
            owned_candidates
                .iter()
                .map(|s| s.as_str())
                .chain(["PRD.md", "prd.md", "docs/PRD.md"].iter().copied())
                .collect()
        }
        AgentType::PlanSystemDesign => {
            owned_candidates = vec![format!("GROVE_DESIGN_{short_id}.md")];
            owned_candidates
                .iter()
                .map(|s| s.as_str())
                .chain(
                    [
                        "DESIGN.md",
                        "design.md",
                        "SYSTEM_DESIGN.md",
                        "docs/DESIGN.md",
                    ]
                    .iter()
                    .copied(),
                )
                .collect()
        }
        AgentType::Reviewer => {
            owned_candidates = vec![format!("GROVE_REVIEW_{short_id}.md")];
            owned_candidates
                .iter()
                .map(|s| s.as_str())
                .chain(["REVIEW.md", "review.md", "REVIEW"].iter().copied())
                .collect()
        }
        AgentType::Judge => {
            owned_candidates = vec![format!("GROVE_VERDICT_{short_id}.md")];
            owned_candidates
                .iter()
                .map(|s| s.as_str())
                .chain(
                    ["JUDGE_VERDICT.md", "judge_verdict.md", "JUDGE_VERDICT"]
                        .iter()
                        .copied(),
                )
                .collect()
        }
        AgentType::Builder => {
            // Builder doesn't produce a document artifact — it writes code.
            return None;
        }
        AgentType::PrePlanner => {
            owned_candidates = vec![format!("PREPLAN_{short_id}.md")];
            owned_candidates.iter().map(|s| s.as_str()).collect()
        }
        AgentType::GraphCreator => {
            owned_candidates = vec![format!("GRAPH_SPEC_{short_id}.json")];
            owned_candidates.iter().map(|s| s.as_str()).collect()
        }
        AgentType::Verdict => {
            owned_candidates = vec![format!("VERDICT_{short_id}.json")];
            owned_candidates.iter().map(|s| s.as_str()).collect()
        }
        AgentType::PhaseValidator => {
            owned_candidates = vec![format!("PHASE_VAL_{short_id}.json")];
            owned_candidates.iter().map(|s| s.as_str()).collect()
        }
        AgentType::PhaseJudge => {
            owned_candidates = vec![format!("PHASE_JUDGE_{short_id}.json")];
            owned_candidates.iter().map(|s| s.as_str()).collect()
        }
    };

    for candidate in &candidates {
        let path = artifacts_dir.join(candidate);
        if path.exists() {
            return Some(candidate.to_string());
        }
    }
    None
}

fn fail_run(conn: &Connection, run_id: &str, objective: &str, budget_usd: f64) -> GroveResult<()> {
    let cp_id = format!("cp_{}", Uuid::new_v4().simple());
    let payload = CheckpointPayload {
        run_id: run_id.to_string(),
        stage: "failed".to_string(),
        active_sessions: vec![],
        pending_tasks: vec![objective.to_string()],
        ownership: vec![],
        budget: BudgetSnapshot {
            allocated_usd: budget_usd,
            used_usd: 0.0,
        },
    };
    checkpoint::save(conn, &cp_id, &payload)?;
    // Release all ownership locks — run is done (failed).
    let _ = ownership_repo::release_all_for_run(conn, run_id);
    events::emit(
        conn,
        run_id,
        None,
        crate::events::event_types::CHECKPOINT_SAVED,
        json!({ "checkpoint_id": cp_id }),
    )?;
    Ok(())
}

fn record_session_failure(
    conn: &Connection,
    run_id: &str,
    session_id: &str,
    error: &GroveError,
    provider_session_id: Option<&str>,
) -> GroveResult<()> {
    set_session_state(conn, session_id, AgentState::Failed)?;
    if let Some(psid) = provider_session_id {
        let _ = conn.execute(
            "UPDATE sessions SET provider_session_id = ?1 WHERE id = ?2",
            params![psid, session_id],
        );
    }
    let error_text = error.to_string();
    events::emit(
        conn,
        run_id,
        Some(session_id),
        crate::events::event_types::SESSION_STATE_CHANGED,
        json!({
            "state": "failed",
            "error": error_text.clone(),
            "provider_session_id": provider_session_id,
        }),
    )?;
    events::emit(
        conn,
        run_id,
        Some(session_id),
        crate::events::event_types::RUN_FAILED,
        json!({
            "error": error_text,
            "provider_session_id": provider_session_id,
        }),
    )?;
    Ok(())
}

fn is_retryable_session_error(err: &GroveError) -> bool {
    match err {
        GroveError::Aborted => false,
        GroveError::Runtime(msg) => !msg.contains("agent process idle — no output for"),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_run_context_packet, is_retryable_session_error};
    use crate::db;
    use crate::errors::GroveError;
    use chrono::Utc;

    #[test]
    fn idle_timeout_errors_are_not_retryable() {
        let err = GroveError::Runtime(
            "agent process idle — no output for 300 seconds; process killed".into(),
        );
        assert!(!is_retryable_session_error(&err));
    }

    #[test]
    fn aborted_errors_are_not_retryable() {
        assert!(!is_retryable_session_error(&GroveError::Aborted));
    }

    #[test]
    fn generic_runtime_errors_remain_retryable() {
        let err = GroveError::Runtime("temporary provider hiccup".into());
        assert!(is_retryable_session_error(&err));
    }

    #[test]
    fn structured_context_packet_includes_run_progress_and_history() {
        let dir = tempfile::TempDir::new().unwrap();
        db::initialize(dir.path()).unwrap();
        let conn_handle = db::DbHandle::new(dir.path());
        let conn = conn_handle.connect().unwrap();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO conversations (id, project_id, state, created_at, updated_at)
             VALUES ('conv1', 'proj1', 'active', ?1, ?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, publish_status, conversation_id, created_at, updated_at)
             VALUES ('run1', 'ship feature', 'executing', 1.0, 0.0, 'pending_retry', 'conv1', ?1, ?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (id, conversation_id, run_id, role, agent_type, session_id, content, created_at, user_id)
             VALUES ('m1', 'conv1', NULL, 'user', NULL, NULL, 'previous user note', ?1, NULL)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (id, conversation_id, run_id, role, agent_type, session_id, content, created_at, user_id)
             VALUES ('m2', 'conv1', 'run1', 'agent', 'build_prd', 'sess1', 'drafted a product brief with milestones', ?1, NULL)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO phase_checkpoints (run_id, agent, status, decision, artifact_path)
             VALUES ('run1', 'build_prd', 'approved_with_note', 'tighten the scope', 'PRD.md')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO run_artifacts (run_id, agent, filename, content_hash, size_bytes)
             VALUES ('run1', 'build_prd', 'PRD.md', 'abc123', 42)",
            [],
        )
        .unwrap();

        let packet =
            build_run_context_packet(&conn, "run1", "ship feature", Some("conv1")).unwrap();
        assert!(packet.contains("Completed phases in this run"));
        assert!(packet.contains("build_prd"));
        assert!(packet.contains("Gate decisions"));
        assert!(packet.contains("tighten the scope"));
        assert!(packet.contains("Latest artifacts"));
        assert!(packet.contains("Recent conversation context"));
    }
}
