use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::config::{BinaryStrategy, CapabilityGuard, MergeStrategy};
use crate::errors::{GroveError, GroveResult};
use crate::hooks::check_file_guard;
use crate::llm::anthropic::AnthropicProvider;
use crate::providers::{Provider, ProviderRequest};
use crate::worktree::git_ops::FileChangeStatus;
use crate::worktree::gitignore::GitignoreFilter;

// ── Known lockfiles ──────────────────────────────────────────────────────────

/// Well-known lockfile names mapped to their regeneration commands.
///
/// These are recognised by `is_lockfile()` and used as defaults by the engine's
/// `regenerate_lockfiles()` function. Users can override via `merge.lockfile_commands`.
pub const KNOWN_LOCKFILE_COMMANDS: &[(&str, &str)] = &[
    ("package-lock.json", "npm install --package-lock-only"),
    ("yarn.lock", "yarn install --mode update-lockfile"),
    ("pnpm-lock.yaml", "pnpm install --lockfile-only"),
    ("Cargo.lock", "cargo generate-lockfile"),
    ("Gemfile.lock", "bundle lock"),
    ("poetry.lock", "poetry lock --no-update"),
    ("composer.lock", "composer update --lock"),
    ("go.sum", "go mod tidy"),
    ("uv.lock", "uv lock"),
];

/// Returns `true` if `filename` is a known lockfile (case-sensitive).
pub fn is_lockfile(filename: &str) -> bool {
    KNOWN_LOCKFILE_COMMANDS
        .iter()
        .any(|(name, _)| *name == filename)
}

/// Copy `source` into a new isolated fork directory.
///
/// Creates `fork` if absent, then uses `sync_directories` to produce an exact
/// copy of `source`. This is the baseline each parallel agent works from.
pub fn fork_worktree(source: &Path, fork: &Path, filter: &GitignoreFilter) -> GroveResult<()> {
    std::fs::create_dir_all(fork)
        .map_err(|e| GroveError::Runtime(format!("mkdir {}: {e}", fork.display())))?;
    super::sync_directories(source, fork, filter)
}

// ── Conflict types ───────────────────────────────────────────────────────────

/// How a same-file conflict between agents was resolved.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ConflictResolution {
    /// Legacy: entire file overwritten by the last agent in merge order.
    LastWriterWins,
    /// 3-way merge succeeded — both agents' changes preserved automatically.
    ThreeWayClean,
    /// 3-way merge found irreconcilable differences — file has conflict markers.
    ThreeWayWithMarkers { marker_count: usize },
    /// Binary file — cannot text-merge, last writer wins.
    BinaryLastWriterWins,
    /// Binary file — kept base version (both agents' changes discarded).
    BinaryKeptBase,
    /// Claude resolved the conflict via the Anthropic API.
    AiResolved,
}

/// Records a file that was independently modified by more than one agent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConflictRecord {
    /// Relative path of the conflicting file.
    pub path: String,
    /// All agents that modified this file.
    pub agents: Vec<String>,
    /// How the conflict was resolved.
    pub resolution: ConflictResolution,
    /// Priority of the agent whose version won the conflict. `None` for
    /// `ThreeWayClean` resolutions where both changes are preserved.
    pub winning_agent_priority: Option<u8>,
}

impl ConflictRecord {
    /// Returns `true` if the conflict was automatically resolved without data loss.
    ///
    /// The engine uses this to decide whether to fail the run: only
    /// *unresolved* conflicts (where `is_resolved() == false`) are fatal.
    pub fn is_resolved(&self) -> bool {
        matches!(
            self.resolution,
            ConflictResolution::ThreeWayClean | ConflictResolution::AiResolved
        )
    }
}

/// Records a file that was excluded from the merge because the writing agent
/// lacked the capability to write it (as defined by `hooks.guards`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GuardViolationRecord {
    /// Relative path of the blocked file.
    pub path: String,
    /// The agent that attempted to write the file.
    pub agent: String,
}

// ── Agent worktree descriptor ────────────────────────────────────────────────

/// Describes one agent's worktree for the merge function.
///
/// Carries the agent name, worktree path, and optional base commit (recorded
/// at worktree creation) so the merge layer can use fast git-native change
/// detection instead of hashing every file.
pub struct AgentWorktree {
    pub name: String,
    pub path: PathBuf,
    /// The commit SHA the worktree was forked from. `None` for non-git repos
    /// or worktrees created before Phase 0 (no metadata file).
    pub base_commit: Option<String>,
    /// Merge priority: lower number = higher priority = applied last = wins.
    /// Defaults to 50 (mid-range) if not set by the engine.
    pub merge_priority: u8,
    /// Whether this worktree uses sparse checkout.
    pub is_sparse: bool,
    /// The sparse checkout patterns for this worktree. Empty if not sparse.
    /// Used during merge to distinguish "file not checked out" from "file deleted".
    pub sparse_patterns: Vec<String>,
}

// ── Merge metrics ────────────────────────────────────────────────────────────

/// Structured metrics emitted after every merge for observability.
///
/// Designed to be collected by OpenTelemetry or parsed from structured logs.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MergeMetrics {
    /// Which merge strategy was actually used (`"three_way"` or `"last_writer_wins"`).
    pub strategy_used: String,
    /// Total files evaluated for changes (across all agent worktrees).
    pub files_processed: usize,
    /// Files that were actually different from the base.
    pub files_changed: usize,
    /// Total conflict records produced.
    pub conflicts_total: usize,
    /// Conflicts that were automatically resolved (ThreeWayClean).
    pub conflicts_auto_resolved: usize,
    /// Conflicts that have markers or need manual intervention.
    pub conflicts_unresolved: usize,
    /// Wall-clock merge duration in milliseconds.
    pub duration_ms: u64,
    /// If the strategy fell back from ThreeWay to LastWriterWins, explains why.
    pub fallback_reason: Option<String>,
    /// Which change detection method was used: `"diff-tree"`, `"working-tree"`,
    /// `"hash-walk"`, or `"mixed"` if different worktrees used different methods.
    pub change_detection_strategy: String,
    /// Time spent on change detection alone (milliseconds).
    pub change_detection_ms: u64,
    /// How many agent worktrees used sparse checkout.
    pub sparse_worktrees: usize,
    /// Total files materialised across all sparse worktrees.
    pub sparse_files_materialized: usize,
    /// Whether crash recovery was triggered before this merge.
    pub crash_recovery_triggered: bool,
    /// Files excluded from the merge due to capability guard violations.
    pub guard_violations_total: usize,
}

// ── Merge result ─────────────────────────────────────────────────────────────

/// Summary of a parallel worktree merge.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// Every file that was modified by more than one agent, with resolution details.
    pub conflicts: Vec<ConflictRecord>,
    /// Structured metrics for observability.
    pub metrics: MergeMetrics,
    /// Lockfiles that were modified by any agent and may need regeneration.
    pub modified_lockfiles: Vec<String>,
    /// Files excluded from the merge because the agent lacked write capability.
    pub guard_violations: Vec<GuardViolationRecord>,
}

impl MergeResult {
    /// Returns `true` if there are conflicts that were NOT automatically resolved.
    ///
    /// The engine should fail the run only when this returns `true`.
    pub fn has_unresolved_conflicts(&self) -> bool {
        self.conflicts.iter().any(|c| !c.is_resolved())
    }

    /// Returns only the unresolved conflicts (for error reporting).
    pub fn unresolved_conflicts(&self) -> Vec<&ConflictRecord> {
        self.conflicts.iter().filter(|c| !c.is_resolved()).collect()
    }
}

// ── Binary detection ─────────────────────────────────────────────────────────

/// Returns `true` if the file appears to be binary (contains null bytes in first 8 KB).
///
/// Matches git's own heuristic for binary detection. A null byte in the first
/// 8 KB is a strong indicator of binary content (images, compiled assets,
/// serialized data). False positives are extremely rare for text files.
fn is_binary_file(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 8192];
    let Ok(n) = f.read(&mut buf) else {
        return false;
    };
    buf[..n].contains(&0)
}

// ── Main merge function ──────────────────────────────────────────────────────

/// Merge multiple agent worktrees into `dest`.
///
/// Algorithm:
/// 1. Initialise `dest` as an exact copy of `base` (the common ancestor).
/// 2. For each agent worktree, detect changed files using the fastest available
///    strategy: `git diff-tree` (committed) → `git diff HEAD` (uncommitted) →
///    hash-walk (fallback for non-git repos).
/// 3. Process only changed files:
///    - Added/modified → copy to `dest` (or 3-way merge on conflict).
///    - Deleted → remove from `dest`.
/// 4. Return [`MergeResult`] with all conflicts and metrics.
pub fn merge_worktrees(
    base: &Path,
    agent_worktrees: &[AgentWorktree],
    dest: &Path,
    filter: &GitignoreFilter,
    strategy: MergeStrategy,
    binary_strategy: BinaryStrategy,
    guards: &HashMap<String, CapabilityGuard>,
) -> GroveResult<MergeResult> {
    let start = Instant::now();

    // Resolve effective strategy: ThreeWay requires git on PATH.
    let (effective_strategy, fallback_reason) = resolve_strategy(strategy);

    // For AiResolve, construct the provider once and share it across all
    // conflict resolution calls. If the API key was already validated by
    // resolve_strategy, this will not fail at construction time.
    let ai_provider: Option<AnthropicProvider> = if effective_strategy == MergeStrategy::AiResolve {
        Some(AnthropicProvider::new())
    } else {
        None
    };

    // Initialise dest as an exact copy of base.
    std::fs::create_dir_all(dest)
        .map_err(|e| GroveError::Runtime(format!("mkdir {}: {e}", dest.display())))?;
    super::sync_directories(base, dest, filter)?;

    // Temp dir for empty-base files (when both agents create a new file).
    let tmp_dir =
        tempfile::tempdir().map_err(|e| GroveError::Runtime(format!("tempdir for merge: {e}")))?;

    let mut conflicts: Vec<ConflictRecord> = Vec::new();
    let mut guard_violations: Vec<GuardViolationRecord> = Vec::new();
    let mut written_by: HashMap<String, String> = HashMap::new();
    let mut files_processed: usize = 0;
    let mut files_changed: usize = 0;
    let mut modified_lockfiles: Vec<String> = Vec::new();

    // ── Sort by priority ────────────────────────────────────────────────────
    // Higher priority number = lower priority = applied FIRST (can be overwritten).
    // Lower priority number = higher priority = applied LAST (wins conflicts).
    // Alphabetical tiebreak for deterministic ordering.
    let mut sorted_indices: Vec<usize> = (0..agent_worktrees.len()).collect();
    sorted_indices.sort_by(|&a, &b| {
        let pa = agent_worktrees[a].merge_priority;
        let pb = agent_worktrees[b].merge_priority;
        pb.cmp(&pa)
            .then(agent_worktrees[a].name.cmp(&agent_worktrees[b].name))
    });

    // Build priority lookup for conflict records.
    let priority_by_name: HashMap<&str, u8> = agent_worktrees
        .iter()
        .map(|wt| (wt.name.as_str(), wt.merge_priority))
        .collect();

    // ── Change detection phase ──────────────────────────────────────────────
    let cd_start = Instant::now();
    let mut detection_strategies: Vec<&str> = Vec::new();
    let mut base_hashes: Option<HashMap<String, u64>> = None;

    // Detect changes for each worktree (in sorted order).
    let mut agent_changes: Vec<(usize, Vec<(FileChangeStatus, String)>)> = Vec::new();
    for &idx in &sorted_indices {
        let wt = &agent_worktrees[idx];
        let (changes, strategy_name) = detect_changes_for_worktree(
            &wt.path,
            wt.base_commit.as_deref(),
            base,
            filter,
            &mut base_hashes,
        )?;
        detection_strategies.push(strategy_name);
        files_processed += changes.len();
        agent_changes.push((idx, changes));
    }

    let change_detection_ms = cd_start.elapsed().as_millis() as u64;
    let change_detection_strategy = summarise_detection_strategies(&detection_strategies);

    // ── Merge phase ─────────────────────────────────────────────────────────
    for (idx, changes) in &agent_changes {
        let wt = &agent_worktrees[*idx];
        let agent_name = wt.name.as_str();

        for (status, rel) in changes {
            // 2.9: Enforce file-scope capability guards before accepting any
            // agent change. A violation means the agent was not configured to
            // write this path — exclude the file and record the violation so
            // callers can surface it in logs and events.
            if !check_file_guard(guards, agent_name, rel) {
                tracing::warn!(
                    file = %rel,
                    agent = %agent_name,
                    "file guard violation: agent not allowed to write this path — excluding from merge"
                );
                guard_violations.push(GuardViolationRecord {
                    path: rel.clone(),
                    agent: agent_name.to_string(),
                });
                continue;
            }

            match status {
                FileChangeStatus::Added | FileChangeStatus::Modified => {
                    files_changed += 1;
                    let src = wt.path.join(rel);
                    let dst = dest.join(rel);

                    // Track lockfiles that any agent modified.
                    let filename = Path::new(rel)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string());
                    if let Some(ref fname) = filename {
                        if is_lockfile(fname) && !modified_lockfiles.contains(rel) {
                            modified_lockfiles.push(rel.clone());
                        }
                    }

                    if let Some(prev_agent) = written_by.get(rel.as_str()) {
                        let agents = vec![prev_agent.clone(), agent_name.to_string()];

                        if let Some(parent) = dst.parent() {
                            std::fs::create_dir_all(parent).map_err(|e| {
                                GroveError::Runtime(format!("mkdir {}: {e}", parent.display()))
                            })?;
                        }

                        let resolution = resolve_file_conflict(
                            effective_strategy,
                            base,
                            rel,
                            &src,
                            &dst,
                            agent_name,
                            prev_agent,
                            tmp_dir.path(),
                            binary_strategy,
                            ai_provider.as_ref(),
                        );

                        let winning_priority = match resolution {
                            ConflictResolution::ThreeWayClean => None,
                            _ => Some(*priority_by_name.get(agent_name).unwrap_or(&50)),
                        };
                        conflicts.push(ConflictRecord {
                            path: rel.clone(),
                            agents,
                            resolution,
                            winning_agent_priority: winning_priority,
                        });
                        written_by.insert(rel.clone(), agent_name.to_string());
                        continue;
                    }

                    // No conflict — copy the file (or recreate symlink).
                    if let Some(parent) = dst.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            GroveError::Runtime(format!("mkdir {}: {e}", parent.display()))
                        })?;
                    }
                    copy_or_link(&src, &dst)?;
                    written_by.insert(rel.clone(), agent_name.to_string());
                }
                FileChangeStatus::Deleted => {
                    // Sparse safety: if this worktree is sparse and the file
                    // was never in its sparse set, this is NOT a real deletion —
                    // it's just a file that was never checked out.
                    if wt.is_sparse
                        && !crate::worktree::git_ops::matches_sparse_patterns(
                            rel,
                            &wt.sparse_patterns,
                        )
                    {
                        continue;
                    }

                    if let Some(prev_agent) = written_by.get(rel.as_str()) {
                        conflicts.push(ConflictRecord {
                            path: rel.clone(),
                            agents: vec![prev_agent.clone(), agent_name.to_string()],
                            resolution: ConflictResolution::LastWriterWins,
                            winning_agent_priority: Some(
                                *priority_by_name.get(agent_name).unwrap_or(&50),
                            ),
                        });
                        tracing::warn!(
                            file = %rel, deleter = %agent_name, writer = %prev_agent,
                            "merge conflict: deleting file written by another agent"
                        );
                    }

                    let dst = dest.join(rel);
                    let _ = std::fs::remove_file(&dst);
                    written_by.insert(rel.clone(), agent_name.to_string());
                }
            }
        }
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let conflicts_auto_resolved = conflicts.iter().filter(|c| c.is_resolved()).count();
    let conflicts_unresolved = conflicts.len() - conflicts_auto_resolved;

    let sparse_worktrees = agent_worktrees.iter().filter(|wt| wt.is_sparse).count();
    let sparse_files_materialized: usize = agent_changes
        .iter()
        .filter(|(idx, _)| agent_worktrees[*idx].is_sparse)
        .map(|(_, changes)| changes.len())
        .sum();

    let metrics = MergeMetrics {
        strategy_used: match effective_strategy {
            MergeStrategy::LastWriterWins => "last_writer_wins".to_string(),
            MergeStrategy::ThreeWay => "three_way".to_string(),
            MergeStrategy::AiResolve => "ai_resolve".to_string(),
        },
        files_processed,
        files_changed,
        conflicts_total: conflicts.len(),
        conflicts_auto_resolved,
        conflicts_unresolved,
        duration_ms,
        fallback_reason,
        change_detection_strategy,
        change_detection_ms,
        sparse_worktrees,
        sparse_files_materialized,
        crash_recovery_triggered: false,
        guard_violations_total: guard_violations.len(),
    };

    tracing::info!(
        merge.strategy = %metrics.strategy_used,
        merge.files_processed = metrics.files_processed,
        merge.files_changed = metrics.files_changed,
        merge.conflicts_total = metrics.conflicts_total,
        merge.conflicts_auto_resolved = metrics.conflicts_auto_resolved,
        merge.conflicts_unresolved = metrics.conflicts_unresolved,
        merge.guard_violations = metrics.guard_violations_total,
        merge.duration_ms = metrics.duration_ms,
        merge.fallback_reason = ?metrics.fallback_reason,
        merge.change_detection = %metrics.change_detection_strategy,
        merge.change_detection_ms = metrics.change_detection_ms,
        "merge completed"
    );

    Ok(MergeResult {
        conflicts,
        metrics,
        modified_lockfiles,
        guard_violations,
    })
}

/// Legacy entry point that wraps simple `(name, path)` tuples as `AgentWorktree`
/// with no base_commit (always falls back to hash-walk).
///
/// Used by tests and callers that don't have base_commit metadata.
pub fn merge_worktrees_simple(
    base: &Path,
    agent_worktrees: &[(&str, PathBuf)],
    dest: &Path,
    filter: &GitignoreFilter,
    strategy: MergeStrategy,
) -> GroveResult<MergeResult> {
    let count = agent_worktrees.len() as u8;
    let wrapped: Vec<AgentWorktree> = agent_worktrees
        .iter()
        .enumerate()
        .map(|(i, (name, path))| AgentWorktree {
            name: name.to_string(),
            path: path.clone(),
            base_commit: None,
            // Preserve original order: last agent in list gets lowest priority number
            // (= highest priority = applied last = wins). This maintains backward
            // compatibility where insertion order determines the winner.
            merge_priority: count - i as u8,
            is_sparse: false,
            sparse_patterns: Vec::new(),
        })
        .collect();
    merge_worktrees(
        base,
        &wrapped,
        dest,
        filter,
        strategy,
        BinaryStrategy::LastWriter,
        &HashMap::new(),
    )
}

// ── Change detection ─────────────────────────────────────────────────────────

/// Detect which files an agent changed, using the fastest available strategy.
///
/// Returns the change list and a strategy name for metrics.
///
/// Tier 1: `git diff-tree` — fast, compares committed changes.
/// Tier 2: `git diff HEAD` + `ls-files --others` — catches uncommitted work.
/// Tier 3: Hash-walk fallback — non-git repos or missing base_commit.
fn detect_changes_for_worktree(
    wt_path: &Path,
    base_commit: Option<&str>,
    base_dir: &Path,
    filter: &GitignoreFilter,
    base_hashes_cache: &mut Option<HashMap<String, u64>>,
) -> GroveResult<(Vec<(FileChangeStatus, String)>, &'static str)> {
    use super::git_ops;

    if let Some(base) = base_commit {
        // Tier 1: Try committed changes via diff-tree.
        if let Ok(head) = git_ops::git_rev_parse_head(wt_path) {
            if head != base {
                match git_ops::git_diff_names(wt_path, base, &head) {
                    Ok(changes) => {
                        tracing::debug!(
                            strategy = "diff-tree", base = %base, head = %head,
                            changes = changes.len(), "change detection"
                        );
                        return Ok((changes, "diff-tree"));
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "diff-tree failed, trying working tree diff");
                    }
                }
            }
        }

        // Tier 2: HEAD == base_commit or diff-tree failed. Check working tree.
        match git_ops::git_diff_working_tree(wt_path) {
            Ok(changes) if !changes.is_empty() => {
                tracing::warn!(
                    count = changes.len(),
                    "agent has uncommitted changes — using working tree diff"
                );
                return Ok((changes, "working-tree"));
            }
            Ok(_) => {
                // No changes at all — empty list.
                tracing::debug!("no committed or uncommitted changes detected");
                return Ok((Vec::new(), "diff-tree"));
            }
            Err(e) => {
                tracing::warn!(error = %e, "working tree diff failed, falling back to hash-walk");
            }
        }
    }

    // Tier 3: Emergency fallback — full hash walk.
    tracing::info!(
        strategy = "hash-walk",
        "falling back to full directory hash"
    );
    let changes = hash_walk_changes(wt_path, base_dir, filter, base_hashes_cache)?;
    Ok((changes, "hash-walk"))
}

/// Hash-walk fallback: compare all files in `wt_path` against `base_dir` by content hash.
///
/// Converts the result to `Vec<(FileChangeStatus, String)>` to unify with git-native paths.
fn hash_walk_changes(
    wt_path: &Path,
    base_dir: &Path,
    filter: &GitignoreFilter,
    base_hashes_cache: &mut Option<HashMap<String, u64>>,
) -> GroveResult<Vec<(FileChangeStatus, String)>> {
    // Lazily compute base hashes (shared across all agents in fallback mode).
    if base_hashes_cache.is_none() {
        *base_hashes_cache = Some(collect_file_hashes(base_dir, base_dir, filter));
    }
    let base_hashes = base_hashes_cache.as_ref().unwrap();
    let wt_hashes = collect_file_hashes(wt_path, wt_path, filter);

    let mut changes = Vec::new();

    // Added or modified files.
    for (rel, wt_hash) in &wt_hashes {
        match base_hashes.get(rel.as_str()) {
            None => changes.push((FileChangeStatus::Added, rel.clone())),
            Some(base_hash) if base_hash != wt_hash => {
                changes.push((FileChangeStatus::Modified, rel.clone()));
            }
            _ => {} // unchanged
        }
    }

    // Deleted files (in base but not in worktree).
    for rel in base_hashes.keys() {
        if !wt_hashes.contains_key(rel.as_str()) {
            changes.push((FileChangeStatus::Deleted, rel.clone()));
        }
    }

    Ok(changes)
}

/// Summarise per-worktree detection strategies into a single aggregate label.
fn summarise_detection_strategies(strategies: &[&str]) -> String {
    if strategies.is_empty() {
        return "none".to_string();
    }
    let first = strategies[0];
    if strategies.iter().all(|s| *s == first) {
        first.to_string()
    } else {
        "mixed".to_string()
    }
}

// ── Strategy resolution ──────────────────────────────────────────────────────

/// Resolve the effective merge strategy, falling back when prerequisites are absent.
///
/// - `ThreeWay` requires git on PATH; falls back to `LastWriterWins` if absent.
/// - `AiResolve` requires git AND an Anthropic API key.
///   Falls back to `ThreeWay` when the key is absent (and git is present),
///   or to `LastWriterWins` when both are absent.
fn resolve_strategy(requested: MergeStrategy) -> (MergeStrategy, Option<String>) {
    match requested {
        MergeStrategy::ThreeWay => {
            if !super::git_available() {
                tracing::warn!(
                    "merge strategy is 'three_way' but git is not on PATH — \
                     falling back to 'last_writer_wins'"
                );
                (
                    MergeStrategy::LastWriterWins,
                    Some("git not available on PATH".to_string()),
                )
            } else {
                (MergeStrategy::ThreeWay, None)
            }
        }
        MergeStrategy::AiResolve => {
            let git_ok = super::git_available();
            let key_ok = crate::llm::auth::AuthStore::get("anthropic").is_some();
            if key_ok && git_ok {
                (MergeStrategy::AiResolve, None)
            } else if key_ok {
                // Have key but no git — AI can still be used (no base merge needed).
                (MergeStrategy::AiResolve, None)
            } else if git_ok {
                tracing::warn!(
                    "merge strategy is 'ai_resolve' but no Anthropic API key found — \
                     falling back to 'three_way'. Set ANTHROPIC_API_KEY or run: \
                     grove auth set anthropic <key>"
                );
                (
                    MergeStrategy::ThreeWay,
                    Some("no Anthropic API key configured".to_string()),
                )
            } else {
                tracing::warn!(
                    "merge strategy is 'ai_resolve' but no API key and no git — \
                     falling back to 'last_writer_wins'"
                );
                (
                    MergeStrategy::LastWriterWins,
                    Some("no API key and no git on PATH".to_string()),
                )
            }
        }
        other => (other, None),
    }
}

// ── Per-file conflict resolution ─────────────────────────────────────────────

/// Resolve a same-file conflict between two agents.
///
/// For `ThreeWay`: uses `git merge-file` for text files, falls back to LWW for binary.
/// For `AiResolve`: sends base/ours/theirs to Claude and writes the response. Falls
///   back to `ThreeWay` on API failure, and then to `LastWriterWins` if git is absent.
/// For `LastWriterWins`: always overwrites with the current agent's version.
///
/// Writes the resolved content to `dst` and returns the resolution type.
#[allow(clippy::too_many_arguments)]
fn resolve_file_conflict(
    strategy: MergeStrategy,
    base_dir: &Path,
    rel: &str,
    src: &Path, // theirs: current agent's version
    dst: &Path, // ours: previous agent's version (already in dest)
    agent_name: &str,
    prev_agent: &str,
    tmp_dir: &Path,
    binary_strategy: BinaryStrategy,
    ai_provider: Option<&AnthropicProvider>,
) -> ConflictResolution {
    // Symlink check: symlinks can't be text-merged, always use LWW.
    let src_is_symlink = src
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    let dst_is_symlink = dst
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    if src_is_symlink || dst_is_symlink {
        tracing::warn!(
            file = %rel, winner = %agent_name, loser = %prev_agent,
            "merge conflict: symlink, using last-writer-wins"
        );
        if let Err(e) = copy_or_link(src, dst) {
            tracing::error!(file = %rel, error = %e, "failed to copy symlink during merge");
        }
        return ConflictResolution::LastWriterWins;
    }

    // Binary check applies to all strategies — honour configured BinaryStrategy.
    if is_binary_file(src) || is_binary_file(dst) {
        match binary_strategy {
            BinaryStrategy::LastWriter => {
                tracing::warn!(
                    file = %rel, winner = %agent_name, loser = %prev_agent,
                    "merge conflict: binary file, using last-writer-wins"
                );
                if let Err(e) = copy_or_link(src, dst) {
                    tracing::error!(file = %rel, error = %e, "failed to copy binary file during merge");
                }
                return ConflictResolution::BinaryLastWriterWins;
            }
            BinaryStrategy::KeepBase => {
                // Restore the base version into dest, discarding both agents' changes.
                let base_file = base_dir.join(rel);
                if base_file.exists() {
                    if let Err(e) = std::fs::copy(&base_file, dst) {
                        tracing::error!(file = %rel, error = %e, "failed to restore base version");
                    }
                }
                tracing::warn!(
                    file = %rel, agents = %format!("{prev_agent}, {agent_name}"),
                    "merge conflict: binary file, keeping base version"
                );
                return ConflictResolution::BinaryKeptBase;
            }
            BinaryStrategy::Fail => {
                tracing::error!(
                    file = %rel, agents = %format!("{prev_agent}, {agent_name}"),
                    "merge conflict: binary file, binary_strategy=fail"
                );
                // Write the last writer's version so the file is in a known state,
                // but mark as unresolved (not ThreeWayClean).
                if let Err(e) = copy_or_link(src, dst) {
                    tracing::error!(file = %rel, error = %e, "failed to copy binary file during merge");
                }
                return ConflictResolution::BinaryLastWriterWins;
            }
        }
    }

    match strategy {
        MergeStrategy::AiResolve => {
            if let Some(provider) = ai_provider {
                match resolve_with_ai(provider, rel, base_dir, src, dst, agent_name, prev_agent) {
                    Ok(resolution) => return resolution,
                    Err(e) => {
                        tracing::warn!(
                            file = %rel, error = %e,
                            "AI resolution failed — falling back to three_way"
                        );
                    }
                }
            }
            // Fallback: ThreeWay (provider was None or AI call failed).
            resolve_three_way(base_dir, rel, src, dst, agent_name, prev_agent, tmp_dir)
        }
        MergeStrategy::ThreeWay => {
            resolve_three_way(base_dir, rel, src, dst, agent_name, prev_agent, tmp_dir)
        }
        MergeStrategy::LastWriterWins => {
            tracing::warn!(
                file = %rel, winner = %agent_name, loser = %prev_agent,
                "merge conflict: overwriting previous agent's change (last-writer-wins)"
            );
            if let Err(e) = copy_or_link(src, dst) {
                tracing::error!(file = %rel, error = %e, "failed to copy file during merge");
            }
            ConflictResolution::LastWriterWins
        }
    }
}

/// Maximum bytes read per file version when constructing the AI merge prompt.
///
/// This keeps the prompt within Claude's context limits even for large files.
/// Content exceeding this limit is truncated with a note to the model.
const AI_MERGE_MAX_BYTES_PER_VERSION: usize = 50 * 1024; // 50 KB

/// Call Claude to resolve a three-way merge conflict.
///
/// Reads the base, ours (current content of `dst`), and theirs (`src`) versions,
/// constructs a structured prompt, and asks the Anthropic API for the merged
/// result. On success, writes the resolved content to `dst` and returns
/// `ConflictResolution::AiResolved`. On any failure, returns `Err` so the
/// caller can fall back to `ThreeWay`.
#[allow(clippy::too_many_arguments)]
fn resolve_with_ai(
    provider: &AnthropicProvider,
    rel: &str,
    base_dir: &Path,
    src: &Path, // theirs
    dst: &Path, // ours (already written to dest)
    agent_name: &str,
    prev_agent: &str,
) -> GroveResult<ConflictResolution> {
    let read_truncated = |path: &Path| -> String {
        match std::fs::read(path) {
            Ok(bytes) => {
                let truncated = bytes.len() > AI_MERGE_MAX_BYTES_PER_VERSION;
                let slice = &bytes[..bytes.len().min(AI_MERGE_MAX_BYTES_PER_VERSION)];
                let mut content = String::from_utf8_lossy(slice).into_owned();
                if truncated {
                    content.push_str("\n[... content truncated for context limit ...]");
                }
                content
            }
            Err(_) => String::new(),
        }
    };

    let base_file = base_dir.join(rel);
    let base_content = if base_file.exists() {
        read_truncated(&base_file)
    } else {
        String::new() // Both agents created this file — no common ancestor.
    };
    let ours_content = read_truncated(dst);
    let theirs_content = read_truncated(src);

    let instructions = format!(
        "You are resolving a three-way merge conflict in `{rel}` inside a multi-agent \
         coding pipeline.\n\n\
         Agent `{prev_agent}` (ours) and agent `{agent_name}` (theirs) both modified \
         the same file. Your task: produce a single merged version that preserves the \
         intent of both changes where possible.\n\n\
         Rules:\n\
         - Output ONLY the merged file content, with no preamble, no explanation, and \
           no markdown fences.\n\
         - Do NOT include any conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`).\n\
         - If the changes are genuinely incompatible, prefer the version from `{agent_name}` \
           (theirs), but note the decision with a brief inline comment if the language allows.\n\n\
         === BASE (common ancestor) ===\n\
         {base_content}\n\n\
         === OURS ({prev_agent}) ===\n\
         {ours_content}\n\n\
         === THEIRS ({agent_name}) ===\n\
         {theirs_content}\n\n\
         Output the merged content now:"
    );

    let request = ProviderRequest {
        objective: format!("Resolve merge conflict in `{rel}`"),
        role: "merge_resolver".to_string(),
        worktree_path: String::new(), // Not applicable for non-agentic API calls.
        instructions,
        model: Some("claude-haiku-4-5-20251001".to_string()), // Fast, cheap; sufficient for merges.
        allowed_tools: None,
        timeout_override: Some(120),
        provider_session_id: None,
        log_dir: None,
        grove_session_id: None,
        input_handle_callback: None,
        mcp_config_path: None,
        conversation_id: None,
    };

    let response = provider
        .execute(&request)
        .map_err(|e| GroveError::Runtime(format!("AI merge resolution failed for `{rel}`: {e}")))?;

    let merged_content = response.summary;
    if merged_content.is_empty() {
        return Err(GroveError::Runtime(format!(
            "AI merge returned empty content for `{rel}`"
        )));
    }

    std::fs::write(dst, merged_content.as_bytes())
        .map_err(|e| GroveError::Runtime(format!("failed to write AI-merged `{rel}`: {e}")))?;

    tracing::info!(
        file = %rel,
        agents = %format!("{prev_agent}, {agent_name}"),
        "AI merge: conflict resolved by Claude"
    );

    Ok(ConflictResolution::AiResolved)
}

/// Perform a 3-way line-level merge via `git merge-file`.
///
/// - `base_dir`: the common ancestor directory
/// - `rel`: relative path of the file
/// - `src` (theirs): current agent's version
/// - `dst` (ours): previous agent's version, already written to dest
/// - `tmp_dir`: for creating empty base files when the file is new
fn resolve_three_way(
    base_dir: &Path,
    rel: &str,
    src: &Path,
    dst: &Path,
    agent_name: &str,
    prev_agent: &str,
    tmp_dir: &Path,
) -> ConflictResolution {
    use super::git_ops;

    let base_file = base_dir.join(rel);

    // If the file doesn't exist in base (both agents created it), use an empty file.
    let effective_base = if base_file.exists() {
        base_file
    } else {
        let empty = tmp_dir.join("empty_base");
        if !empty.exists() {
            if let Err(e) = std::fs::write(&empty, b"") {
                tracing::error!(error = %e, "failed to create empty base for 3-way merge");
                // Fall back to LWW.
                if let Err(e) = std::fs::copy(src, dst) {
                    tracing::error!(file = %rel, error = %e, "failed to copy file during merge fallback");
                }
                return ConflictResolution::LastWriterWins;
            }
        }
        empty
    };

    // git merge-file expects: ours, base, theirs
    // ours = dst (previous agent's version, currently in dest)
    // base = effective_base (common ancestor)
    // theirs = src (current agent's version)
    match git_ops::git_merge_file(dst, &effective_base, src) {
        Ok(git_ops::MergeFileResult::Clean(content)) => {
            if let Err(e) = std::fs::write(dst, &content) {
                tracing::error!(file = %rel, error = %e, "failed to write 3-way merged content");
                return ConflictResolution::LastWriterWins;
            }
            tracing::info!(
                file = %rel, agents = %format!("{prev_agent}, {agent_name}"),
                "3-way merge: clean auto-merge"
            );
            ConflictResolution::ThreeWayClean
        }
        Ok(git_ops::MergeFileResult::Conflict {
            merged_with_markers,
            conflict_count,
        }) => {
            if let Err(e) = std::fs::write(dst, &merged_with_markers) {
                tracing::error!(file = %rel, error = %e, "failed to write conflict markers");
                return ConflictResolution::LastWriterWins;
            }
            tracing::warn!(
                file = %rel, agents = %format!("{prev_agent}, {agent_name}"),
                conflicts = conflict_count,
                "3-way merge: conflict markers written"
            );
            ConflictResolution::ThreeWayWithMarkers {
                marker_count: conflict_count,
            }
        }
        Err(e) => {
            tracing::error!(
                file = %rel, error = %e,
                "git merge-file failed — falling back to last-writer-wins"
            );
            if let Err(copy_err) = std::fs::copy(src, dst) {
                tracing::error!(file = %rel, error = %copy_err, "LWW fallback copy also failed");
            }
            ConflictResolution::LastWriterWins
        }
    }
}

// ── File/symlink copy ────────────────────────────────────────────────────────

/// Copy a file or recreate a symlink from `src` to `dst`.
///
/// If `src` is a symlink, reads its target and recreates it at `dst` (without
/// following). Otherwise, performs a regular file copy.
fn copy_or_link(src: &Path, dst: &Path) -> GroveResult<()> {
    let meta = src
        .symlink_metadata()
        .map_err(|e| GroveError::Runtime(format!("stat {}: {e}", src.display())))?;

    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(src)
            .map_err(|e| GroveError::Runtime(format!("readlink {}: {e}", src.display())))?;
        // Remove any existing entry at destination.
        let _ = std::fs::remove_file(dst);
        super::recreate_symlink(&target, dst)
    } else {
        std::fs::copy(src, dst)
            .map_err(|e| GroveError::Runtime(format!("copy {}: {e}", src.display())))?;
        Ok(())
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Recursively collect `(relative_path → content_hash)` for all non-protected files.
///
/// Uses `DefaultHasher` (SipHash) for speed — this is equality detection for file
/// change tracking, NOT a security primitive. Collision probability is negligible
/// for this use case. Do NOT replace with a cryptographic hash.
///
/// Files are streamed through a 64 KB buffer to avoid loading large files entirely
/// into memory.
fn collect_file_hashes(root: &Path, dir: &Path, filter: &GitignoreFilter) -> HashMap<String, u64> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    use std::io::Read;

    use crate::worktree::gitignore::is_grove_internal_file;

    let mut map = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return map;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let n = name.to_string_lossy();
        if n == ".git" || n == ".grove" {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path);
        if is_grove_internal_file(&n) || filter.is_ignored(rel, ft.is_dir()) {
            continue;
        }
        if ft.is_symlink() {
            // Hash the link target path, not the content it points to.
            // This matches git's behavior: symlinks are tracked by target string.
            let target = std::fs::read_link(&path).unwrap_or_default();
            let mut hasher = DefaultHasher::new();
            hasher.write(target.to_string_lossy().as_bytes());
            let hash = hasher.finish();
            if let Ok(rel) = path.strip_prefix(root) {
                map.insert(rel.to_string_lossy().into_owned(), hash);
            }
        } else if ft.is_file() {
            let hash = if let Ok(file) = std::fs::File::open(&path) {
                let mut reader = std::io::BufReader::with_capacity(64 * 1024, file);
                let mut hasher = DefaultHasher::new();
                let mut buf = [0u8; 64 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => hasher.write(&buf[..n]),
                        Err(_) => break,
                    }
                }
                hasher.finish()
            } else {
                0
            };
            if let Ok(rel) = path.strip_prefix(root) {
                map.insert(rel.to_string_lossy().into_owned(), hash);
            }
        } else if ft.is_dir() {
            map.extend(collect_file_hashes(root, &path, filter));
        }
    }
    map
}
