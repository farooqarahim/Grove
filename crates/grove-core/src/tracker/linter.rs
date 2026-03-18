use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::LintCommandConfig;
use crate::errors::{GroveError, GroveResult};

/// Severity level for a lint issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LintSeverity {
    Error,
    Warning,
    Info,
}

impl LintSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            LintSeverity::Error => "error",
            LintSeverity::Warning => "warning",
            LintSeverity::Info => "info",
        }
    }
}

/// A single issue found by a linter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintIssue {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub severity: LintSeverity,
    pub message: String,
    pub rule: Option<String>,
}

impl LintIssue {
    /// Convert a lint issue to a tracker Issue for consistent handling.
    pub fn to_tracker_issue(&self, linter: &str) -> super::Issue {
        let rule_suffix = self
            .rule
            .as_deref()
            .map(|r| format!(" [{r}]"))
            .unwrap_or_default();
        super::Issue {
            external_id: format!("{linter}:{file}:{line}", file = self.file, line = self.line),
            provider: "linter".to_string(),
            title: format!(
                "{}: {}:{}:{} {}{}",
                linter, self.file, self.line, self.column, self.message, rule_suffix,
            ),
            status: self.severity.as_str().to_string(),
            labels: vec![linter.to_string(), self.severity.as_str().to_string()],
            body: Some(format!(
                "Lint {severity} in `{file}` at line {line}, column {col}:\n\n{msg}{rule}",
                severity = self.severity.as_str(),
                file = self.file,
                line = self.line,
                col = self.column,
                msg = self.message,
                rule = rule_suffix,
            )),
            url: None,
            assignee: None,
            raw_json: serde_json::json!({}),
            provider_native_id: None,
            provider_scope_type: None,
            provider_scope_key: None,
            provider_scope_name: None,
            provider_metadata: serde_json::json!({}),
            id: None,
            project_id: None,
            canonical_status: None,
            priority: None,
            is_native: false,
            created_at: None,
            updated_at: None,
            synced_at: None,
            run_id: None,
        }
    }
}

/// Result of running a single linter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintResult {
    pub linter: String,
    pub issues: Vec<LintIssue>,
    pub passed: bool,
}

/// Which output format a linter produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintParser {
    /// ESLint JSON, clippy JSON, ruff JSON
    Json,
    /// Standard `file:line:col: severity: message` format
    Line,
    /// SARIF format (GitHub CodeQL, etc.)
    Sarif,
}

impl LintParser {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => Self::Json,
            "sarif" => Self::Sarif,
            _ => Self::Line,
        }
    }
}

/// Run a linter command and parse its output.
pub fn run_linter(config: &LintCommandConfig, workdir: &Path) -> GroveResult<LintResult> {
    let parts: Vec<&str> = config.command.split_whitespace().collect();
    if parts.is_empty() {
        return Err(GroveError::Runtime(format!(
            "linter '{}': empty command string",
            config.name
        )));
    }

    let output = Command::new(parts[0])
        .args(&parts[1..])
        .current_dir(workdir)
        .env("PATH", crate::capability::shell_path())
        .output()
        .map_err(|e| {
            GroveError::Runtime(format!("linter '{}' failed to start: {e}", config.name))
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stdout.is_empty() {
        stderr.to_string()
    } else {
        stdout.to_string()
    };

    let parser = LintParser::parse(&config.parser);
    let issues = parse_lint_output(parser, &combined);
    let passed =
        output.status.success() && issues.iter().all(|i| i.severity != LintSeverity::Error);

    Ok(LintResult {
        linter: config.name.clone(),
        issues,
        passed,
    })
}

/// Parse linter output according to the specified format.
pub fn parse_lint_output(parser: LintParser, output: &str) -> Vec<LintIssue> {
    match parser {
        LintParser::Json => parse_json_output(output),
        LintParser::Line => parse_line_output(output),
        LintParser::Sarif => parse_sarif_output(output),
    }
}

/// Build a single enriched objective from lint results for an agent to fix.
pub fn lint_issues_to_objective(results: &[LintResult]) -> String {
    let mut lines = Vec::new();
    lines.push("Fix the following lint issues:\n".to_string());

    for result in results {
        if result.issues.is_empty() {
            continue;
        }
        lines.push(format!(
            "## {} ({} issue(s))\n",
            result.linter,
            result.issues.len()
        ));
        for issue in &result.issues {
            let rule = issue
                .rule
                .as_deref()
                .map(|r| format!(" [{r}]"))
                .unwrap_or_default();
            lines.push(format!(
                "- `{}:{}:{}` [{}] {}{rule}",
                issue.file,
                issue.line,
                issue.column,
                issue.severity.as_str(),
                issue.message,
            ));
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

// ── JSON parser (ESLint / clippy / ruff) ──────────────────────────────────

fn parse_json_output(output: &str) -> Vec<LintIssue> {
    // Try ESLint format: array of { filePath, messages: [{ line, column, severity, message, ruleId }] }
    if let Ok(arr) = serde_json::from_str::<Vec<Value>>(output) {
        let mut issues = Vec::new();
        for entry in &arr {
            // ESLint format
            if let Some(file_path) = entry.get("filePath").and_then(|v| v.as_str()) {
                if let Some(messages) = entry.get("messages").and_then(|v| v.as_array()) {
                    for msg in messages {
                        issues.push(LintIssue {
                            file: file_path.to_string(),
                            line: msg.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                            column: msg.get("column").and_then(|v| v.as_u64()).unwrap_or(0)
                                as usize,
                            severity: match msg.get("severity").and_then(|v| v.as_u64()) {
                                Some(2) => LintSeverity::Error,
                                Some(1) => LintSeverity::Warning,
                                _ => LintSeverity::Info,
                            },
                            message: msg
                                .get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            rule: msg
                                .get("ruleId")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                        });
                    }
                }
            }
            // Ruff format: { filename, message, code, location: { row, column } }
            else if let Some(filename) = entry.get("filename").and_then(|v| v.as_str()) {
                let loc = entry.get("location");
                issues.push(LintIssue {
                    file: filename.to_string(),
                    line: loc
                        .and_then(|l| l.get("row"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize,
                    column: loc
                        .and_then(|l| l.get("column"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize,
                    severity: LintSeverity::Warning,
                    message: entry
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    rule: entry
                        .get("code")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
        if !issues.is_empty() {
            return issues;
        }
    }

    // Try clippy/cargo JSON format: NDJSON with { reason: "compiler-message", message: { ... } }
    let mut issues = Vec::new();
    for line in output.lines() {
        if let Ok(val) = serde_json::from_str::<Value>(line) {
            if val.get("reason").and_then(|v| v.as_str()) == Some("compiler-message") {
                if let Some(msg) = val.get("message") {
                    let level = msg.get("level").and_then(|v| v.as_str()).unwrap_or("");
                    let text = msg
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let span = msg
                        .get("spans")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.first());

                    let file = span
                        .and_then(|s| s.get("file_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let line_num = span
                        .and_then(|s| s.get("line_start"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    let col = span
                        .and_then(|s| s.get("column_start"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;

                    if !file.is_empty() {
                        issues.push(LintIssue {
                            file,
                            line: line_num,
                            column: col,
                            severity: match level {
                                "error" => LintSeverity::Error,
                                "warning" => LintSeverity::Warning,
                                _ => LintSeverity::Info,
                            },
                            message: text,
                            rule: msg
                                .get("code")
                                .and_then(|v| v.get("code"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                        });
                    }
                }
            }
        }
    }
    issues
}

// ── Line parser (file:line:col: severity: message) ────────────────────────

fn parse_line_output(output: &str) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Try: file:line:col: severity: message
        let parts: Vec<&str> = trimmed.splitn(4, ':').collect();
        if parts.len() >= 4 {
            let file = parts[0].trim().to_string();
            let line_num = parts[1].trim().parse::<usize>().unwrap_or(0);
            let col = parts[2].trim().parse::<usize>().unwrap_or(0);
            let rest = parts[3].trim();

            let (severity, message) = if let Some(msg) = rest.strip_prefix("error:") {
                (LintSeverity::Error, msg.trim())
            } else if let Some(msg) = rest.strip_prefix("warning:") {
                (LintSeverity::Warning, msg.trim())
            } else if let Some(msg) = rest.strip_prefix("info:") {
                (LintSeverity::Info, msg.trim())
            } else {
                (LintSeverity::Warning, rest)
            };

            if line_num > 0 && !file.is_empty() {
                // Extract rule ID if present in brackets at end: "message [rule-id]"
                let (msg, rule) = if let Some(bracket_start) = message.rfind('[') {
                    if message.ends_with(']') {
                        let rule = &message[bracket_start + 1..message.len() - 1];
                        (message[..bracket_start].trim_end(), Some(rule.to_string()))
                    } else {
                        (message, None)
                    }
                } else {
                    (message, None)
                };

                issues.push(LintIssue {
                    file,
                    line: line_num,
                    column: col,
                    severity,
                    message: msg.to_string(),
                    rule,
                });
            }
        }
    }
    issues
}

// ── SARIF parser ──────────────────────────────────────────────────────────

fn parse_sarif_output(output: &str) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    let Ok(sarif) = serde_json::from_str::<Value>(output) else {
        return issues;
    };

    let runs = sarif.get("runs").and_then(|v| v.as_array());
    let Some(runs) = runs else { return issues };

    for run in runs {
        let tool_name = run
            .get("tool")
            .and_then(|t| t.get("driver"))
            .and_then(|d| d.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let results = run.get("results").and_then(|v| v.as_array());
        let Some(results) = results else { continue };

        for result in results {
            let message = result
                .get("message")
                .and_then(|m| m.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let rule_id = result
                .get("ruleId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let severity = match result.get("level").and_then(|v| v.as_str()) {
                Some("error") => LintSeverity::Error,
                Some("warning") => LintSeverity::Warning,
                _ => LintSeverity::Info,
            };

            let location = result
                .get("locations")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first());

            let phys = location.and_then(|l| l.get("physicalLocation"));

            let file = phys
                .and_then(|p| p.get("artifactLocation"))
                .and_then(|a| a.get("uri"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let region = phys.and_then(|p| p.get("region"));
            let line_num = region
                .and_then(|r| r.get("startLine"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let col = region
                .and_then(|r| r.get("startColumn"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            if !file.is_empty() {
                issues.push(LintIssue {
                    file,
                    line: line_num,
                    column: col,
                    severity,
                    message: format!("[{tool_name}] {message}"),
                    rule: rule_id,
                });
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_eslint_json() {
        let json = r#"[{
            "filePath": "/src/App.tsx",
            "messages": [
                {"line": 10, "column": 5, "severity": 2, "message": "Unexpected console statement", "ruleId": "no-console"},
                {"line": 20, "column": 1, "severity": 1, "message": "Missing return type", "ruleId": "explicit-return"}
            ]
        }]"#;
        let issues = parse_json_output(json);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].file, "/src/App.tsx");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert_eq!(issues[0].rule.as_deref(), Some("no-console"));
        assert_eq!(issues[1].severity, LintSeverity::Warning);
    }

    #[test]
    fn parse_ruff_json() {
        let json = r#"[{
            "filename": "src/main.py",
            "message": "Unused import",
            "code": "F401",
            "location": {"row": 3, "column": 1}
        }]"#;
        let issues = parse_json_output(json);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file, "src/main.py");
        assert_eq!(issues[0].line, 3);
        assert_eq!(issues[0].rule.as_deref(), Some("F401"));
    }

    #[test]
    fn parse_clippy_ndjson() {
        let ndjson = r#"{"reason":"compiler-message","message":{"level":"warning","message":"unused variable: `x`","code":{"code":"unused_variables"},"spans":[{"file_name":"src/main.rs","line_start":5,"column_start":9}]}}"#;
        let issues = parse_json_output(ndjson);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file, "src/main.rs");
        assert_eq!(issues[0].line, 5);
        assert_eq!(issues[0].severity, LintSeverity::Warning);
        assert_eq!(issues[0].rule.as_deref(), Some("unused_variables"));
    }

    #[test]
    fn parse_line_format() {
        let output = "src/foo.rs:10:5: error: missing semicolon [E0001]\nsrc/bar.rs:20:1: warning: unused variable\n";
        let issues = parse_line_output(output);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].file, "src/foo.rs");
        assert_eq!(issues[0].line, 10);
        assert_eq!(issues[0].column, 5);
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert_eq!(issues[0].rule.as_deref(), Some("E0001"));
        assert_eq!(issues[1].severity, LintSeverity::Warning);
        assert!(issues[1].rule.is_none());
    }

    #[test]
    fn parse_sarif_format() {
        let sarif = r#"{
            "runs": [{
                "tool": {"driver": {"name": "CodeQL"}},
                "results": [{
                    "ruleId": "js/sql-injection",
                    "level": "error",
                    "message": {"text": "SQL injection vulnerability"},
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": {"uri": "src/db.js"},
                            "region": {"startLine": 42, "startColumn": 10}
                        }
                    }]
                }]
            }]
        }"#;
        let issues = parse_sarif_output(sarif);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file, "src/db.js");
        assert_eq!(issues[0].line, 42);
        assert_eq!(issues[0].severity, LintSeverity::Error);
        assert_eq!(issues[0].rule.as_deref(), Some("js/sql-injection"));
        assert!(issues[0].message.contains("CodeQL"));
    }

    #[test]
    fn lint_issues_to_objective_formats_correctly() {
        let results = vec![LintResult {
            linter: "clippy".into(),
            issues: vec![LintIssue {
                file: "src/main.rs".into(),
                line: 5,
                column: 1,
                severity: LintSeverity::Warning,
                message: "unused variable".into(),
                rule: Some("W0001".into()),
            }],
            passed: false,
        }];
        let objective = lint_issues_to_objective(&results);
        assert!(objective.contains("Fix the following lint issues"));
        assert!(objective.contains("clippy"));
        assert!(objective.contains("src/main.rs:5:1"));
        assert!(objective.contains("[W0001]"));
    }

    #[test]
    fn lint_issue_to_tracker_issue() {
        let issue = LintIssue {
            file: "src/app.ts".into(),
            line: 10,
            column: 5,
            severity: LintSeverity::Error,
            message: "Missing type annotation".into(),
            rule: Some("TS7006".into()),
        };
        let tracker_issue = issue.to_tracker_issue("eslint");
        assert_eq!(tracker_issue.provider, "linter");
        assert!(tracker_issue.title.contains("eslint"));
        assert!(tracker_issue.title.contains("[TS7006]"));
        assert_eq!(tracker_issue.labels, vec!["eslint", "error"]);
    }
}
