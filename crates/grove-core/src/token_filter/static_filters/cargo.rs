//! Cargo output filter — handles test, build, clippy, and check subcommands.

use super::universal::strip_ansi;

/// Filter cargo command output based on compression level.
///
/// All levels: Strip ANSI and drop "Compiling …" progress lines.
/// Level 2: Keep only FAILED tests + panic/assertion messages.
/// Level 3: Summary line only (N passed, M failed) + failure names.
pub fn filter(output: &str, level: u8) -> String {
    let cleaned = strip_ansi(output);

    // Guard against unexpected parse failures — always return usable output.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if is_test_output(&cleaned) {
            filter_test(&cleaned, level)
        } else if is_build_output(&cleaned) {
            filter_build(&cleaned, level)
        } else {
            filter_generic(&cleaned, level)
        }
    }));

    match result {
        Ok(text) if !text.is_empty() => text,
        _ => {
            // Raw fallback on empty result or panic.
            format!("{}\n[grove: raw fallback — filter error]\n", cleaned)
        }
    }
}

fn is_test_output(text: &str) -> bool {
    text.contains("running ") && text.contains(" test")
        || text.contains("test result:")
        || text.contains("failures:")
}

fn is_build_output(text: &str) -> bool {
    text.contains("Compiling ") || text.contains("Finished ") || text.contains("error[E")
}

/// Filter cargo test output.
fn filter_test(text: &str, level: u8) -> String {
    let lines: Vec<&str> = text.lines().collect();

    // Always strip compile progress lines.
    let lines: Vec<&str> = lines
        .into_iter()
        .filter(|l| !is_compile_progress(l))
        .collect();

    if level <= 1 {
        return lines.join("\n") + "\n";
    }

    let mut result = String::new();
    let mut in_failure_section = false;
    let mut failures: Vec<String> = Vec::new();
    let mut summary_line: Option<String> = None;
    let mut pass_count = 0usize;
    let mut fail_count = 0usize;

    for line in &lines {
        // Detect summary line: "test result: ok. 42 passed; 0 failed; ..."
        if line.starts_with("test result:") {
            summary_line = Some(line.to_string());
            // Parse counts
            if let Some(p) = extract_count(line, "passed") {
                pass_count += p;
            }
            if let Some(f) = extract_count(line, "failed") {
                fail_count += f;
            }
            continue;
        }

        // Detect failures section header
        if line.trim() == "failures:" || line.starts_with("---- ") {
            in_failure_section = true;
        }

        // Collect failure details
        if in_failure_section {
            if line.starts_with("test ") && line.contains("FAILED") {
                failures.push(line.to_string());
            } else if line.contains("panicked at")
                || line.contains("assertion")
                || line.contains("thread '")
                || line.starts_with("---- ")
            {
                failures.push(line.to_string());
            }
        }

        // Individual test FAILED line outside failures section
        if !in_failure_section && line.contains("FAILED") && line.starts_with("test ") {
            failures.push(line.to_string());
        }
    }

    if level >= 3 {
        // Summary only
        result.push_str(&format!(
            "[grove: cargo test — {} passed, {} failed]\n",
            pass_count, fail_count
        ));
        if !failures.is_empty() {
            result.push_str("failures:\n");
            for f in &failures {
                if f.starts_with("test ") || f.starts_with("---- ") {
                    result.push_str(f);
                    result.push('\n');
                }
            }
        }
        return result;
    }

    // Level 2: failures + summary
    if !failures.is_empty() {
        result.push_str("failures:\n");
        for f in &failures {
            result.push_str(f);
            result.push('\n');
        }
        result.push('\n');
    }
    if let Some(ref s) = summary_line {
        result.push_str(s);
        result.push('\n');
    } else {
        result.push_str(&format!(
            "test result: {} passed, {} failed\n",
            pass_count, fail_count
        ));
    }
    result
}

/// Filter cargo build/check/clippy output.
fn filter_build(text: &str, level: u8) -> String {
    let mut result = String::new();
    let mut error_count = 0usize;
    let mut warning_count = 0usize;

    for line in text.lines() {
        if is_compile_progress(line) {
            continue;
        }

        if line.starts_with("error") {
            error_count += 1;
        } else if line.starts_with("warning") && !line.starts_with("warning: build failed") {
            warning_count += 1;
        }

        if level >= 3 {
            // Only keep error and warning lines
            if line.starts_with("error") || line.starts_with("warning") || line.contains("Finished")
            {
                result.push_str(line);
                result.push('\n');
            }
        } else {
            // Level 1-2: keep everything except compile progress
            result.push_str(line);
            result.push('\n');
        }
    }

    if level >= 3 && result.is_empty() {
        return format!(
            "[grove: cargo build — {} error(s), {} warning(s)]\n",
            error_count, warning_count
        );
    }

    result
}

/// Generic filter for unknown cargo subcommands.
fn filter_generic(text: &str, _level: u8) -> String {
    text.lines()
        .filter(|l| !is_compile_progress(l))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Returns true for lines like "Compiling foo v1.0.0", "Downloading …", etc.
fn is_compile_progress(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("Compiling ")
        || trimmed.starts_with("Downloading ")
        || trimmed.starts_with("Downloaded ")
        || trimmed.starts_with("Updating ")
        || trimmed.starts_with("Locking ")
        || trimmed.starts_with("Blocking ")
        || trimmed.starts_with("Waiting ")
        || trimmed.starts_with("Fresh ")
        || trimmed.starts_with("Packaging ")
        || trimmed.starts_with("Verifying ")
        || (trimmed.starts_with("Building") && trimmed.contains("["))
}

/// Extract a count from a summary line like "42 passed".
fn extract_count(line: &str, keyword: &str) -> Option<usize> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    for (i, part) in parts.iter().enumerate() {
        if part.trim_end_matches(';').trim_end_matches(',') == keyword {
            if i > 0 {
                return parts[i - 1].parse().ok();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_compile_progress() {
        let input = "   Compiling foo v1.0.0\n   Compiling bar v2.0.0\ntest foo::test ... ok\ntest result: ok. 1 passed; 0 failed\n";
        let result = filter(input, 1);
        assert!(!result.contains("Compiling"));
        assert!(result.contains("test foo::test"));
    }

    #[test]
    fn level2_failures_only() {
        let input = "\
test foo::passing ... ok
test bar::failing ... FAILED
failures:
---- bar::failing stdout ----
thread 'bar::failing' panicked at 'assertion failed'
test result: FAILED. 1 passed; 1 failed; 0 ignored
";
        let result = filter(input, 2);
        assert!(result.contains("FAILED"));
        assert!(result.contains("panicked"));
        assert!(!result.contains("foo::passing ... ok"));
    }

    #[test]
    fn level3_summary_only() {
        let input = "test a ... ok\ntest b ... ok\ntest c ... FAILED\ntest result: FAILED. 2 passed; 1 failed; 0 ignored\n";
        let result = filter(input, 3);
        assert!(result.contains("[grove: cargo test"));
        assert!(result.contains("2 passed"));
        assert!(result.contains("1 failed"));
    }

    #[test]
    fn build_errors_kept() {
        let input =
            "   Compiling foo v1.0\nerror[E0308]: mismatched types\n  --> src/lib.rs:5:10\n";
        let result = filter(input, 1);
        assert!(!result.contains("Compiling"));
        assert!(result.contains("error[E0308]"));
    }
}
