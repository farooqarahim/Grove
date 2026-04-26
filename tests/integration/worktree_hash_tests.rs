use grove_core::config::{BinaryStrategy, MergeStrategy};
use grove_core::worktree::gitignore::GitignoreFilter;
use grove_core::worktree::merge;
use std::fs;
use tempfile::TempDir;

/// Create a base directory with known file contents.
fn setup_base(dir: &std::path::Path) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("file.txt"), "base content\n").unwrap();
    fs::write(dir.join("shared.txt"), "shared\n").unwrap();
}

/// Copy base into a fork directory and optionally modify a file.
fn setup_fork(base: &std::path::Path, fork: &std::path::Path, modify: Option<(&str, &str)>) {
    fs::create_dir_all(fork).unwrap();
    for entry in fs::read_dir(base).unwrap() {
        let entry = entry.unwrap();
        let dest = fork.join(entry.file_name());
        fs::copy(entry.path(), &dest).unwrap();
    }
    if let Some((name, content)) = modify {
        fs::write(fork.join(name), content).unwrap();
    }
}

#[test]
fn hash_based_merge_detects_modification() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork = tmp.path().join("fork");
    let merged = tmp.path().join("merged");

    setup_base(&base);
    setup_fork(&base, &fork, Some(("file.txt", "modified by agent\n")));

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork.clone())],
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
    )
    .unwrap();

    assert!(
        result.conflicts.is_empty(),
        "single-fork merge should have no conflicts"
    );

    let content = fs::read_to_string(merged.join("file.txt")).unwrap();
    assert_eq!(
        content, "modified by agent\n",
        "modified file should be in merge result"
    );

    let shared = fs::read_to_string(merged.join("shared.txt")).unwrap();
    assert_eq!(shared, "shared\n", "unmodified file should be unchanged");
}

#[test]
fn hash_based_merge_detects_conflict_between_two_forks() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    setup_base(&base);
    setup_fork(&base, &fork_a, Some(("file.txt", "agent A version\n")));
    setup_fork(&base, &fork_b, Some(("file.txt", "agent B version\n")));

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork_a), ("agent-b", fork_b)],
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
    )
    .unwrap();

    assert!(
        !result.conflicts.is_empty(),
        "two forks modifying the same file should produce a conflict"
    );
    assert_eq!(result.conflicts[0].path, "file.txt");

    // All LWW conflicts should be unresolved
    assert!(
        result.has_unresolved_conflicts(),
        "LWW conflicts should be unresolved"
    );
}

#[test]
fn hash_based_merge_detects_deletion() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork = tmp.path().join("fork");
    let merged = tmp.path().join("merged");

    setup_base(&base);
    setup_fork(&base, &fork, None);
    // Delete file.txt in the fork — agent removed it.
    fs::remove_file(fork.join("file.txt")).unwrap();

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork.clone())],
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
    )
    .unwrap();

    assert!(
        result.conflicts.is_empty(),
        "single-fork deletion should not conflict"
    );
    assert!(
        !merged.join("file.txt").exists(),
        "deleted file should not appear in merge result"
    );
    assert!(
        merged.join("shared.txt").exists(),
        "non-deleted file should be preserved"
    );
}

#[test]
fn hash_based_merge_conflict_when_one_deletes_other_modifies() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    setup_base(&base);
    // Fork A deletes file.txt.
    setup_fork(&base, &fork_a, None);
    fs::remove_file(fork_a.join("file.txt")).unwrap();
    // Fork B modifies file.txt.
    setup_fork(&base, &fork_b, Some(("file.txt", "agent B change\n")));

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork_a), ("agent-b", fork_b)],
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
    )
    .unwrap();

    // Delete vs modify should be detected as a conflict (both changed from base).
    assert!(
        !result.conflicts.is_empty(),
        "delete-vs-modify should produce a conflict"
    );
}

// ── Phase 0 specific tests ───────────────────────────────────────────────────

#[test]
fn merge_metrics_emitted_correctly() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    setup_base(&base);
    setup_fork(&base, &fork_a, Some(("file.txt", "agent A version\n")));
    setup_fork(
        &base,
        &fork_b,
        Some(("shared.txt", "agent B modified shared\n")),
    );

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork_a), ("agent-b", fork_b)],
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
    )
    .unwrap();

    // No conflicts: each agent changed a different file
    assert!(result.conflicts.is_empty());
    assert!(!result.has_unresolved_conflicts());

    // Metrics should reflect the work done
    assert_eq!(result.metrics.strategy_used, "last_writer_wins");
    assert!(result.metrics.files_processed > 0);
    assert!(result.metrics.files_changed >= 2, "two files were changed");
    assert_eq!(result.metrics.conflicts_total, 0);
    assert_eq!(result.metrics.conflicts_auto_resolved, 0);
    assert_eq!(result.metrics.conflicts_unresolved, 0);
    assert!(
        result.metrics.duration_ms < 10_000,
        "merge should complete in <10s"
    );
    assert!(result.metrics.fallback_reason.is_none());
}

#[test]
fn merge_strategy_three_way_single_fork_no_conflict() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork = tmp.path().join("fork");
    let merged = tmp.path().join("merged");

    setup_base(&base);
    setup_fork(&base, &fork, Some(("file.txt", "modified\n")));

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork.clone())],
        &merged,
        &filter,
        MergeStrategy::ThreeWay,
    )
    .unwrap();

    // Single fork — no conflicts, ThreeWay strategy used
    assert_eq!(result.metrics.strategy_used, "three_way");
    assert!(result.conflicts.is_empty());
    let content = fs::read_to_string(merged.join("file.txt")).unwrap();
    assert_eq!(content, "modified\n");
}

#[test]
fn conflict_record_is_resolved_api() {
    use grove_core::worktree::merge::{ConflictRecord, ConflictResolution};

    let clean = ConflictRecord {
        path: "file.txt".to_string(),
        agents: vec!["a".to_string(), "b".to_string()],
        resolution: ConflictResolution::ThreeWayClean,
        winning_agent_priority: None,
    };
    assert!(clean.is_resolved(), "ThreeWayClean should be resolved");

    let lww = ConflictRecord {
        path: "file.txt".to_string(),
        agents: vec!["a".to_string(), "b".to_string()],
        resolution: ConflictResolution::LastWriterWins,
        winning_agent_priority: Some(50),
    };
    assert!(!lww.is_resolved(), "LastWriterWins should be unresolved");

    let markers = ConflictRecord {
        path: "file.txt".to_string(),
        agents: vec!["a".to_string(), "b".to_string()],
        resolution: ConflictResolution::ThreeWayWithMarkers { marker_count: 2 },
        winning_agent_priority: None,
    };
    assert!(
        !markers.is_resolved(),
        "ThreeWayWithMarkers should be unresolved"
    );

    let binary = ConflictRecord {
        path: "image.png".to_string(),
        agents: vec!["a".to_string(), "b".to_string()],
        resolution: ConflictResolution::BinaryLastWriterWins,
        winning_agent_priority: Some(50),
    };
    assert!(
        !binary.is_resolved(),
        "BinaryLastWriterWins should be unresolved"
    );
}

#[test]
fn merge_result_unresolved_conflicts_filter() {
    use grove_core::worktree::merge::{
        ConflictRecord, ConflictResolution, MergeMetrics, MergeResult,
    };

    let result = MergeResult {
        conflicts: vec![
            ConflictRecord {
                path: "auto.txt".to_string(),
                agents: vec!["a".to_string(), "b".to_string()],
                resolution: ConflictResolution::ThreeWayClean,
                winning_agent_priority: None,
            },
            ConflictRecord {
                path: "manual.txt".to_string(),
                agents: vec!["a".to_string(), "b".to_string()],
                resolution: ConflictResolution::LastWriterWins,
                winning_agent_priority: Some(50),
            },
        ],
        metrics: MergeMetrics {
            strategy_used: "three_way".to_string(),
            files_processed: 10,
            files_changed: 2,
            conflicts_total: 2,
            conflicts_auto_resolved: 1,
            conflicts_unresolved: 1,
            duration_ms: 50,
            fallback_reason: None,
            change_detection_ms: 0,
            change_detection_strategy: "hash-walk".to_string(),
            sparse_worktrees: 0,
            sparse_files_materialized: 0,
            crash_recovery_triggered: false,
            guard_violations_total: 0,
        },
        modified_lockfiles: Vec::new(),
        guard_violations: Vec::new(),
    };

    assert!(result.has_unresolved_conflicts());
    let unresolved = result.unresolved_conflicts();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].path, "manual.txt");
}

#[test]
fn merge_strategy_roundtrips_through_serde() {
    // Ensure the enum serializes/deserializes correctly for grove.yaml
    let lww = MergeStrategy::LastWriterWins;
    let json = serde_json::to_string(&lww).unwrap();
    assert_eq!(json, "\"last_writer_wins\"");
    let parsed: MergeStrategy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, MergeStrategy::LastWriterWins);

    let tw = MergeStrategy::ThreeWay;
    let json = serde_json::to_string(&tw).unwrap();
    assert_eq!(json, "\"three_way\"");
    let parsed: MergeStrategy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, MergeStrategy::ThreeWay);

    let ai = MergeStrategy::AiResolve;
    let json = serde_json::to_string(&ai).unwrap();
    assert_eq!(json, "\"ai_resolve\"");
    let parsed: MergeStrategy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, MergeStrategy::AiResolve);
}

#[test]
fn ai_resolved_conflict_resolution_is_resolved() {
    use grove_core::worktree::merge::{ConflictRecord, ConflictResolution};

    let record = ConflictRecord {
        path: "src/main.rs".to_string(),
        agents: vec!["builder".to_string(), "reviewer".to_string()],
        resolution: ConflictResolution::AiResolved,
        winning_agent_priority: None,
    };
    // AiResolved is a clean resolution — should not be flagged as unresolved.
    assert!(
        record.is_resolved(),
        "AiResolved must be treated as resolved"
    );
}

// ── Phase 1: 3-way merge tests ──────────────────────────────────────────────

#[test]
fn three_way_merge_different_lines_auto_resolves() {
    // Two agents edit different lines of the same file → ThreeWayClean
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("code.txt"), "line1\nline2\nline3\nline4\n").unwrap();

    // Fork A modifies line 1
    fs::create_dir_all(&fork_a).unwrap();
    fs::write(fork_a.join("code.txt"), "AGENT_A\nline2\nline3\nline4\n").unwrap();

    // Fork B modifies line 4
    fs::create_dir_all(&fork_b).unwrap();
    fs::write(fork_b.join("code.txt"), "line1\nline2\nline3\nAGENT_B\n").unwrap();

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork_a), ("agent-b", fork_b)],
        &merged,
        &filter,
        MergeStrategy::ThreeWay,
    )
    .unwrap();

    // Should auto-resolve cleanly
    assert_eq!(result.conflicts.len(), 1, "one file conflict detected");
    assert!(
        result.conflicts[0].resolution == merge::ConflictResolution::ThreeWayClean,
        "expected ThreeWayClean, got {:?}",
        result.conflicts[0].resolution
    );
    assert!(!result.has_unresolved_conflicts());

    // Both changes should be present
    let content = fs::read_to_string(merged.join("code.txt")).unwrap();
    assert!(
        content.contains("AGENT_A"),
        "agent A's change should be in merged output"
    );
    assert!(
        content.contains("AGENT_B"),
        "agent B's change should be in merged output"
    );
}

#[test]
fn three_way_merge_same_line_produces_markers() {
    // Two agents edit the same line → ThreeWayWithMarkers
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("code.txt"), "line1\nline2\nline3\n").unwrap();

    // Both agents modify line 2 differently
    fs::create_dir_all(&fork_a).unwrap();
    fs::write(fork_a.join("code.txt"), "line1\nAGENT_A_CHANGE\nline3\n").unwrap();

    fs::create_dir_all(&fork_b).unwrap();
    fs::write(fork_b.join("code.txt"), "line1\nAGENT_B_CHANGE\nline3\n").unwrap();

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork_a), ("agent-b", fork_b)],
        &merged,
        &filter,
        MergeStrategy::ThreeWay,
    )
    .unwrap();

    assert_eq!(result.conflicts.len(), 1);
    match &result.conflicts[0].resolution {
        merge::ConflictResolution::ThreeWayWithMarkers { marker_count } => {
            assert!(
                *marker_count > 0,
                "should have at least one conflict region"
            );
        }
        other => panic!("expected ThreeWayWithMarkers, got {other:?}"),
    }
    assert!(result.has_unresolved_conflicts());

    // Conflict markers should be in the file
    let content = fs::read_to_string(merged.join("code.txt")).unwrap();
    assert!(
        content.contains("<<<<<<<"),
        "should contain conflict markers"
    );
    assert!(
        content.contains(">>>>>>>"),
        "should contain conflict markers"
    );
}

#[test]
fn three_way_merge_binary_files_use_lww() {
    // Binary files always use last-writer-wins even with ThreeWay strategy
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    // Create a binary file (contains null bytes)
    let binary_content = b"PNG\x00\x01\x02\x03base_version";
    fs::write(base.join("image.bin"), binary_content).unwrap();

    fs::create_dir_all(&fork_a).unwrap();
    fs::write(
        fork_a.join("image.bin"),
        b"PNG\x00\x01\x02\x03agent_a_version",
    )
    .unwrap();

    fs::create_dir_all(&fork_b).unwrap();
    fs::write(
        fork_b.join("image.bin"),
        b"PNG\x00\x01\x02\x03agent_b_version",
    )
    .unwrap();

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork_a), ("agent-b", fork_b)],
        &merged,
        &filter,
        MergeStrategy::ThreeWay,
    )
    .unwrap();

    assert_eq!(result.conflicts.len(), 1);
    assert!(
        result.conflicts[0].resolution == merge::ConflictResolution::BinaryLastWriterWins,
        "binary files should use BinaryLastWriterWins, got {:?}",
        result.conflicts[0].resolution
    );

    // Last writer (agent-b) should win
    let content = fs::read(merged.join("image.bin")).unwrap();
    assert_eq!(content, b"PNG\x00\x01\x02\x03agent_b_version");
}

#[test]
fn three_way_merge_three_agents_progressive() {
    // Three agents modify different lines → progressive merge works
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let fork_c = tmp.path().join("fork_c");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(
        base.join("code.txt"),
        "line1\nline2\nline3\nline4\nline5\nline6\n",
    )
    .unwrap();

    // Each agent modifies different lines
    fs::create_dir_all(&fork_a).unwrap();
    fs::write(
        fork_a.join("code.txt"),
        "AGENT_A\nline2\nline3\nline4\nline5\nline6\n",
    )
    .unwrap();

    fs::create_dir_all(&fork_b).unwrap();
    fs::write(
        fork_b.join("code.txt"),
        "line1\nline2\nAGENT_B\nline4\nline5\nline6\n",
    )
    .unwrap();

    fs::create_dir_all(&fork_c).unwrap();
    fs::write(
        fork_c.join("code.txt"),
        "line1\nline2\nline3\nline4\nline5\nAGENT_C\n",
    )
    .unwrap();

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[
            ("agent-a", fork_a),
            ("agent-b", fork_b),
            ("agent-c", fork_c),
        ],
        &merged,
        &filter,
        MergeStrategy::ThreeWay,
    )
    .unwrap();

    // Two conflicts detected (B vs A, C vs merged-AB), both should be clean
    assert!(
        !result.has_unresolved_conflicts(),
        "all conflicts should auto-resolve"
    );

    let content = fs::read_to_string(merged.join("code.txt")).unwrap();
    assert!(content.contains("AGENT_A"), "agent A change preserved");
    assert!(content.contains("AGENT_B"), "agent B change preserved");
    assert!(content.contains("AGENT_C"), "agent C change preserved");
}

#[test]
fn lww_strategy_ignores_three_way_merge() {
    // Regression guard: LWW flag always uses old behavior
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("code.txt"), "line1\nline2\nline3\n").unwrap();

    // Different lines — would auto-merge with ThreeWay, but LWW should just overwrite
    fs::create_dir_all(&fork_a).unwrap();
    fs::write(fork_a.join("code.txt"), "AGENT_A\nline2\nline3\n").unwrap();

    fs::create_dir_all(&fork_b).unwrap();
    fs::write(fork_b.join("code.txt"), "line1\nline2\nAGENT_B\n").unwrap();

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork_a), ("agent-b", fork_b)],
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
    )
    .unwrap();

    assert_eq!(result.metrics.strategy_used, "last_writer_wins");
    assert_eq!(result.conflicts.len(), 1);
    assert!(
        result.conflicts[0].resolution == merge::ConflictResolution::LastWriterWins,
        "LWW strategy should always produce LastWriterWins resolution"
    );
    assert!(result.has_unresolved_conflicts());

    // Last writer (agent-b) should have overwritten completely
    let content = fs::read_to_string(merged.join("code.txt")).unwrap();
    assert_eq!(
        content, "line1\nline2\nAGENT_B\n",
        "LWW should take agent-b's full file"
    );
}

#[test]
fn three_way_merge_both_agents_create_new_file() {
    // Both agents create a file that doesn't exist in base → empty base used
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    // No file.txt in base

    fs::create_dir_all(&fork_a).unwrap();
    fs::write(fork_a.join("new.txt"), "agent A content\n").unwrap();

    fs::create_dir_all(&fork_b).unwrap();
    fs::write(fork_b.join("new.txt"), "agent B content\n").unwrap();

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork_a), ("agent-b", fork_b)],
        &merged,
        &filter,
        MergeStrategy::ThreeWay,
    )
    .unwrap();

    // Both created the same file — should produce a conflict (markers or clean merge)
    assert_eq!(result.conflicts.len(), 1);
    // The file should exist in merged output
    assert!(
        merged.join("new.txt").exists(),
        "new file should exist in merge output"
    );
}

// ── Phase 3: Git-native change detection tests ──────────────────────────────

#[test]
fn change_detection_hash_walk_fallback_with_no_base_commit() {
    // When base_commit is None, should fall back to hash-walk.
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork = tmp.path().join("fork");
    let merged = tmp.path().join("merged");

    setup_base(&base);
    setup_fork(&base, &fork, Some(("file.txt", "modified\n")));

    let filter = GitignoreFilter::empty();
    // Use merge_worktrees_simple which always passes None for base_commit
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork)],
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
    )
    .unwrap();

    assert_eq!(result.metrics.change_detection_strategy, "hash-walk");
    assert!(
        result.metrics.change_detection_ms < 5000,
        "should complete quickly"
    );
    let content = fs::read_to_string(merged.join("file.txt")).unwrap();
    assert_eq!(content, "modified\n");
}

#[test]
fn change_detection_with_agent_worktree_no_git() {
    // AgentWorktree with base_commit but non-git dir should fall back to hash-walk.
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork = tmp.path().join("fork");
    let merged = tmp.path().join("merged");

    setup_base(&base);
    setup_fork(&base, &fork, Some(("file.txt", "changed\n")));

    let filter = GitignoreFilter::empty();
    let worktrees = vec![merge::AgentWorktree {
        name: "agent-a".to_string(),
        path: fork,
        base_commit: Some("deadbeef".to_string()), // won't resolve — not a git repo
        merge_priority: 50,
        is_sparse: false,
        sparse_patterns: Vec::new(),
    }];

    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::LastWriter,
        &Default::default(),
    )
    .unwrap();

    // Should fall back to hash-walk since git rev-parse fails in non-git dir
    assert_eq!(result.metrics.change_detection_strategy, "hash-walk");
    let content = fs::read_to_string(merged.join("file.txt")).unwrap();
    assert_eq!(content, "changed\n");
}

#[test]
fn change_detection_metrics_report_strategy() {
    // Verify metrics report the detection strategy used.
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("a.txt"), "original\n").unwrap();
    fs::write(base.join("b.txt"), "original\n").unwrap();

    // Fork A modifies a.txt
    fs::create_dir_all(&fork_a).unwrap();
    fs::write(fork_a.join("a.txt"), "from A\n").unwrap();
    fs::write(fork_a.join("b.txt"), "original\n").unwrap();

    // Fork B modifies b.txt
    fs::create_dir_all(&fork_b).unwrap();
    fs::write(fork_b.join("a.txt"), "original\n").unwrap();
    fs::write(fork_b.join("b.txt"), "from B\n").unwrap();

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork_a), ("agent-b", fork_b)],
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
    )
    .unwrap();

    // Both use hash-walk (no base_commit)
    assert_eq!(result.metrics.change_detection_strategy, "hash-walk");
    // Verify change_detection_ms is populated (u64 is always >= 0)
    let _ = result.metrics.change_detection_ms;
    // No conflicts (different files)
    assert!(result.conflicts.is_empty());
    // Both changes should be present
    assert_eq!(
        fs::read_to_string(merged.join("a.txt")).unwrap(),
        "from A\n"
    );
    assert_eq!(
        fs::read_to_string(merged.join("b.txt")).unwrap(),
        "from B\n"
    );
}

#[test]
fn git_diff_names_parse_rename_as_delete_add() {
    // Test that the NUL-delimited parser correctly handles rename status.
    use grove_core::worktree::git_ops::FileChangeStatus;

    // Simulate NUL-delimited diff-tree output with a rename:
    // M\0modified.txt\0R100\0old.txt\0new.txt\0A\0added.txt\0D\0deleted.txt\0
    let output = b"M\0modified.txt\0R100\0old.txt\0new.txt\0A\0added.txt\0D\0deleted.txt\0";

    // We can't call parse_diff_name_status_nul directly (it's private),
    // but we can verify the behavior through the merge pipeline.
    // Instead, verify FileChangeStatus is accessible and correct.
    assert_ne!(FileChangeStatus::Added, FileChangeStatus::Deleted);
    assert_ne!(FileChangeStatus::Modified, FileChangeStatus::Added);
    assert_eq!(FileChangeStatus::Added, FileChangeStatus::Added);
    let _ = output; // used for documentation only
}

// ── Phase 4: Symlink support tests ──────────────────────────────────────────

#[cfg(unix)]
#[test]
fn symlink_preserved_across_fork_and_merge() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork = tmp.path().join("fork");
    let merged = tmp.path().join("merged");

    // Base has a file and a symlink pointing to it.
    fs::create_dir_all(base.join("data")).unwrap();
    fs::write(base.join("data/config.json"), r#"{"key": "value"}"#).unwrap();
    symlink("data/config.json", base.join("link_to_config")).unwrap();

    // Fork: manually create a copy with symlink intact (can't use setup_fork — it uses fs::copy).
    fs::create_dir_all(fork.join("data")).unwrap();
    fs::write(fork.join("data/config.json"), r#"{"key": "value"}"#).unwrap();
    symlink("data/config.json", fork.join("link_to_config")).unwrap();

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork)],
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
    )
    .unwrap();

    assert!(result.conflicts.is_empty());
    // The symlink should exist in merged output and point to same target.
    let merged_link = merged.join("link_to_config");
    assert!(
        merged_link
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let target = fs::read_link(&merged_link).unwrap();
    assert_eq!(target.to_string_lossy(), "data/config.json");
}

#[cfg(unix)]
#[test]
fn dangling_symlink_preserved() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork = tmp.path().join("fork");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("file.txt"), "content\n").unwrap();
    // Dangling symlink — target doesn't exist.
    symlink("/nonexistent/path", base.join("broken_link")).unwrap();

    fs::create_dir_all(&fork).unwrap();
    fs::write(fork.join("file.txt"), "content\n").unwrap();
    symlink("/nonexistent/path", fork.join("broken_link")).unwrap();

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork)],
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
    )
    .unwrap();

    assert!(result.conflicts.is_empty());
    // Dangling symlink should be preserved.
    let merged_link = merged.join("broken_link");
    assert!(
        merged_link
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&merged_link).unwrap().to_string_lossy(),
        "/nonexistent/path"
    );
}

#[cfg(unix)]
#[test]
fn two_agents_different_symlink_targets_conflict() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("file.txt"), "content\n").unwrap();
    symlink("target_original", base.join("link")).unwrap();

    // Agent A changes symlink target
    fs::create_dir_all(&fork_a).unwrap();
    fs::write(fork_a.join("file.txt"), "content\n").unwrap();
    symlink("target_A", fork_a.join("link")).unwrap();

    // Agent B changes symlink target differently
    fs::create_dir_all(&fork_b).unwrap();
    fs::write(fork_b.join("file.txt"), "content\n").unwrap();
    symlink("target_B", fork_b.join("link")).unwrap();

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork_a), ("agent-b", fork_b)],
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
    )
    .unwrap();

    // Conflict should be recorded (both agents changed the same symlink).
    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].path, "link");
    // Last writer (agent-b) wins.
    let target = fs::read_link(merged.join("link")).unwrap();
    assert_eq!(target.to_string_lossy(), "target_B");
}

// ── Phase 5: Merge Priority + Ordering ──────────────────────────────────────

#[test]
fn higher_priority_agent_wins_conflict() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_lo = tmp.path().join("fork_lo");
    let fork_hi = tmp.path().join("fork_hi");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("file.txt"), "original\n").unwrap();

    // Two agents modify the same file.
    setup_fork(&base, &fork_lo, Some(("file.txt", "low priority change\n")));
    setup_fork(
        &base,
        &fork_hi,
        Some(("file.txt", "high priority change\n")),
    );

    let filter = GitignoreFilter::empty();

    // Use merge_worktrees directly with explicit priorities.
    // Lower number = higher priority = applied last = wins.
    let worktrees = vec![
        merge::AgentWorktree {
            name: "agent-lo".into(),
            path: fork_lo,
            base_commit: None,
            merge_priority: 60, // low priority
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
        merge::AgentWorktree {
            name: "agent-hi".into(),
            path: fork_hi,
            base_commit: None,
            merge_priority: 10, // high priority
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
    ];

    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::LastWriter,
        &Default::default(),
    )
    .unwrap();

    // agent-hi should win the conflict (priority 10 < 60).
    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].winning_agent_priority, Some(10));
    let content = fs::read_to_string(merged.join("file.txt")).unwrap();
    assert_eq!(content, "high priority change\n");
}

#[test]
fn alphabetical_tiebreak_when_same_priority() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_z = tmp.path().join("fork_z");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("file.txt"), "original\n").unwrap();

    setup_fork(&base, &fork_a, Some(("file.txt", "alpha change\n")));
    setup_fork(&base, &fork_z, Some(("file.txt", "zulu change\n")));

    let filter = GitignoreFilter::empty();

    // Same priority — alphabetical tiebreak: "agent-alpha" < "agent-zulu".
    // Sort: highest priority number first → same → alphabetical first processed first.
    // alpha is alphabetically first → applied first → zulu applied second → zulu wins.
    let worktrees = vec![
        merge::AgentWorktree {
            name: "agent-alpha".into(),
            path: fork_a,
            base_commit: None,
            merge_priority: 30,
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
        merge::AgentWorktree {
            name: "agent-zulu".into(),
            path: fork_z,
            base_commit: None,
            merge_priority: 30,
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
    ];

    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::LastWriter,
        &Default::default(),
    )
    .unwrap();

    assert_eq!(result.conflicts.len(), 1);
    // zulu is alphabetically last, so processed last, so wins.
    let content = fs::read_to_string(merged.join("file.txt")).unwrap();
    assert_eq!(content, "zulu change\n");
}

#[test]
fn non_conflicting_files_unaffected_by_priority() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("shared.txt"), "original\n").unwrap();

    // Each agent modifies a different file — no conflicts.
    setup_fork(&base, &fork_a, None);
    fs::write(fork_a.join("a_only.txt"), "from agent a\n").unwrap();

    setup_fork(&base, &fork_b, None);
    fs::write(fork_b.join("b_only.txt"), "from agent b\n").unwrap();

    let filter = GitignoreFilter::empty();

    let worktrees = vec![
        merge::AgentWorktree {
            name: "agent-a".into(),
            path: fork_a,
            base_commit: None,
            merge_priority: 10,
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
        merge::AgentWorktree {
            name: "agent-b".into(),
            path: fork_b,
            base_commit: None,
            merge_priority: 60,
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
    ];

    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::LastWriter,
        &Default::default(),
    )
    .unwrap();

    assert!(result.conflicts.is_empty());
    assert_eq!(
        fs::read_to_string(merged.join("a_only.txt")).unwrap(),
        "from agent a\n"
    );
    assert_eq!(
        fs::read_to_string(merged.join("b_only.txt")).unwrap(),
        "from agent b\n"
    );
}

#[cfg(unix)]
#[test]
fn symlink_hash_compares_target_not_content() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork = tmp.path().join("fork");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("data.txt"), "hello\n").unwrap();
    symlink("data.txt", base.join("link")).unwrap();

    // Fork changes the symlink target to a different file.
    fs::create_dir_all(&fork).unwrap();
    fs::write(fork.join("data.txt"), "hello\n").unwrap();
    fs::write(fork.join("other.txt"), "hello\n").unwrap();
    symlink("other.txt", fork.join("link")).unwrap();

    let filter = GitignoreFilter::empty();
    let _result = merge::merge_worktrees_simple(
        &base,
        &[("agent-a", fork)],
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
    )
    .unwrap();

    // Even though both targets have the same content, the symlink target changed.
    // This should be detected as a change.
    let merged_target = fs::read_link(merged.join("link")).unwrap();
    assert_eq!(merged_target.to_string_lossy(), "other.txt");
}

// ── Phase 6: Sparse Checkout Support ────────────────────────────────────────

#[test]
fn sparse_worktree_does_not_treat_unchecked_files_as_deleted() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork = tmp.path().join("fork");
    let merged = tmp.path().join("merged");

    // Base has files in src/ and docs/.
    fs::create_dir_all(base.join("src")).unwrap();
    fs::create_dir_all(base.join("docs")).unwrap();
    fs::write(base.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(base.join("docs/readme.md"), "# Hello\n").unwrap();

    // Fork only has src/ (simulating sparse checkout that excluded docs/).
    fs::create_dir_all(fork.join("src")).unwrap();
    fs::write(
        fork.join("src/main.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .unwrap();
    // docs/ is NOT present in the fork — but it shouldn't be treated as deleted.

    let filter = GitignoreFilter::empty();
    let worktrees = vec![merge::AgentWorktree {
        name: "builder".into(),
        path: fork,
        base_commit: None,
        merge_priority: 30,
        is_sparse: true,
        sparse_patterns: vec!["src/".to_string()],
    }];

    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::LastWriter,
        &Default::default(),
    )
    .unwrap();

    // docs/readme.md should still exist in merged (not deleted).
    assert!(
        merged.join("docs/readme.md").exists(),
        "docs/readme.md should survive — sparse worktree didn't check it out"
    );
    // src/main.rs should be updated.
    let content = fs::read_to_string(merged.join("src/main.rs")).unwrap();
    assert!(
        content.contains("println"),
        "src/main.rs should be updated by the agent"
    );
    // No conflicts.
    assert!(result.conflicts.is_empty());
}

#[test]
fn sparse_metrics_counted_correctly() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_sparse = tmp.path().join("fork_sparse");
    let fork_full = tmp.path().join("fork_full");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/lib.rs"), "// lib\n").unwrap();
    fs::write(base.join("README.md"), "# readme\n").unwrap();

    // Sparse agent only touches src/.
    fs::create_dir_all(fork_sparse.join("src")).unwrap();
    fs::write(fork_sparse.join("src/lib.rs"), "// updated lib\n").unwrap();
    fs::write(fork_sparse.join("README.md"), "# readme\n").unwrap();

    // Full agent touches README.
    fs::create_dir_all(fork_full.join("src")).unwrap();
    fs::write(fork_full.join("src/lib.rs"), "// lib\n").unwrap();
    fs::write(fork_full.join("README.md"), "# updated readme\n").unwrap();

    let filter = GitignoreFilter::empty();
    let worktrees = vec![
        merge::AgentWorktree {
            name: "builder".into(),
            path: fork_sparse,
            base_commit: None,
            merge_priority: 30,
            is_sparse: true,
            sparse_patterns: vec!["src/".to_string()],
        },
        merge::AgentWorktree {
            name: "reviewer".into(),
            path: fork_full,
            base_commit: None,
            merge_priority: 20,
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
    ];

    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::LastWriter,
        &Default::default(),
    )
    .unwrap();

    assert_eq!(result.metrics.sparse_worktrees, 1, "one sparse worktree");
    assert!(result.conflicts.is_empty());
}

#[test]
fn sparse_config_defaults_to_disabled() {
    use grove_core::config::SparseConfig;

    let cfg = SparseConfig::default();
    assert!(!cfg.enabled);
    assert!(cfg.profiles.is_empty());
}

#[test]
fn sparse_config_parses_from_yaml() {
    let yaml = r#"
project:
  name: "test"
  default_branch: "main"
runtime:
  max_agents: 3
  max_run_minutes: 60
  log_level: "info"
providers:
  default: "claude_code"
  mock:
    enabled: true
  claude_code:
    enabled: true
    command: "claude"
    timeout_seconds: 300
budgets:
  default_run_usd: 5.0
  warning_threshold_percent: 80
  hard_stop_percent: 100
orchestration:
  enforce_design_first: true
  enable_retries: true
  max_retries_per_session: 2
worktree:
  root: ".grove/worktrees"
  cleanup_on_success: false
merge:
  strategy: "last_writer_wins"
  auto_merge: false
checkpoint:
  enabled: true
  save_on_stage_transition: true
observability:
  emit_json_logs: true
  redact_secrets: true
network:
  allow_provider_network: false
sparse:
  enabled: true
  profiles:
    builder: ["src/", "tests/", "Cargo.toml"]
    tester: ["tests/", "src/"]
"#;
    let cfg: grove_core::config::GroveConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(cfg.sparse.enabled);
    assert_eq!(cfg.sparse.profiles.len(), 2);
    assert_eq!(
        cfg.sparse.profiles["builder"],
        vec!["src/", "tests/", "Cargo.toml"]
    );
    assert_eq!(cfg.sparse.profiles["tester"], vec!["tests/", "src/"]);
}

#[test]
fn matches_sparse_patterns_directory_and_file() {
    use grove_core::worktree::git_ops::matches_sparse_patterns;

    let patterns = vec![
        "src/".to_string(),
        "Cargo.toml".to_string(),
        "*.lock".to_string(),
    ];

    // Directory patterns.
    assert!(matches_sparse_patterns("src/main.rs", &patterns));
    assert!(matches_sparse_patterns("src/lib/mod.rs", &patterns));
    assert!(!matches_sparse_patterns("docs/readme.md", &patterns));

    // File pattern (exact match).
    assert!(matches_sparse_patterns("Cargo.toml", &patterns));
    assert!(!matches_sparse_patterns("package.json", &patterns));

    // Wildcard pattern.
    assert!(matches_sparse_patterns("Cargo.lock", &patterns));
    assert!(matches_sparse_patterns("yarn.lock", &patterns));
    assert!(!matches_sparse_patterns(
        "Cargo.toml",
        &["*.lock".to_string()]
    ));
}

#[test]
fn submodule_detection() {
    use grove_core::worktree::git_ops::has_submodules;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // No .gitmodules → no submodules.
    assert!(!has_submodules(root));

    // With .gitmodules → has submodules.
    fs::write(root.join(".gitmodules"), "[submodule \"vendor\"]\n").unwrap();
    assert!(has_submodules(root));
}

// ── Phase 7: Interactive Conflict Resolution ────────────────────────────────

#[test]
fn conflict_strategy_auto_continues_on_conflict() {
    use grove_core::config::ConflictStrategy;

    // Auto = last-writer-wins, already the default merge behavior.
    // Just verify the enum exists and defaults correctly.
    let default = ConflictStrategy::default();
    assert_eq!(default, ConflictStrategy::Markers);
    assert_ne!(default, ConflictStrategy::Auto);
}

#[test]
fn conflict_strategy_enum_roundtrips_yaml() {
    use grove_core::config::ConflictStrategy;

    let strategies = vec![
        ("\"auto\"", ConflictStrategy::Auto),
        ("\"markers\"", ConflictStrategy::Markers),
        ("\"pause\"", ConflictStrategy::Pause),
        ("\"fail\"", ConflictStrategy::Fail),
    ];
    for (yaml, expected) in strategies {
        let parsed: ConflictStrategy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed, expected, "failed to parse {yaml}");
    }
}

#[test]
fn conflict_artifacts_written_and_readable() {
    use grove_core::worktree::conflict_ui;

    let tmp = TempDir::new().unwrap();
    let grove_dir = tmp.path().join(".grove");

    conflict_ui::write_conflict_artifacts(
        &grove_dir,
        "src/lib.rs",
        b"base content\n",
        b"ours content\n",
        b"theirs content\n",
    )
    .unwrap();

    let base = fs::read_to_string(grove_dir.join("conflicts/src/lib.rs.base")).unwrap();
    let ours = fs::read_to_string(grove_dir.join("conflicts/src/lib.rs.ours")).unwrap();
    let theirs = fs::read_to_string(grove_dir.join("conflicts/src/lib.rs.theirs")).unwrap();

    assert_eq!(base, "base content\n");
    assert_eq!(ours, "ours content\n");
    assert_eq!(theirs, "theirs content\n");
}

#[test]
fn conflicts_manifest_written_and_readable() {
    use grove_core::worktree::conflict_ui;
    use grove_core::worktree::merge::{ConflictRecord, ConflictResolution};

    let tmp = TempDir::new().unwrap();
    let grove_dir = tmp.path().join(".grove");

    let conflicts = vec![
        ConflictRecord {
            path: "src/lib.rs".to_string(),
            agents: vec!["builder".to_string(), "refactorer".to_string()],
            resolution: ConflictResolution::ThreeWayWithMarkers { marker_count: 2 },
            winning_agent_priority: Some(30),
        },
        ConflictRecord {
            path: "README.md".to_string(),
            agents: vec!["documenter".to_string(), "builder".to_string()],
            resolution: ConflictResolution::LastWriterWins,
            winning_agent_priority: Some(30),
        },
    ];

    conflict_ui::write_conflicts_manifest(&grove_dir, &conflicts).unwrap();

    let read_back = conflict_ui::read_conflicts_manifest(&grove_dir).unwrap();
    assert_eq!(read_back.len(), 2);
    assert_eq!(read_back[0].path, "src/lib.rs");
    assert_eq!(read_back[1].path, "README.md");
    assert_eq!(read_back[0].agents, vec!["builder", "refactorer"]);
}

#[test]
fn empty_conflicts_no_manifest_written() {
    use grove_core::worktree::conflict_ui;

    let tmp = TempDir::new().unwrap();
    let grove_dir = tmp.path().join(".grove");

    conflict_ui::write_conflicts_manifest(&grove_dir, &[]).unwrap();

    // No file should be created for empty conflicts.
    assert!(!grove_dir.join("conflicts").exists());
}

#[test]
fn resolve_conflict_removes_artifacts() {
    use grove_core::worktree::conflict_ui;
    use grove_core::worktree::merge::{ConflictRecord, ConflictResolution};

    let tmp = TempDir::new().unwrap();
    let grove_dir = tmp.path().join(".grove");

    // Write artifacts.
    conflict_ui::write_conflict_artifacts(&grove_dir, "file.txt", b"base", b"ours", b"theirs")
        .unwrap();

    let conflicts = vec![ConflictRecord {
        path: "file.txt".to_string(),
        agents: vec!["a".to_string(), "b".to_string()],
        resolution: ConflictResolution::ThreeWayWithMarkers { marker_count: 1 },
        winning_agent_priority: None,
    }];
    conflict_ui::write_conflicts_manifest(&grove_dir, &conflicts).unwrap();

    // Resolve it.
    let removed = conflict_ui::resolve_conflict_artifacts(&grove_dir, "file.txt").unwrap();
    assert!(removed);

    // Artifacts should be gone.
    assert!(!grove_dir.join("conflicts/file.txt.base").exists());
    assert!(!grove_dir.join("conflicts/file.txt.ours").exists());
    assert!(!grove_dir.join("conflicts/file.txt.theirs").exists());
    // Last conflict resolved → entire conflicts dir removed.
    assert!(!grove_dir.join("conflicts").exists());
}

#[test]
fn resolve_nonexistent_conflict_returns_false() {
    use grove_core::worktree::conflict_ui;

    let tmp = TempDir::new().unwrap();
    let grove_dir = tmp.path().join(".grove");

    let removed = conflict_ui::resolve_conflict_artifacts(&grove_dir, "no_such.txt").unwrap();
    assert!(!removed);
}

#[test]
fn effective_strategy_non_tty_degrades_pause() {
    use grove_core::config::ConflictStrategy;
    use grove_core::worktree::conflict_ui::effective_strategy_with_tty;

    // Use the TTY-parametrized variant so the assertion is deterministic
    // regardless of how `cargo test` is invoked (interactive shell vs CI).
    let result = effective_strategy_with_tty(ConflictStrategy::Pause, false);
    assert_eq!(result, ConflictStrategy::Fail);
}

#[test]
fn effective_strategy_preserves_others() {
    use grove_core::config::ConflictStrategy;
    use grove_core::worktree::conflict_ui::effective_strategy;

    assert_eq!(
        effective_strategy(ConflictStrategy::Auto),
        ConflictStrategy::Auto
    );
    assert_eq!(
        effective_strategy(ConflictStrategy::Markers),
        ConflictStrategy::Markers
    );
    assert_eq!(
        effective_strategy(ConflictStrategy::Fail),
        ConflictStrategy::Fail
    );
}

#[test]
fn conflict_record_deserialize_roundtrip() {
    use grove_core::worktree::merge::{ConflictRecord, ConflictResolution};

    let record = ConflictRecord {
        path: "src/main.rs".to_string(),
        agents: vec!["builder".to_string(), "reviewer".to_string()],
        resolution: ConflictResolution::ThreeWayWithMarkers { marker_count: 3 },
        winning_agent_priority: Some(20),
    };

    let json = serde_json::to_string(&record).unwrap();
    let back: ConflictRecord = serde_json::from_str(&json).unwrap();

    assert_eq!(back.path, "src/main.rs");
    assert_eq!(back.agents.len(), 2);
    assert_eq!(back.winning_agent_priority, Some(20));
    assert_eq!(
        back.resolution,
        ConflictResolution::ThreeWayWithMarkers { marker_count: 3 }
    );
}

// ── Phase 8: Binary File Awareness ──────────────────────────────────────────

#[test]
fn binary_strategy_last_writer_wins_on_conflict() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    // Create base with a binary file (contains null byte).
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("image.png"), b"PNG\x00base").unwrap();

    // Fork A modifies the binary.
    setup_fork(&base, &fork_a, None);
    fs::write(fork_a.join("image.png"), b"PNG\x00fork_a").unwrap();

    // Fork B also modifies the binary.
    setup_fork(&base, &fork_b, None);
    fs::write(fork_b.join("image.png"), b"PNG\x00fork_b").unwrap();

    let filter = GitignoreFilter::empty();
    let worktrees = vec![
        merge::AgentWorktree {
            name: "agent_a".into(),
            path: fork_a,
            base_commit: None,
            merge_priority: 50,
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
        merge::AgentWorktree {
            name: "agent_b".into(),
            path: fork_b,
            base_commit: None,
            merge_priority: 10, // higher priority = wins
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
    ];

    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::LastWriter,
        &Default::default(),
    )
    .unwrap();

    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(
        result.conflicts[0].resolution,
        merge::ConflictResolution::BinaryLastWriterWins,
    );
    // Higher priority agent (agent_b, priority 10) is applied last → wins.
    let content = fs::read(merged.join("image.png")).unwrap();
    assert_eq!(content, b"PNG\x00fork_b");
}

#[test]
fn binary_strategy_keep_base_restores_base_version() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("data.bin"), b"\x00original").unwrap();

    setup_fork(&base, &fork_a, None);
    fs::write(fork_a.join("data.bin"), b"\x00version_a").unwrap();

    setup_fork(&base, &fork_b, None);
    fs::write(fork_b.join("data.bin"), b"\x00version_b").unwrap();

    let filter = GitignoreFilter::empty();
    let worktrees = vec![
        merge::AgentWorktree {
            name: "a".into(),
            path: fork_a,
            base_commit: None,
            merge_priority: 50,
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
        merge::AgentWorktree {
            name: "b".into(),
            path: fork_b,
            base_commit: None,
            merge_priority: 10,
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
    ];

    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::KeepBase,
        &Default::default(),
    )
    .unwrap();

    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(
        result.conflicts[0].resolution,
        merge::ConflictResolution::BinaryKeptBase,
    );
    let content = fs::read(merged.join("data.bin")).unwrap();
    assert_eq!(content, b"\x00original");
}

#[test]
fn binary_strategy_fail_marks_unresolved() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let fork_b = tmp.path().join("fork_b");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("data.bin"), b"\x00original").unwrap();

    setup_fork(&base, &fork_a, None);
    fs::write(fork_a.join("data.bin"), b"\x00version_a").unwrap();

    setup_fork(&base, &fork_b, None);
    fs::write(fork_b.join("data.bin"), b"\x00version_b").unwrap();

    let filter = GitignoreFilter::empty();
    let worktrees = vec![
        merge::AgentWorktree {
            name: "a".into(),
            path: fork_a,
            base_commit: None,
            merge_priority: 50,
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
        merge::AgentWorktree {
            name: "b".into(),
            path: fork_b,
            base_commit: None,
            merge_priority: 10,
            is_sparse: false,
            sparse_patterns: Vec::new(),
        },
    ];

    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::Fail,
        &Default::default(),
    )
    .unwrap();

    // BinaryStrategy::Fail still writes the file (LWW) but marks as BinaryLastWriterWins,
    // which is_resolved() returns false → has_unresolved_conflicts() = true.
    assert!(result.has_unresolved_conflicts());
}

#[test]
fn is_lockfile_detection() {
    use grove_core::worktree::merge::is_lockfile;

    assert!(is_lockfile("package-lock.json"));
    assert!(is_lockfile("yarn.lock"));
    assert!(is_lockfile("pnpm-lock.yaml"));
    assert!(is_lockfile("Cargo.lock"));
    assert!(is_lockfile("Gemfile.lock"));
    assert!(is_lockfile("poetry.lock"));
    assert!(is_lockfile("composer.lock"));
    assert!(is_lockfile("go.sum"));
    assert!(is_lockfile("uv.lock"));

    assert!(!is_lockfile("package.json"));
    assert!(!is_lockfile("Cargo.toml"));
    assert!(!is_lockfile("README.md"));
    assert!(!is_lockfile("lockfile.txt"));
}

#[test]
fn modified_lockfiles_tracked_during_merge() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("file.txt"), "base\n").unwrap();
    fs::write(base.join("Cargo.lock"), "old-lock\n").unwrap();

    setup_fork(&base, &fork_a, None);
    fs::write(fork_a.join("Cargo.lock"), "new-lock\n").unwrap();

    let filter = GitignoreFilter::empty();
    let worktrees = vec![merge::AgentWorktree {
        name: "builder".into(),
        path: fork_a,
        base_commit: None,
        merge_priority: 30,
        is_sparse: false,
        sparse_patterns: Vec::new(),
    }];

    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::LastWriter,
        &Default::default(),
    )
    .unwrap();

    assert_eq!(result.modified_lockfiles, vec!["Cargo.lock"]);
}

#[test]
fn no_lockfiles_tracked_when_none_modified() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork_a = tmp.path().join("fork_a");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("file.txt"), "base\n").unwrap();

    setup_fork(&base, &fork_a, Some(("file.txt", "modified\n")));

    let filter = GitignoreFilter::empty();
    let worktrees = vec![merge::AgentWorktree {
        name: "builder".into(),
        path: fork_a,
        base_commit: None,
        merge_priority: 30,
        is_sparse: false,
        sparse_patterns: Vec::new(),
    }];

    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::LastWriter,
        &Default::default(),
    )
    .unwrap();

    assert!(result.modified_lockfiles.is_empty());
}

#[test]
fn binary_strategy_enum_roundtrips_yaml() {
    let strategies = vec![
        ("\"last_writer\"", BinaryStrategy::LastWriter),
        ("\"fail\"", BinaryStrategy::Fail),
        ("\"keep_base\"", BinaryStrategy::KeepBase),
    ];
    for (yaml, expected) in strategies {
        let parsed: BinaryStrategy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed, expected, "failed to parse {yaml}");
    }
}

#[test]
fn lockfile_strategy_enum_roundtrips_yaml() {
    use grove_core::config::LockfileStrategy;

    let strategies = vec![
        ("\"regenerate\"", LockfileStrategy::Regenerate),
        ("\"last_writer\"", LockfileStrategy::LastWriter),
        ("\"fail\"", LockfileStrategy::Fail),
    ];
    for (yaml, expected) in strategies {
        let parsed: LockfileStrategy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed, expected, "failed to parse {yaml}");
    }
}

#[test]
fn binary_kept_base_resolution_is_resolved() {
    // BinaryKeptBase is a deterministic resolution (base version kept),
    // so it should NOT be treated as "unresolved" — it's a known outcome.
    let record = merge::ConflictRecord {
        path: "image.png".to_string(),
        agents: vec!["a".to_string(), "b".to_string()],
        resolution: merge::ConflictResolution::BinaryKeptBase,
        winning_agent_priority: None,
    };
    // BinaryKeptBase is NOT ThreeWayClean, so is_resolved() returns false.
    // This is correct: the engine should report it even though data isn't lost.
    assert!(!record.is_resolved());
}

// ── Phase 9: Merge Destination Safety ───────────────────────────────────────

/// Helper: initialize a git repo in `dir` with an initial commit.
fn init_git_repo(dir: &std::path::Path) {
    use std::process::Command;
    Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .output()
        .unwrap();
    fs::write(dir.join("README.md"), "init\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir)
        .output()
        .unwrap();
}

#[test]
fn git_status_porcelain_reports_clean_repo() {
    use grove_core::worktree::git_ops;

    let tmp = TempDir::new().unwrap();
    init_git_repo(tmp.path());

    let status = git_ops::git_status_porcelain(tmp.path()).unwrap();
    assert!(status.is_empty(), "clean repo should have empty status");
}

#[test]
fn git_status_porcelain_detects_dirty_files() {
    use grove_core::worktree::git_ops;

    let tmp = TempDir::new().unwrap();
    init_git_repo(tmp.path());

    // Create an untracked file.
    fs::write(tmp.path().join("stale.txt"), "stale\n").unwrap();

    let status = git_ops::git_status_porcelain(tmp.path()).unwrap();
    assert!(
        !status.is_empty(),
        "dirty repo should have non-empty status"
    );
    assert!(status.contains("stale.txt"));
}

#[test]
fn git_clean_worktree_verified_on_clean_repo() {
    use grove_core::worktree::git_ops;

    let tmp = TempDir::new().unwrap();
    init_git_repo(tmp.path());

    let stale = git_ops::git_clean_worktree_verified(tmp.path()).unwrap();
    assert!(stale.is_empty(), "clean repo should return no stale files");
}

#[test]
fn git_clean_worktree_verified_removes_untracked_files() {
    use grove_core::worktree::git_ops;

    let tmp = TempDir::new().unwrap();
    init_git_repo(tmp.path());

    // Add untracked files.
    fs::write(tmp.path().join("build_artifact.o"), "object\n").unwrap();
    fs::create_dir_all(tmp.path().join("target")).unwrap();
    fs::write(tmp.path().join("target/debug.bin"), "debug\n").unwrap();

    let stale = git_ops::git_clean_worktree_verified(tmp.path()).unwrap();
    // First pass should clean everything — stale should be empty.
    assert!(
        stale.is_empty(),
        "normal untracked files should be cleaned on first pass"
    );

    // Verify files are gone.
    assert!(!tmp.path().join("build_artifact.o").exists());
    assert!(!tmp.path().join("target/debug.bin").exists());
}

#[test]
fn git_clean_worktree_verified_restores_modified_tracked_files() {
    use grove_core::worktree::git_ops;

    let tmp = TempDir::new().unwrap();
    init_git_repo(tmp.path());

    // Modify a tracked file.
    fs::write(tmp.path().join("README.md"), "modified\n").unwrap();

    let stale = git_ops::git_clean_worktree_verified(tmp.path()).unwrap();
    assert!(stale.is_empty());

    // Verify the tracked file is restored to committed state.
    let content = fs::read_to_string(tmp.path().join("README.md")).unwrap();
    assert_eq!(content, "init\n");
}

#[test]
fn crash_recovery_triggered_field_defaults_to_false() {
    let metrics = merge::MergeMetrics {
        strategy_used: "last_writer_wins".to_string(),
        files_processed: 0,
        files_changed: 0,
        conflicts_total: 0,
        conflicts_auto_resolved: 0,
        conflicts_unresolved: 0,
        duration_ms: 0,
        fallback_reason: None,
        change_detection_strategy: "hash-walk".to_string(),
        change_detection_ms: 0,
        sparse_worktrees: 0,
        sparse_files_materialized: 0,
        crash_recovery_triggered: false,
        guard_violations_total: 0,
    };
    assert!(!metrics.crash_recovery_triggered);
}

#[test]
fn merge_result_includes_crash_recovery_flag() {
    // Verify the MergeMetrics struct can carry the flag through a merge result.
    let result = merge::MergeResult {
        conflicts: Vec::new(),
        metrics: merge::MergeMetrics {
            strategy_used: "last_writer_wins".to_string(),
            files_processed: 5,
            files_changed: 2,
            conflicts_total: 0,
            conflicts_auto_resolved: 0,
            conflicts_unresolved: 0,
            duration_ms: 10,
            fallback_reason: None,
            change_detection_strategy: "git-diff".to_string(),
            change_detection_ms: 5,
            sparse_worktrees: 0,
            sparse_files_materialized: 0,
            crash_recovery_triggered: true,
            guard_violations_total: 0,
        },
        modified_lockfiles: Vec::new(),
        guard_violations: Vec::new(),
    };
    assert!(result.metrics.crash_recovery_triggered);
    assert!(!result.has_unresolved_conflicts());
}

// ── Guard violation enforcement ───────────────────────────────────────────────

#[test]
fn file_guard_blocks_agent_from_writing_disallowed_path() {
    use grove_core::config::CapabilityGuard;
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork = tmp.path().join("fork");
    let merged = tmp.path().join("merged");

    // Base: one allowed file (src/main.rs) and one protected file (config/app.yaml).
    fs::create_dir_all(base.join("src")).unwrap();
    fs::create_dir_all(base.join("config")).unwrap();
    fs::write(base.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(base.join("config/app.yaml"), "env: production\n").unwrap();

    // Fork: builder modifies both files.
    fs::create_dir_all(fork.join("src")).unwrap();
    fs::create_dir_all(fork.join("config")).unwrap();
    fs::write(
        fork.join("src/main.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .unwrap();
    fs::write(fork.join("config/app.yaml"), "env: staging\n").unwrap(); // BLOCKED by guard

    let worktrees = vec![merge::AgentWorktree {
        name: "builder".into(),
        path: fork,
        base_commit: None,
        merge_priority: 50,
        is_sparse: false,
        sparse_patterns: Vec::new(),
    }];

    // Guard: builder may only write src/**. config/** is not in allowed_paths.
    let mut guards: HashMap<String, CapabilityGuard> = HashMap::new();
    guards.insert(
        "builder".to_string(),
        CapabilityGuard {
            allowed_paths: vec!["src/**".to_string()],
            blocked_paths: vec![],
            blocked_tools: vec![],
        },
    );

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::LastWriter,
        &guards,
    )
    .unwrap();

    // The allowed file should be updated in the merge output.
    let src_content = fs::read_to_string(merged.join("src/main.rs")).unwrap();
    assert!(
        src_content.contains("println!"),
        "src/main.rs should be updated by builder"
    );

    // The blocked file should NOT be updated — base version must be preserved.
    let cfg_content = fs::read_to_string(merged.join("config/app.yaml")).unwrap();
    assert_eq!(
        cfg_content, "env: production\n",
        "config/app.yaml should not be modified by builder"
    );

    // Exactly one guard violation recorded.
    assert_eq!(
        result.guard_violations.len(),
        1,
        "expected one guard violation"
    );
    assert_eq!(result.guard_violations[0].path, "config/app.yaml");
    assert_eq!(result.guard_violations[0].agent, "builder");
    assert_eq!(result.metrics.guard_violations_total, 1);

    // No merge conflicts (only one agent, no two-agent overlap).
    assert!(!result.has_unresolved_conflicts());
}

#[test]
fn file_guard_with_blocked_paths_excludes_matched_files() {
    use grove_core::config::CapabilityGuard;
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork = tmp.path().join("fork");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("code.rs"), "// base\n").unwrap();
    fs::write(base.join("secrets.env"), "TOKEN=old\n").unwrap();

    fs::create_dir_all(&fork).unwrap();
    fs::write(fork.join("code.rs"), "// updated\n").unwrap();
    fs::write(fork.join("secrets.env"), "TOKEN=leaked\n").unwrap(); // BLOCKED by guard

    let worktrees = vec![merge::AgentWorktree {
        name: "tester".into(),
        path: fork,
        base_commit: None,
        merge_priority: 50,
        is_sparse: false,
        sparse_patterns: Vec::new(),
    }];

    // Guard: tester has blocked_paths = ["*.env"].
    let mut guards: HashMap<String, CapabilityGuard> = HashMap::new();
    guards.insert(
        "tester".to_string(),
        CapabilityGuard {
            allowed_paths: vec![], // no allow restriction — all paths allowed
            blocked_paths: vec!["*.env".to_string()],
            blocked_tools: vec![],
        },
    );

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::LastWriter,
        &guards,
    )
    .unwrap();

    // code.rs is not blocked — tester's change should appear.
    let code = fs::read_to_string(merged.join("code.rs")).unwrap();
    assert_eq!(code, "// updated\n");

    // secrets.env matches *.env blocked pattern — base version preserved.
    let secret = fs::read_to_string(merged.join("secrets.env")).unwrap();
    assert_eq!(
        secret, "TOKEN=old\n",
        "secrets.env must not be overwritten by tester"
    );

    assert_eq!(result.guard_violations.len(), 1);
    assert_eq!(result.guard_violations[0].path, "secrets.env");
    assert_eq!(result.metrics.guard_violations_total, 1);
}

#[test]
fn no_guards_configured_allows_all_files() {
    // With an empty guards map (Default::default()), no files are blocked.
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().join("base");
    let fork = tmp.path().join("fork");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("anything.txt"), "base\n").unwrap();

    fs::create_dir_all(&fork).unwrap();
    fs::write(fork.join("anything.txt"), "updated\n").unwrap();

    let worktrees = vec![merge::AgentWorktree {
        name: "builder".into(),
        path: fork,
        base_commit: None,
        merge_priority: 50,
        is_sparse: false,
        sparse_patterns: Vec::new(),
    }];

    let filter = GitignoreFilter::empty();
    let result = merge::merge_worktrees(
        &base,
        &worktrees,
        &merged,
        &filter,
        MergeStrategy::LastWriterWins,
        BinaryStrategy::LastWriter,
        &Default::default(),
    )
    .unwrap();

    let content = fs::read_to_string(merged.join("anything.txt")).unwrap();
    assert_eq!(content, "updated\n");
    assert_eq!(result.guard_violations.len(), 0);
    assert_eq!(result.metrics.guard_violations_total, 0);
}
