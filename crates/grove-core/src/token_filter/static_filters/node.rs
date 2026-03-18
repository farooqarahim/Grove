//! Node.js ecosystem output filter — tsc, eslint, vitest, jest, next build, etc.

use super::universal::strip_ansi;

/// Filter Node.js toolchain output based on compression level.
pub fn filter(output: &str, level: u8) -> String {
    let cleaned = strip_ansi(output);

    if level <= 1 {
        return cleaned;
    }

    if is_tsc_output(&cleaned) {
        return filter_tsc(&cleaned, level);
    }
    if is_test_runner_output(&cleaned) {
        return filter_test_runner(&cleaned, level);
    }
    if is_eslint_output(&cleaned) {
        return filter_eslint(&cleaned, level);
    }

    // Generic node output — just return cleaned
    cleaned
}

fn is_tsc_output(text: &str) -> bool {
    text.contains(": error TS") || text.contains(": warning TS") || text.contains("Found 0 errors")
}

fn is_test_runner_output(text: &str) -> bool {
    text.contains("Tests ") && (text.contains("passed") || text.contains("failed"))
        || text.contains("Test Suites:")
        || text.contains("✓") && text.contains("ms")
        || text.contains("PASS")
        || text.contains("FAIL")
}

fn is_eslint_output(text: &str) -> bool {
    text.contains("problems (")
        || text.contains("warning  ")
        || text
            .lines()
            .any(|l| l.contains("  error  ") || l.contains("  warning  "))
}

/// Filter TypeScript compiler output.
///
/// Level 2: Group errors by file + error code, deduplicate.
/// Level 3: Count only.
fn filter_tsc(text: &str, level: u8) -> String {
    let mut errors: Vec<&str> = Vec::new();
    let mut file_set: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in text.lines() {
        if line.contains(": error TS") || line.contains(": warning TS") {
            errors.push(line);
            // Extract file path
            if let Some(file) = line.split('(').next() {
                file_set.insert(file.to_string());
            } else if let Some(file) = line.split(':').next() {
                file_set.insert(file.to_string());
            }
        }
    }

    if level >= 3 {
        return format!(
            "[grove: tsc — {} error(s) in {} file(s)]\n",
            errors.len(),
            file_set.len()
        );
    }

    // Level 2: show errors grouped (no duplicate-heavy repetition)
    let mut result = String::new();
    for err in &errors {
        result.push_str(err);
        result.push('\n');
    }

    // Keep "Found N errors" summary if present
    for line in text.lines() {
        if line.starts_with("Found ") && line.contains(" error") {
            result.push_str(line);
            result.push('\n');
            break;
        }
    }

    result
}

/// Filter test runner output (vitest, jest).
///
/// Level 2: Failures only.
/// Level 3: Summary line only.
fn filter_test_runner(text: &str, level: u8) -> String {
    let mut result = String::new();
    let mut in_failure = false;
    let mut summary_lines: Vec<String> = Vec::new();

    for line in text.lines() {
        // Jest/vitest summary lines
        if line.starts_with("Test Suites:")
            || line.starts_with("Tests:")
            || line.starts_with("Snapshots:")
            || line.starts_with("Time:")
            || (line.contains("Tests ") && (line.contains("passed") || line.contains("failed")))
        {
            summary_lines.push(line.to_string());
            continue;
        }

        // FAIL marker
        if line.starts_with("FAIL ") || line.contains("FAIL ") {
            in_failure = true;
            if level < 3 {
                result.push_str(line);
                result.push('\n');
            }
            continue;
        }

        // PASS marker — end failure block
        if line.starts_with("PASS ") || line.contains("PASS ") {
            in_failure = false;
            continue;
        }

        // In failure block, keep details
        if in_failure && level < 3 {
            result.push_str(line);
            result.push('\n');
        }
    }

    if level >= 3 {
        if summary_lines.is_empty() {
            return "[grove: test runner — no summary found]\n".to_string();
        }
        return summary_lines.join("\n") + "\n";
    }

    // Append summary
    for s in &summary_lines {
        result.push_str(s);
        result.push('\n');
    }

    result
}

/// Filter ESLint output.
fn filter_eslint(text: &str, level: u8) -> String {
    if level >= 3 {
        // Find summary line
        for line in text.lines().rev() {
            if line.contains("problems (") || line.contains("problem (") {
                return format!("{}\n", line.trim());
            }
        }
        let error_count = text
            .lines()
            .filter(|l| l.contains("  error  ") || l.contains("  warning  "))
            .count();
        return format!("[grove: eslint — {} issue(s)]\n", error_count);
    }

    // Level 2: keep as-is (already compact enough after ANSI strip)
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsc_level3_summary() {
        let input = "\
src/foo.ts(10,5): error TS2304: Cannot find name 'x'.
src/foo.ts(15,3): error TS2304: Cannot find name 'y'.
src/bar.ts(1,1): error TS1005: ';' expected.
Found 3 errors in 2 files.
";
        let result = filter(input, 3);
        assert!(result.contains("[grove: tsc"));
        assert!(result.contains("3 error(s)"));
        assert!(result.contains("2 file(s)"));
    }

    #[test]
    fn jest_level2_failures_only() {
        let input = "\
PASS src/utils.test.ts
FAIL src/api.test.ts
  ● GET /health returns 200
    Expected: 200
    Received: 500
Test Suites: 1 failed, 1 passed, 2 total
Tests: 1 failed, 5 passed, 6 total
";
        let result = filter(input, 2);
        assert!(result.contains("FAIL src/api.test.ts"));
        assert!(result.contains("Expected: 200"));
        assert!(!result.contains("PASS src/utils"));
        assert!(result.contains("Test Suites:"));
    }

    #[test]
    fn jest_level3_summary() {
        let input = "\
PASS src/a.test.ts
PASS src/b.test.ts
Test Suites: 2 passed, 2 total
Tests: 10 passed, 10 total
Time: 3.5s
";
        let result = filter(input, 3);
        assert!(result.contains("Test Suites:"));
        assert!(result.contains("Tests:"));
        assert!(!result.contains("PASS"));
    }
}
