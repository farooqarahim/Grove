//! Go output filter — handles go test, build, and vet output.

use super::universal::strip_ansi;

/// Filter go command output based on compression level.
///
/// For `go test -json` (NDJSON), keeps only `"Action":"fail"` events.
/// For text-mode output, uses line-based heuristics.
pub fn filter(output: &str, level: u8) -> String {
    let cleaned = strip_ansi(output);

    if level <= 1 {
        return cleaned;
    }

    if is_json_test_output(&cleaned) {
        return filter_json_test(&cleaned, level);
    }
    if is_text_test_output(&cleaned) {
        return filter_text_test(&cleaned, level);
    }
    if is_build_output(&cleaned) {
        return filter_build(&cleaned, level);
    }

    cleaned
}

fn is_json_test_output(text: &str) -> bool {
    // NDJSON lines start with `{` and contain "Action"
    text.lines()
        .take(5)
        .any(|l| l.starts_with('{') && l.contains("\"Action\""))
}

fn is_text_test_output(text: &str) -> bool {
    text.contains("--- FAIL")
        || text.contains("--- PASS")
        || text.contains("ok  \t")
        || text.contains("FAIL\t")
}

fn is_build_output(text: &str) -> bool {
    text.contains("./") && text.contains(".go:") || text.contains("# ")
}

/// Filter NDJSON go test output.
fn filter_json_test(text: &str, level: u8) -> String {
    let mut pass_count = 0usize;
    let mut fail_count = 0usize;
    let mut skip_count = 0usize;
    let mut fail_output = String::new();

    for line in text.lines() {
        if !line.starts_with('{') {
            continue;
        }
        // Lightweight JSON parsing — avoid pulling in a full JSON parser just for this.
        let action = extract_json_string(line, "Action");
        let test_name = extract_json_string(line, "Test");
        let output_text = extract_json_string(line, "Output");

        match action.as_deref() {
            Some("pass") => pass_count += 1,
            Some("fail") => {
                fail_count += 1;
                if level < 3 {
                    if let Some(ref name) = test_name {
                        fail_output.push_str(&format!("FAIL: {}\n", name));
                    }
                }
            }
            Some("skip") => skip_count += 1,
            Some("output") => {
                // Keep output lines that look like error details
                if level < 3 {
                    if let Some(ref out) = output_text {
                        let trimmed = out.trim();
                        if trimmed.contains("Error")
                            || trimmed.contains("panic")
                            || trimmed.contains("FAIL")
                            || trimmed.starts_with("---")
                        {
                            fail_output.push_str(trimmed);
                            fail_output.push('\n');
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if level >= 3 {
        return format!(
            "[grove: go test — {} passed, {} failed, {} skipped]\n",
            pass_count, fail_count, skip_count
        );
    }

    let mut result = fail_output;
    result.push_str(&format!(
        "\n{} passed, {} failed, {} skipped\n",
        pass_count, fail_count, skip_count
    ));
    result
}

/// Extract a string value from a JSON object line by key (lightweight, no serde).
fn extract_json_string(line: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", key);
    let start = line.find(&pattern)?;
    let value_start = start + pattern.len();
    let rest = &line[value_start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Filter text-mode go test output.
fn filter_text_test(text: &str, level: u8) -> String {
    let mut result = String::new();
    let mut in_fail_block = false;
    let mut pass_count = 0usize;
    let mut fail_count = 0usize;

    for line in text.lines() {
        if line.starts_with("--- FAIL") {
            in_fail_block = true;
            fail_count += 1;
            if level < 3 {
                result.push_str(line);
                result.push('\n');
            }
            continue;
        }
        if line.starts_with("--- PASS") {
            in_fail_block = false;
            pass_count += 1;
            continue;
        }
        if line.starts_with("ok  \t") {
            pass_count += 1;
            continue;
        }
        if line.starts_with("FAIL\t") {
            fail_count += 1;
            if level < 3 {
                result.push_str(line);
                result.push('\n');
            }
            continue;
        }

        if in_fail_block && level < 3 {
            result.push_str(line);
            result.push('\n');
        }
    }

    if level >= 3 {
        return format!(
            "[grove: go test — {} passed, {} failed]\n",
            pass_count, fail_count
        );
    }

    if !result.is_empty() {
        result.push_str(&format!("\n{} passed, {} failed\n", pass_count, fail_count));
    }
    result
}

/// Filter go build/vet output — keep error lines, drop noise.
fn filter_build(text: &str, level: u8) -> String {
    if level >= 3 {
        let errors: Vec<&str> = text
            .lines()
            .filter(|l| l.contains(".go:") && (l.contains("error") || l.contains("undefined")))
            .collect();
        return format!(
            "[grove: go build — {} error(s)]\n{}",
            errors.len(),
            if errors.is_empty() {
                String::new()
            } else {
                errors.join("\n") + "\n"
            }
        );
    }

    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_test_level3() {
        let input = "--- PASS: TestFoo (0.01s)\n--- FAIL: TestBar (0.02s)\n    bar_test.go:10: expected 1, got 2\nFAIL\n";
        let result = filter(input, 3);
        assert!(result.contains("[grove: go test"));
        assert!(result.contains("1 passed"));
        assert!(result.contains("1 failed"));
    }

    #[test]
    fn text_test_level2_failures() {
        let input = "--- PASS: TestFoo (0.01s)\n--- FAIL: TestBar (0.02s)\n    error details\nok  \tpkg1\t0.1s\n";
        let result = filter(input, 2);
        assert!(result.contains("--- FAIL: TestBar"));
        assert!(result.contains("error details"));
        assert!(!result.contains("--- PASS"));
    }

    #[test]
    fn json_test_level3() {
        let input = r#"{"Action":"pass","Test":"TestFoo"}
{"Action":"fail","Test":"TestBar"}
{"Action":"skip","Test":"TestBaz"}
"#;
        let result = filter(input, 3);
        assert!(result.contains("1 passed"));
        assert!(result.contains("1 failed"));
        assert!(result.contains("1 skipped"));
    }
}
