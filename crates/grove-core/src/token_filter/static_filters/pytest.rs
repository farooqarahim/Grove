//! Python test output filter — handles pytest, ruff, and mypy output.

use super::universal::strip_ansi;

/// Filter Python test/lint output based on compression level.
///
/// Uses a state machine to parse pytest output sections:
/// Header → Progress → Failures → Summary.
///
/// Level 1: ANSI strip only.
/// Level 2: Failure sections + summary line.
/// Level 3: Summary line only.
pub fn filter(output: &str, level: u8) -> String {
    let cleaned = strip_ansi(output);

    if level <= 1 {
        return cleaned;
    }

    if is_ruff_output(&cleaned) {
        return filter_ruff(&cleaned, level);
    }
    if is_mypy_output(&cleaned) {
        return filter_mypy(&cleaned, level);
    }

    filter_pytest(&cleaned, level)
}

fn is_ruff_output(text: &str) -> bool {
    text.contains("Found ") && text.contains(" error")
        || text.lines().any(|l| {
            // ruff format: file.py:line:col: EXXXX message
            l.contains(".py:")
                && l.split_whitespace()
                    .any(|w| w.starts_with('E') || w.starts_with('W'))
        })
}

fn is_mypy_output(text: &str) -> bool {
    text.contains("error:") && text.contains(".py:")
        || text.contains("Found ") && text.contains(" error")
}

#[derive(PartialEq)]
enum PytestSection {
    Header,
    Progress,
    Failures,
    Summary,
}

fn filter_pytest(text: &str, level: u8) -> String {
    let mut result = String::new();
    let mut section = PytestSection::Header;
    let mut summary_line: Option<String> = None;
    let mut in_failure_block = false;

    for line in text.lines() {
        // Detect section transitions
        if line.starts_with("=") && line.contains("FAILURES") {
            section = PytestSection::Failures;
            in_failure_block = true;
            if level < 3 {
                result.push_str(line);
                result.push('\n');
            }
            continue;
        }
        if line.starts_with("=")
            && (line.contains("passed") || line.contains("failed") || line.contains("error"))
        {
            section = PytestSection::Summary;
            summary_line = Some(line.to_string());
            continue;
        }
        if line.starts_with("=") && line.contains("short test summary") {
            section = PytestSection::Summary;
            if level < 3 {
                result.push_str(line);
                result.push('\n');
            }
            continue;
        }

        match section {
            PytestSection::Header => {
                // Skip platform/plugin header noise at level 2+
                if level >= 2
                    && (line.starts_with("platform ")
                        || line.starts_with("plugins:")
                        || line.starts_with("cachedir:")
                        || line.starts_with("rootdir:")
                        || line.starts_with("configfile:")
                        || line.starts_with("collected "))
                {
                    continue;
                }
            }
            PytestSection::Progress => {
                // Skip progress dots/percentages
                if level >= 2 {
                    continue;
                }
            }
            PytestSection::Failures => {
                // Keep failure details at level 2
                if level < 3 {
                    // Failure header (underscores) or content
                    if line.starts_with("___") || line.starts_with("---") {
                        in_failure_block = true;
                        result.push_str(line);
                        result.push('\n');
                        continue;
                    }
                    if in_failure_block {
                        result.push_str(line);
                        result.push('\n');
                    }
                }
                continue;
            }
            PytestSection::Summary => {
                if level < 3 && line.starts_with("FAILED ") {
                    result.push_str(line);
                    result.push('\n');
                }
                continue;
            }
        }

        // Detect progress section (lines that are mostly dots or percentages)
        if is_progress_line(line) {
            section = PytestSection::Progress;
            if level < 2 {
                result.push_str(line);
                result.push('\n');
            }
            continue;
        }

        if level < 3 {
            result.push_str(line);
            result.push('\n');
        }
    }

    // Always include summary
    if let Some(ref s) = summary_line {
        result.push_str(s);
        result.push('\n');
    }

    if level >= 3 {
        // Return just the summary
        return summary_line.unwrap_or_else(|| "[grove: pytest — no summary found]\n".to_string());
    }

    result
}

fn is_progress_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    // pytest progress: mostly dots, F, E, s, x characters followed by [XX%]
    let non_whitespace: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    non_whitespace.chars().all(|c| {
        matches!(
            c,
            '.' | 'F' | 'E' | 's' | 'x' | 'X' | '[' | ']' | '%' | '0'..='9'
        )
    }) && non_whitespace.len() > 5
}

fn filter_ruff(text: &str, level: u8) -> String {
    if level >= 3 {
        // Count errors
        let error_count = text.lines().filter(|l| l.contains(".py:")).count();
        return format!("[grove: ruff — {} issue(s)]\n", error_count);
    }

    // Level 2: group by file, keep first occurrence per rule
    text.to_string()
}

fn filter_mypy(text: &str, level: u8) -> String {
    if level >= 3 {
        // Find the summary line
        for line in text.lines().rev() {
            if line.starts_with("Found ") && line.contains(" error") {
                return format!("{}\n", line);
            }
        }
        let error_count = text.lines().filter(|l| l.contains("error:")).count();
        return format!("[grove: mypy — {} error(s)]\n", error_count);
    }

    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pytest_level1_passthrough() {
        let input = "\x1b[32mplatform linux\x1b[0m\ncollected 5 items\n...F.\n";
        let result = filter(input, 1);
        assert!(!result.contains("\x1b"));
        assert!(result.contains("collected 5 items"));
    }

    #[test]
    fn pytest_level3_summary_only() {
        let input = "\
platform linux
collected 10 items
..........
==================== 10 passed in 2.5s ====================
";
        let result = filter(input, 3);
        assert!(result.contains("10 passed"));
        assert!(!result.contains("platform"));
        assert!(!result.contains("collected"));
    }

    #[test]
    fn ruff_level3_count() {
        let input = "src/foo.py:10:1: E302 expected 2 blank lines\nsrc/bar.py:5:1: W291 trailing whitespace\n";
        let result = filter(input, 3);
        assert!(result.contains("[grove: ruff"));
        assert!(result.contains("2 issue(s)"));
    }
}
