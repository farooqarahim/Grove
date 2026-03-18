use crate::automation::{
    AutomationDef, AutomationDefaults, AutomationStep, NotificationConfig, SessionMode,
    TriggerConfig,
};
use crate::errors::{GroveError, GroveResult};
use serde::Deserialize;

/// Intermediate struct for YAML frontmatter parsing.
#[derive(Debug, Deserialize)]
struct MarkdownFrontmatter {
    name: String,
    #[serde(default)]
    description: Option<String>,
    trigger: TriggerConfig,
    #[serde(default)]
    defaults: Option<AutomationDefaults>,
    #[serde(default)]
    notifications: Option<NotificationConfig>,
    #[serde(default)]
    session_mode: Option<SessionMode>,
}

/// Parse a markdown automation file into an AutomationDef and its steps.
/// `file_stem` is used as the automation ID and `project_id` is the owning project.
pub fn parse_automation_markdown(
    content: &str,
    file_stem: &str,
    project_id: &str,
) -> GroveResult<(AutomationDef, Vec<AutomationStep>)> {
    let (frontmatter_yaml, body) = extract_frontmatter(content)?;
    let fm: MarkdownFrontmatter =
        serde_yaml::from_str(&frontmatter_yaml).map_err(|e| GroveError::ValidationError {
            field: "frontmatter".into(),
            message: format!("invalid YAML frontmatter: {e}"),
        })?;

    let now = chrono::Utc::now().to_rfc3339();

    let automation = AutomationDef {
        id: file_stem.to_string(),
        project_id: project_id.to_string(),
        name: fm.name,
        description: fm.description,
        enabled: true,
        trigger: fm.trigger,
        defaults: fm.defaults.unwrap_or_default(),
        session_mode: fm.session_mode.unwrap_or_default(),
        dedicated_conversation_id: None,
        source_path: None,
        last_triggered_at: None,
        created_at: now.clone(),
        updated_at: now.clone(),
        notifications: fm.notifications,
    };

    let steps = parse_steps(&body, file_stem, &now)?;

    Ok((automation, steps))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extract YAML frontmatter and the remaining body from markdown content.
/// Frontmatter is delimited by `---` lines at the start of the document.
fn extract_frontmatter(content: &str) -> GroveResult<(String, String)> {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return Err(GroveError::ValidationError {
            field: "frontmatter".into(),
            message: "markdown file must start with YAML frontmatter (--- delimiter)".into(),
        });
    }

    // Find the opening delimiter line end
    let after_first = match trimmed.find('\n') {
        Some(pos) => pos + 1,
        None => {
            return Err(GroveError::ValidationError {
                field: "frontmatter".into(),
                message: "no closing --- delimiter found for frontmatter".into(),
            });
        }
    };

    let rest = &trimmed[after_first..];

    // Find the closing --- delimiter
    let closing_pos = find_closing_delimiter(rest)?;
    let yaml_content = &rest[..closing_pos];
    let after_closing_line_end = rest[closing_pos..]
        .find('\n')
        .map(|p| closing_pos + p + 1)
        .unwrap_or(rest.len());
    let body = &rest[after_closing_line_end..];

    Ok((yaml_content.to_string(), body.to_string()))
}

/// Find the position of the closing `---` line in the remaining text after
/// the opening delimiter.
fn find_closing_delimiter(text: &str) -> GroveResult<usize> {
    let mut pos = 0;
    for line in text.lines() {
        if line.trim() == "---" {
            return Ok(pos);
        }
        // +1 for the newline
        pos += line.len() + 1;
    }

    Err(GroveError::ValidationError {
        field: "frontmatter".into(),
        message: "no closing --- delimiter found for frontmatter".into(),
    })
}

/// Parse the body after frontmatter into `AutomationStep` entries.
fn parse_steps(body: &str, file_stem: &str, now: &str) -> GroveResult<Vec<AutomationStep>> {
    let mut steps: Vec<AutomationStep> = Vec::new();
    let mut current_key: Option<String> = None;
    let mut meta_lines: Vec<String> = Vec::new();
    let mut objective_lines: Vec<String> = Vec::new();
    let mut in_objective = false;

    for line in body.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            // Flush previous step
            if let Some(key) = current_key.take() {
                let step = build_step(
                    file_stem,
                    &key,
                    steps.len() as i32,
                    &meta_lines,
                    &objective_lines,
                    now,
                )?;
                steps.push(step);
            }

            current_key = Some(heading.trim().to_string());
            meta_lines.clear();
            objective_lines.clear();
            in_objective = false;
        } else if current_key.is_some() {
            if in_objective {
                objective_lines.push(line.to_string());
            } else if line.trim().is_empty() {
                // Blank line transitions from metadata to objective
                in_objective = true;
            } else {
                meta_lines.push(line.to_string());
            }
        }
        // Lines before first ## heading are ignored (shouldn't be any after frontmatter)
    }

    // Flush last step
    if let Some(key) = current_key.take() {
        let step = build_step(
            file_stem,
            &key,
            steps.len() as i32,
            &meta_lines,
            &objective_lines,
            now,
        )?;
        steps.push(step);
    }

    Ok(steps)
}

/// Build a single `AutomationStep` from its parsed heading, metadata lines, and objective lines.
fn build_step(
    file_stem: &str,
    step_key: &str,
    ordinal: i32,
    meta_lines: &[String],
    objective_lines: &[String],
    now: &str,
) -> GroveResult<AutomationStep> {
    let mut depends_on: Vec<String> = Vec::new();
    let mut condition: Option<String> = None;
    let mut provider: Option<String> = None;
    let mut model: Option<String> = None;
    let mut budget_usd: Option<f64> = None;
    let mut pipeline: Option<String> = None;
    let mut permission_mode: Option<String> = None;

    for line in meta_lines {
        if let Some((key, value)) = parse_meta_line(line) {
            match key.as_str() {
                "depends_on" => {
                    depends_on = parse_inline_array(&value);
                }
                "condition" => {
                    condition = Some(value);
                }
                "provider" => {
                    provider = Some(value);
                }
                "model" => {
                    model = Some(value);
                }
                "budget_usd" => {
                    budget_usd = value.parse::<f64>().ok();
                }
                "pipeline" => {
                    pipeline = Some(value);
                }
                "permission_mode" => {
                    permission_mode = Some(value);
                }
                _ => {
                    // Unknown metadata key — ignore gracefully
                }
            }
        }
    }

    let objective = trim_objective(objective_lines);

    Ok(AutomationStep {
        id: format!("step_{}_{}", file_stem, step_key),
        automation_id: file_stem.to_string(),
        step_key: step_key.to_string(),
        ordinal,
        objective,
        depends_on,
        provider,
        model,
        budget_usd,
        pipeline,
        permission_mode,
        condition,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    })
}

/// Parse a single `key: value` metadata line. Returns `None` if the line
/// doesn't match the expected pattern.
fn parse_meta_line(line: &str) -> Option<(String, String)> {
    let colon_pos = line.find(':')?;
    let key = line[..colon_pos].trim().to_string();
    let value = line[colon_pos + 1..].trim().to_string();
    if key.is_empty() {
        return None;
    }
    Some((key, value))
}

/// Parse a YAML-style inline array like `[scan, update]` into a `Vec<String>`.
fn parse_inline_array(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(trimmed);

    if inner.trim().is_empty() {
        return Vec::new();
    }

    inner
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Join objective lines, trimming leading/trailing blank lines.
fn trim_objective(lines: &[String]) -> String {
    let joined = lines.join("\n");
    let trimmed = joined.trim();
    trimmed.to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MD: &str = r#"---
name: weekly-dep-update
description: Update dependencies every Monday at 9am
trigger:
  type: cron
  schedule: "0 0 9 * * MON *"
defaults:
  provider: claude_code
  model: claude-sonnet-4-6
  budget_usd: 5.0
  pipeline: autonomous
notifications:
  on_success:
    - type: slack
      webhook_url: "https://hooks.slack.com/..."
  on_failure:
    - type: system
---

## scan
provider: claude_code

Scan for outdated dependencies and list them
with current and latest versions.

## update
depends_on: [scan]
condition: steps.scan.state == 'completed'

Update all outdated deps, run the full test suite,
fix any breaking changes.

## pr
depends_on: [update]

Create a pull request with the dependency updates.
Include a summary of what changed.
"#;

    #[test]
    fn parse_sample_markdown() {
        let (auto, steps) =
            parse_automation_markdown(SAMPLE_MD, "weekly-dep-update", "proj-1").unwrap();

        assert_eq!(auto.id, "weekly-dep-update");
        assert_eq!(auto.name, "weekly-dep-update");
        assert_eq!(
            auto.description.as_deref(),
            Some("Update dependencies every Monday at 9am")
        );
        assert_eq!(auto.project_id, "proj-1");
        assert!(auto.enabled);

        // Trigger
        match &auto.trigger {
            TriggerConfig::Cron { schedule } => {
                assert_eq!(schedule, "0 0 9 * * MON *");
            }
            other => panic!("expected Cron trigger, got {:?}", other),
        }

        // Defaults
        assert_eq!(auto.defaults.provider.as_deref(), Some("claude_code"));
        assert_eq!(auto.defaults.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(auto.defaults.budget_usd, Some(5.0));
        assert_eq!(auto.defaults.pipeline.as_deref(), Some("autonomous"));

        // Notifications
        let notif = auto.notifications.as_ref().unwrap();
        assert_eq!(notif.on_success.len(), 1);
        assert_eq!(notif.on_failure.len(), 1);

        // Steps
        assert_eq!(steps.len(), 3);

        // Step 0: scan
        assert_eq!(steps[0].step_key, "scan");
        assert_eq!(steps[0].id, "step_weekly-dep-update_scan");
        assert_eq!(steps[0].ordinal, 0);
        assert!(steps[0].depends_on.is_empty());
        assert_eq!(steps[0].provider.as_deref(), Some("claude_code"));
        assert!(
            steps[0]
                .objective
                .contains("Scan for outdated dependencies")
        );

        // Step 1: update
        assert_eq!(steps[1].step_key, "update");
        assert_eq!(steps[1].ordinal, 1);
        assert_eq!(steps[1].depends_on, vec!["scan"]);
        assert_eq!(
            steps[1].condition.as_deref(),
            Some("steps.scan.state == 'completed'")
        );
        assert!(steps[1].objective.contains("Update all outdated deps"));

        // Step 2: pr
        assert_eq!(steps[2].step_key, "pr");
        assert_eq!(steps[2].ordinal, 2);
        assert_eq!(steps[2].depends_on, vec!["update"]);
        assert!(steps[2].objective.contains("Create a pull request"));
    }

    #[test]
    fn step_with_no_metadata() {
        let md = r#"---
name: simple
trigger:
  type: manual
---

## only-step

Just do the thing described here.
"#;

        let (auto, steps) = parse_automation_markdown(md, "simple", "proj-1").unwrap();
        assert_eq!(auto.name, "simple");
        assert_eq!(steps.len(), 1);

        let step = &steps[0];
        assert_eq!(step.step_key, "only-step");
        assert!(step.depends_on.is_empty());
        assert!(step.condition.is_none());
        assert!(step.provider.is_none());
        assert_eq!(step.objective, "Just do the thing described here.");
    }

    #[test]
    fn missing_frontmatter_returns_error() {
        let md = "## step1\n\nDo something.\n";
        let result = parse_automation_markdown(md, "no-fm", "proj-1");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("frontmatter"),
            "error should mention frontmatter: {err_msg}"
        );
    }

    #[test]
    fn empty_step_body() {
        let md = r#"---
name: empty-body
trigger:
  type: manual
---

## empty-step
"#;

        let (_auto, steps) = parse_automation_markdown(md, "empty-body", "proj-1").unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].step_key, "empty-step");
        assert!(
            steps[0].objective.is_empty(),
            "expected empty objective, got: {:?}",
            steps[0].objective
        );
    }

    #[test]
    fn parse_inline_array_variants() {
        assert_eq!(parse_inline_array("[a, b, c]"), vec!["a", "b", "c"]);
        assert_eq!(parse_inline_array("[single]"), vec!["single"]);
        assert!(parse_inline_array("[]").is_empty());
        assert_eq!(parse_inline_array("[ x , y ]"), vec!["x", "y"]);
    }

    #[test]
    fn multiple_metadata_keys_on_step() {
        let md = r#"---
name: multi-meta
trigger:
  type: manual
---

## build
provider: claude_code
model: claude-sonnet-4-6
budget_usd: 10.0
pipeline: autonomous
permission_mode: skip_all

Build the project.
"#;

        let (_auto, steps) = parse_automation_markdown(md, "multi-meta", "proj-1").unwrap();
        let step = &steps[0];
        assert_eq!(step.provider.as_deref(), Some("claude_code"));
        assert_eq!(step.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(step.budget_usd, Some(10.0));
        assert_eq!(step.pipeline.as_deref(), Some("autonomous"));
        assert_eq!(step.permission_mode.as_deref(), Some("skip_all"));
        assert_eq!(step.objective, "Build the project.");
    }

    #[test]
    fn unclosed_frontmatter_returns_error() {
        let md = "---\nname: bad\ntrigger:\n  type: manual\n";
        let result = parse_automation_markdown(md, "bad", "proj-1");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("closing"),
            "error should mention closing delimiter: {err_msg}"
        );
    }

    #[test]
    fn step_with_depends_on_multiple() {
        let md = r#"---
name: multi-dep
trigger:
  type: manual
---

## final
depends_on: [build, test, lint]

Finalize everything.
"#;

        let (_auto, steps) = parse_automation_markdown(md, "multi-dep", "proj-1").unwrap();
        assert_eq!(steps[0].depends_on, vec!["build", "test", "lint"]);
    }
}
