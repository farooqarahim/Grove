use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;
use std::time::Duration;

use crate::config::ConflictStrategy;
use crate::errors::{GroveError, GroveResult};
use crate::worktree::merge::ConflictRecord;

// ── TTY detection + auto-degradation ────────────────────────────────────────

/// Resolve the effective conflict strategy, degrading `Pause` to `Fail` in
/// non-TTY environments to prevent hanging in CI.
pub fn effective_strategy(configured: ConflictStrategy) -> ConflictStrategy {
    effective_strategy_with_tty(configured, io::stdin().is_terminal())
}

/// Pure-logic variant of [`effective_strategy`] that takes an explicit
/// `is_tty` flag — testable without depending on the runtime environment.
pub fn effective_strategy_with_tty(configured: ConflictStrategy, is_tty: bool) -> ConflictStrategy {
    match configured {
        ConflictStrategy::Pause if !is_tty => {
            tracing::warn!(
                "conflict_strategy is 'pause' but stdin is not a TTY — \
                 degrading to 'fail' to prevent hanging in CI"
            );
            ConflictStrategy::Fail
        }
        other => other,
    }
}

// ── User resolution choices ─────────────────────────────────────────────────

/// The user's choice when interactively resolving a conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserResolution {
    /// Keep the first agent's version ("ours" — already in dest).
    KeepOurs,
    /// Keep the second agent's version ("theirs" — current agent).
    KeepTheirs,
    /// Keep the merged content with conflict markers.
    KeepMarkers,
    /// User edited the file manually — use the edited content.
    ManualEdit(Vec<u8>),
    /// User chose to abort the run.
    Abort,
}

// ── Interactive prompt ──────────────────────────────────────────────────────

/// Prompt the user to resolve a conflict interactively.
///
/// Shows the conflict details and offers 5 choices:
/// 1. Keep ours (first agent's version)
/// 2. Keep theirs (current agent's version)
/// 3. Keep merged with conflict markers
/// 4. Open in $EDITOR to resolve manually
/// 5. Abort run
///
/// `timeout_secs` is the max time to wait for user input (0 = no timeout).
/// On timeout, returns `Abort`.
///
/// `ours_path` and `theirs_path` are paths to the two versions (for `$EDITOR`).
pub fn prompt_resolution(
    rel_path: &str,
    record: &ConflictRecord,
    timeout_secs: u64,
    dest_path: &Path,
) -> GroveResult<UserResolution> {
    let stderr = io::stderr();
    let mut err = stderr.lock();

    writeln!(err).ok();
    writeln!(err, "--- Merge conflict in: {rel_path} ---").ok();
    writeln!(err, "Agents involved: {}", record.agents.join(", ")).ok();
    writeln!(err, "Resolution so far: {:?}", record.resolution).ok();
    writeln!(err).ok();
    writeln!(err, "Options:").ok();
    if record.agents.len() >= 2 {
        writeln!(
            err,
            "  [1] Keep agent {}'s version (ours)",
            record.agents[0]
        )
        .ok();
        writeln!(
            err,
            "  [2] Keep agent {}'s version (theirs)",
            record.agents[1]
        )
        .ok();
    } else {
        writeln!(err, "  [1] Keep first version (ours)").ok();
        writeln!(err, "  [2] Keep second version (theirs)").ok();
    }
    writeln!(err, "  [3] Keep merged with conflict markers").ok();
    writeln!(err, "  [4] Open in $EDITOR to resolve manually").ok();
    writeln!(err, "  [5] Abort run").ok();
    write!(err, "\nChoice [1-5]: ").ok();
    err.flush().ok();

    let choice = read_line_with_timeout(timeout_secs)?;
    let choice = choice.trim();

    match choice {
        "1" => Ok(UserResolution::KeepOurs),
        "2" => Ok(UserResolution::KeepTheirs),
        "3" => Ok(UserResolution::KeepMarkers),
        "4" => {
            let edited = open_in_editor(dest_path)?;
            Ok(UserResolution::ManualEdit(edited))
        }
        "5" | "" => Ok(UserResolution::Abort),
        _ => {
            writeln!(err, "Invalid choice '{choice}', treating as abort.").ok();
            Ok(UserResolution::Abort)
        }
    }
}

/// Read a line from stdin with an optional timeout.
///
/// If `timeout_secs` is 0, waits indefinitely.
/// On timeout, returns an error.
fn read_line_with_timeout(timeout_secs: u64) -> GroveResult<String> {
    if timeout_secs == 0 {
        // No timeout — blocking read.
        let stdin = io::stdin();
        let mut line = String::new();
        stdin
            .lock()
            .read_line(&mut line)
            .map_err(|e| GroveError::Runtime(format!("read stdin: {e}")))?;
        return Ok(line);
    }

    // With timeout: spawn a thread to read, join with timeout.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut line = String::new();
        let result = stdin.lock().read_line(&mut line);
        let _ = tx.send(result.map(|_| line));
    });

    match rx.recv_timeout(Duration::from_secs(timeout_secs)) {
        Ok(Ok(line)) => Ok(line),
        Ok(Err(e)) => Err(GroveError::Runtime(format!("read stdin: {e}"))),
        Err(_) => {
            tracing::warn!(
                timeout_secs,
                "conflict resolution timed out — treating as abort"
            );
            Err(GroveError::Runtime(format!(
                "conflict resolution timed out after {timeout_secs}s"
            )))
        }
    }
}

/// Open a file in `$EDITOR` (or `vi` as fallback) and return the edited content.
fn open_in_editor(file_path: &Path) -> GroveResult<Vec<u8>> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    let status = std::process::Command::new(&editor)
        .arg(file_path)
        .env("PATH", crate::capability::shell_path())
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| GroveError::Runtime(format!("launch {editor}: {e}")))?;

    if !status.success() {
        return Err(GroveError::Runtime(format!(
            "{editor} exited with status {status}"
        )));
    }

    std::fs::read(file_path).map_err(|e| GroveError::Runtime(format!("read edited file: {e}")))
}

// ── Conflict artifacts ──────────────────────────────────────────────────────

/// Write the three versions of a conflicted file to `.grove/conflicts/`.
///
/// Creates:
///   `.grove/conflicts/<rel_path>.base`   — common ancestor
///   `.grove/conflicts/<rel_path>.ours`   — first agent's version
///   `.grove/conflicts/<rel_path>.theirs` — second agent's version
pub fn write_conflict_artifacts(
    grove_dir: &Path,
    rel_path: &str,
    base_content: &[u8],
    ours_content: &[u8],
    theirs_content: &[u8],
) -> GroveResult<()> {
    let conflict_dir = grove_dir.join("conflicts");
    let base_path = conflict_dir.join(format!("{rel_path}.base"));
    let ours_path = conflict_dir.join(format!("{rel_path}.ours"));
    let theirs_path = conflict_dir.join(format!("{rel_path}.theirs"));

    for p in [&base_path, &ours_path, &theirs_path] {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| GroveError::Runtime(format!("mkdir {}: {e}", parent.display())))?;
        }
    }

    std::fs::write(&base_path, base_content)
        .map_err(|e| GroveError::Runtime(format!("write {}: {e}", base_path.display())))?;
    std::fs::write(&ours_path, ours_content)
        .map_err(|e| GroveError::Runtime(format!("write {}: {e}", ours_path.display())))?;
    std::fs::write(&theirs_path, theirs_content)
        .map_err(|e| GroveError::Runtime(format!("write {}: {e}", theirs_path.display())))?;

    Ok(())
}

/// Write `CONFLICTS.json` manifest with all conflict records.
pub fn write_conflicts_manifest(grove_dir: &Path, conflicts: &[ConflictRecord]) -> GroveResult<()> {
    if conflicts.is_empty() {
        return Ok(());
    }
    let manifest_path = grove_dir.join("conflicts").join("CONFLICTS.json");
    if let Some(parent) = manifest_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| GroveError::Runtime(format!("mkdir {}: {e}", parent.display())))?;
    }
    let json = serde_json::to_string_pretty(conflicts)
        .map_err(|e| GroveError::Runtime(format!("serialize conflicts: {e}")))?;
    std::fs::write(&manifest_path, json)
        .map_err(|e| GroveError::Runtime(format!("write {}: {e}", manifest_path.display())))?;

    tracing::info!(
        path = %manifest_path.display(),
        count = conflicts.len(),
        "conflict artifacts saved"
    );
    Ok(())
}

/// Read the `CONFLICTS.json` manifest from `.grove/conflicts/`.
///
/// Returns `None` if the file doesn't exist.
pub fn read_conflicts_manifest(grove_dir: &Path) -> Option<Vec<ConflictRecord>> {
    let manifest_path = grove_dir.join("conflicts").join("CONFLICTS.json");
    let content = std::fs::read_to_string(&manifest_path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Remove conflict artifacts for a specific file path.
///
/// Deletes the `.base`, `.ours`, `.theirs` files and updates `CONFLICTS.json`.
pub fn resolve_conflict_artifacts(grove_dir: &Path, rel_path: &str) -> GroveResult<bool> {
    let conflict_dir = grove_dir.join("conflicts");

    let mut removed = false;
    for ext in ["base", "ours", "theirs"] {
        let p = conflict_dir.join(format!("{rel_path}.{ext}"));
        if p.exists() {
            std::fs::remove_file(&p)
                .map_err(|e| GroveError::Runtime(format!("remove {}: {e}", p.display())))?;
            removed = true;
        }
    }

    // Update the manifest to remove this conflict.
    if let Some(mut conflicts) = read_conflicts_manifest(grove_dir) {
        conflicts.retain(|c| c.path != rel_path);
        if conflicts.is_empty() {
            // Remove the entire conflicts directory.
            let _ = std::fs::remove_dir_all(&conflict_dir);
        } else {
            write_conflicts_manifest(grove_dir, &conflicts)?;
        }
    }

    Ok(removed)
}
