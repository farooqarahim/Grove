//! Git output filter — handles diff, log, status, and show subcommands.

use super::super::session::FilterState;
use super::universal::strip_ansi;

/// Fallback constants (used when state has zero values).
const DEFAULT_HUNK_LINES: usize = 30;
const DEFAULT_DIFF_LINES: usize = 500;
const DEFAULT_COMMITS: usize = 10;

/// Filter git command output based on compression level.
pub fn filter(output: &str, level: u8, state: &FilterState) -> String {
    let cleaned = strip_ansi(output);

    if is_diff_output(&cleaned) {
        filter_diff(&cleaned, level, state)
    } else if is_log_output(&cleaned) {
        filter_log(&cleaned, level, state)
    } else if is_status_output(&cleaned) {
        filter_status(&cleaned, level)
    } else {
        cleaned
    }
}

fn is_diff_output(text: &str) -> bool {
    text.starts_with("diff --git")
        || text.contains("\ndiff --git")
        || text.starts_with("---")
        || text.contains("\n--- a/")
}

fn is_log_output(text: &str) -> bool {
    text.starts_with("commit ") || text.contains("\ncommit ")
}

fn is_status_output(text: &str) -> bool {
    text.contains("On branch ")
        || text.contains("Changes to be committed")
        || text.contains("Changes not staged")
        || text.contains("Untracked files")
        || text.starts_with("## ")
}

/// Filter diff output.
///
/// Level 1: Passthrough (ANSI already stripped).
/// Level 2: Keep stat summary + compact hunks (max lines per hunk).
/// Level 3: Stat summary only, no hunk content.
fn filter_diff(text: &str, level: u8, state: &FilterState) -> String {
    if level <= 1 {
        return text.to_string();
    }

    let max_hunk = if state.max_hunk_lines > 0 {
        state.max_hunk_lines
    } else {
        DEFAULT_HUNK_LINES
    };
    let max_diff = if state.max_diff_lines > 0 {
        state.max_diff_lines
    } else {
        DEFAULT_DIFF_LINES
    };

    let mut result = String::with_capacity(text.len() / 2);
    let mut current_file: Option<String> = None;
    let mut hunk_lines: usize = 0;
    let mut hunk_truncated = false;
    let mut total_lines: usize = 0;
    let mut file_count: usize = 0;
    let mut total_additions: usize = 0;
    let mut total_deletions: usize = 0;

    for line in text.lines() {
        // File header
        if line.starts_with("diff --git") {
            // Flush previous truncation hint
            if hunk_truncated {
                if let Some(ref f) = current_file {
                    result.push_str(&format!(
                        "[grove: hunk truncated at {} lines — git diff -- {} for full diff]\n",
                        max_hunk, f
                    ));
                }
                hunk_truncated = false;
            }

            file_count += 1;
            current_file = line.split(" b/").nth(1).map(|s| s.to_string());
            hunk_lines = 0;

            if level < 3 {
                result.push_str(line);
                result.push('\n');
                total_lines += 1;
            }
            continue;
        }

        // Track additions/deletions
        if line.starts_with('+') && !line.starts_with("+++") {
            total_additions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            total_deletions += 1;
        }

        if level >= 3 {
            continue;
        }

        // Level 2: keep file headers, context around hunks, truncate long hunks.
        if line.starts_with("@@") {
            hunk_lines = 0;
            hunk_truncated = false;
            result.push_str(line);
            result.push('\n');
            total_lines += 1;
        } else if line.starts_with("---") || line.starts_with("+++") || line.starts_with("index ") {
            result.push_str(line);
            result.push('\n');
            total_lines += 1;
        } else {
            hunk_lines += 1;
            if hunk_lines <= max_hunk {
                result.push_str(line);
                result.push('\n');
                total_lines += 1;
            } else if !hunk_truncated {
                hunk_truncated = true;
            }
        }

        if total_lines >= max_diff {
            result.push_str(&format!(
                "[grove: diff truncated at {} lines — {} files total]\n",
                max_diff, file_count
            ));
            return result;
        }
    }

    // Flush final truncation hint
    if hunk_truncated {
        if let Some(ref f) = current_file {
            result.push_str(&format!(
                "[grove: hunk truncated at {} lines — git diff -- {} for full diff]\n",
                max_hunk, f
            ));
        }
    }

    if level >= 3 {
        return format!(
            "[grove: git diff — {} file(s), +{} -{} lines]\n",
            file_count, total_additions, total_deletions
        );
    }

    result
}

/// Filter log output.
///
/// Level 1: Passthrough.
/// Level 2: Hash + subject + author (first N commits). Includes first body
///          line if it starts with "BREAKING CHANGE".
/// Level 3: One-liner summary.
fn filter_log(text: &str, level: u8, state: &FilterState) -> String {
    if level <= 1 {
        return text.to_string();
    }

    let max_commits = if state.max_commits > 0 {
        state.max_commits
    } else {
        DEFAULT_COMMITS
    };

    let mut commits: Vec<LogEntry> = Vec::new();
    let mut current: Option<LogEntry> = None;

    for line in text.lines() {
        if let Some(hash) = line.strip_prefix("commit ") {
            if let Some(entry) = current.take() {
                commits.push(entry);
            }
            current = Some(LogEntry {
                hash: hash.split_whitespace().next().unwrap_or(hash).to_string(),
                subject: String::new(),
                author: String::new(),
                date: String::new(),
                body_first_line: None,
            });
        } else if let Some(ref mut entry) = current {
            if let Some(author) = line.strip_prefix("Author:") {
                entry.author = author.trim().to_string();
            } else if let Some(date) = line.strip_prefix("Date:") {
                entry.date = date.trim().to_string();
            } else if !line.trim().is_empty() && entry.subject.is_empty() {
                entry.subject = line.trim().to_string();
            } else if !line.trim().is_empty()
                && !entry.subject.is_empty()
                && entry.body_first_line.is_none()
            {
                // Capture first body line after subject (for BREAKING CHANGE detection).
                let trimmed = line.trim();
                if trimmed.starts_with("BREAKING CHANGE") || trimmed.starts_with("BREAKING-CHANGE")
                {
                    entry.body_first_line = Some(trimmed.to_string());
                }
            }
        }
    }
    if let Some(entry) = current {
        commits.push(entry);
    }

    if level >= 3 {
        let latest = commits
            .first()
            .map(|c| c.subject.as_str())
            .unwrap_or("(none)");
        return format!(
            "[grove: git log — {} commit(s), latest: {}]\n",
            commits.len(),
            latest
        );
    }

    // Level 2: compact display, limited to max_commits
    let mut result = String::new();
    for entry in commits.iter().take(max_commits) {
        let date_part = if entry.date.is_empty() {
            String::new()
        } else {
            format!(", {}", entry.date)
        };
        result.push_str(&format!(
            "{} {} ({}{})\n",
            &entry.hash[..entry.hash.len().min(8)],
            entry.subject,
            entry.author,
            date_part,
        ));
        // Include BREAKING CHANGE body line when present.
        if let Some(ref body) = entry.body_first_line {
            result.push_str(&format!("  {}\n", body));
        }
    }
    if commits.len() > max_commits {
        result.push_str(&format!(
            "[grove: +{} more commits omitted]\n",
            commits.len() - max_commits
        ));
    }
    result
}

struct LogEntry {
    hash: String,
    subject: String,
    author: String,
    #[allow(dead_code)]
    date: String,
    /// First body line, captured only if it starts with "BREAKING CHANGE".
    body_first_line: Option<String>,
}

/// Filter status output.
///
/// Level 1-2: Passthrough.
/// Level 3: File counts only.
fn filter_status(text: &str, level: u8) -> String {
    if level < 3 {
        return text.to_string();
    }

    let mut staged = 0usize;
    let mut unstaged = 0usize;
    let mut untracked = 0usize;
    let mut branch = String::new();

    for line in text.lines() {
        if line.starts_with("## ") {
            branch = line[3..].to_string();
        } else if line.starts_with("On branch ") {
            branch = line.strip_prefix("On branch ").unwrap_or("").to_string();
        } else if line.starts_with("??") || line.contains("Untracked") {
            untracked += 1;
        } else if line.starts_with('M')
            || line.starts_with('A')
            || line.starts_with('D')
            || line.starts_with('R')
        {
            staged += 1;
            if line.len() > 1 {
                let second = line.as_bytes().get(1).copied().unwrap_or(b' ');
                if second != b' ' && second != b'?' {
                    unstaged += 1;
                }
            }
        } else if line.starts_with(" M") || line.starts_with(" D") {
            unstaged += 1;
        }
    }

    let branch_info = if branch.is_empty() {
        String::new()
    } else {
        format!(" on {}", branch)
    };

    format!(
        "[grove: git status{} — {} staged, {} unstaged, {} untracked]\n",
        branch_info, staged, unstaged, untracked
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_filter::session::FilterState;

    fn test_state() -> FilterState {
        FilterState::new("test-run".into(), vec![], 200_000)
    }

    #[test]
    fn diff_level1_passthrough() {
        let input = "diff --git a/foo.rs b/foo.rs\n--- a/foo.rs\n+++ b/foo.rs\n@@ -1,3 +1,4 @@\n+new line\n context\n";
        let state = test_state();
        let result = filter(input, 1, &state);
        assert_eq!(result, input);
    }

    #[test]
    fn diff_level3_summary_only() {
        let input = "diff --git a/foo.rs b/foo.rs\n--- a/foo.rs\n+++ b/foo.rs\n@@ -1,3 +1,4 @@\n+new line\n context\n-old\n";
        let state = test_state();
        let result = filter(input, 3, &state);
        assert!(result.contains("[grove: git diff"));
        assert!(result.contains("1 file(s)"));
        assert!(result.contains("+1"));
        assert!(result.contains("-1"));
    }

    #[test]
    fn log_level2_compact() {
        let input = "\
commit abc123def456
Author: Dev <dev@test.com>
Date:   Mon Jan 1 12:00:00 2024

    Fix the bug

commit 789abcdef012
Author: Dev <dev@test.com>
Date:   Sun Dec 31 12:00:00 2023

    Initial commit
";
        let state = test_state();
        let result = filter(input, 2, &state);
        assert!(result.contains("abc123de Fix the bug"));
        assert!(result.contains("789abcde Initial commit"));
    }

    #[test]
    fn log_level2_breaking_change() {
        let input = "\
commit abc123def456
Author: Dev <dev@test.com>
Date:   Mon Jan 1 12:00:00 2024

    refactor!: rename API

    BREAKING CHANGE: `/v1/users` endpoint removed

commit 789abcdef012
Author: Dev <dev@test.com>
Date:   Sun Dec 31 12:00:00 2023

    Initial commit
";
        let state = test_state();
        let result = filter(input, 2, &state);
        assert!(result.contains("BREAKING CHANGE"));
        assert!(result.contains("/v1/users"));
    }

    #[test]
    fn log_level3_one_liner() {
        let input = "commit abc123\nAuthor: Dev\nDate: Mon\n\n    Fix bug\n\ncommit def456\nAuthor: Dev\nDate: Sun\n\n    Init\n";
        let state = test_state();
        let result = filter(input, 3, &state);
        assert!(result.contains("2 commit(s)"));
        assert!(result.contains("Fix bug"));
    }

    #[test]
    fn status_level3_counts() {
        let input = "## main\nM  src/lib.rs\n M src/main.rs\n?? new_file.txt\n";
        let result = filter_status(input, 3);
        assert!(result.contains("[grove: git status"));
        assert!(result.contains("main"));
    }
}
