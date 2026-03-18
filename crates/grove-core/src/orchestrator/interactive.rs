use std::io::{self, BufRead, Write};
use std::path::Path;

use crate::agents::AgentType;
use crate::errors::GroveResult;
use crate::providers::ProviderResponse;

/// Outcome returned by [`pause_and_prompt`].
pub enum PauseOutcome {
    Continue,
    ContinueWithNote(String),
    Retry,
    Abort,
}

/// Pause after an agent completes and prompt the user for input.
///
/// Shows changed files, a summary snippet, and cost. Reads a single line
/// from stdin.  When stdin is not a TTY (e.g. in CI), returns
/// `PauseOutcome::Continue` so automated pipelines are never blocked.
pub fn pause_and_prompt(
    agent_type: AgentType,
    response: &ProviderResponse,
    worktree_path: &Path,
    seed_path: &Path,
) -> GroveResult<PauseOutcome> {
    let label = agent_type.as_str().to_uppercase();

    eprintln!();
    eprintln!("✓ [{label}] Done.");
    eprintln!();

    // --- Changed files ---
    let diff = diff_files(seed_path, worktree_path);
    if diff.is_empty() {
        eprintln!("  (no file changes detected)");
    } else {
        eprintln!("  Changed files:");
        for entry in &diff {
            eprintln!("    {entry}");
        }
    }
    eprintln!();

    // --- Summary snippet (first 5 non-empty lines) ---
    let snippet: Vec<&str> = response
        .summary
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(5)
        .collect();
    if !snippet.is_empty() {
        eprintln!("  Summary:");
        for line in &snippet {
            eprintln!("    {line}");
        }
        eprintln!();
    }

    // --- Cost ---
    if let Some(cost) = response.cost_usd {
        if cost > 0.0 {
            eprintln!("  Cost: ${cost:.4}");
            eprintln!();
        }
    }

    // --- Menu ---
    eprintln!("  [c] Continue  [n] Add note  [r] Retry  [a] Abort");
    eprint!("  > ");
    io::stderr().flush().ok();

    let line = read_line_from_stdin();
    let choice = line.trim().to_lowercase();

    match choice.as_str() {
        "a" | "abort" => Ok(PauseOutcome::Abort),
        "r" | "retry" => Ok(PauseOutcome::Retry),
        "n" | "note" => {
            eprint!("  Note: ");
            io::stderr().flush().ok();
            let note = read_line_from_stdin();
            let note = note.trim().to_string();
            if note.is_empty() {
                Ok(PauseOutcome::Continue)
            } else {
                Ok(PauseOutcome::ContinueWithNote(note))
            }
        }
        // "" | "c" | "continue" | anything else → Continue
        _ => Ok(PauseOutcome::Continue),
    }
}

/// Build the feedback prefix prepended to the next agent's instructions.
pub fn format_feedback_prefix(from_agent: AgentType, note: &str) -> String {
    format!(
        "FEEDBACK FROM [{}] REVIEW: \"{}\"\n\nTake this into account.\n\n",
        from_agent.as_str().to_uppercase(),
        note
    )
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Read a single line from stdin. Returns empty string on EOF or error
/// (non-TTY / CI fallback → `PauseOutcome::Continue`).
fn read_line_from_stdin() -> String {
    let stdin = io::stdin();
    let mut line = String::new();
    let _ = stdin.lock().read_line(&mut line);
    line
}

/// Compare `seed` and `worktree` directories. Returns display strings:
/// - `+ path` — new file added by the agent
/// - `~ path` — file modified (different size)
/// - `- path` — file deleted by the agent
///
/// Skips `.git`, `.grove`, and grove-internal files.
fn diff_files(seed: &Path, worktree: &Path) -> Vec<String> {
    use crate::worktree::gitignore::GitignoreFilter;
    let filter = GitignoreFilter::load(seed);

    let seed_sizes = collect_file_sizes(seed, seed, &filter);
    let wt_sizes = collect_file_sizes(worktree, worktree, &filter);

    let mut entries: Vec<String> = Vec::new();

    // New or modified files in worktree
    for (rel, wt_size) in &wt_sizes {
        match seed_sizes.get(rel) {
            None => entries.push(format!("+ {rel}")),
            Some(seed_size) if seed_size != wt_size => entries.push(format!("~ {rel}")),
            _ => {}
        }
    }

    // Deleted files
    for rel in seed_sizes.keys() {
        if !wt_sizes.contains_key(rel.as_str()) {
            entries.push(format!("- {rel}"));
        }
    }

    entries.sort();
    entries
}

/// Recursively collect `(relative_path → file_size)` for all non-protected files.
fn collect_file_sizes(
    root: &Path,
    dir: &Path,
    filter: &crate::worktree::gitignore::GitignoreFilter,
) -> std::collections::HashMap<String, u64> {
    use crate::worktree::gitignore::is_grove_internal_file;
    let mut map = std::collections::HashMap::new();
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
            // Symlinks have no meaningful "size" — report 0.
            if let Ok(rel) = path.strip_prefix(root) {
                map.insert(rel.to_string_lossy().into_owned(), 0);
            }
        } else if ft.is_file() {
            let size = path.metadata().map(|m| m.len()).unwrap_or(0);
            if let Ok(rel) = path.strip_prefix(root) {
                map.insert(rel.to_string_lossy().into_owned(), size);
            }
        } else if path.is_dir() {
            map.extend(collect_file_sizes(root, &path, filter));
        }
    }
    map
}
