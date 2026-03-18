use anyhow::Result;
use grove_core::config::GroveConfig;
use grove_core::db;
use grove_core::orchestrator::{self, RunOptions};
use grove_core::tracker::linter::{LintResult, lint_issues_to_objective, run_linter};
use serde_json::json;

use crate::cli::LintArgs;
use crate::command_context::CommandContext;
use crate::commands::queue::provider_from_config;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &LintArgs) -> Result<CommandOutput> {
    let cfg = GroveConfig::load_or_create(&ctx.project_root)?;

    if !cfg.linter.enabled {
        return Ok(CommandOutput {
            text: "Linter integration is disabled. Enable it in grove.yaml:\n\n  linter:\n    enabled: true\n    commands:\n      - name: clippy\n        command: \"cargo clippy --message-format=json\"\n        parser: json".into(),
            json: json!({ "enabled": false }),
        });
    }

    if cfg.linter.commands.is_empty() {
        return Ok(CommandOutput {
            text: "No linter commands configured. Add commands to linter.commands in grove.yaml."
                .into(),
            json: json!({ "enabled": true, "commands": 0 }),
        });
    }

    let mut results: Vec<LintResult> = Vec::new();
    let mut all_passed = true;

    for cmd_cfg in &cfg.linter.commands {
        eprintln!("[lint] Running {}…", cmd_cfg.name);
        match run_linter(cmd_cfg, &ctx.project_root) {
            Ok(result) => {
                if !result.passed {
                    all_passed = false;
                }
                eprintln!(
                    "[lint] {} — {} issue(s), {}",
                    cmd_cfg.name,
                    result.issues.len(),
                    if result.passed { "passed" } else { "failed" },
                );
                results.push(result);
            }
            Err(e) => {
                eprintln!("[lint] {} — error: {e}", cmd_cfg.name);
                all_passed = false;
            }
        }
    }

    let total_issues: usize = results.iter().map(|r| r.issues.len()).sum();

    if args.fix && !all_passed && total_issues > 0 {
        eprintln!("\n[lint] Spawning agent to fix {total_issues} lint issue(s)…");
        db::initialize(&ctx.project_root)?;

        let objective = lint_issues_to_objective(&results);
        let provider = provider_from_config(&cfg, &ctx.project_root, None, None)?;
        let run_result = orchestrator::execute_objective(
            &ctx.project_root,
            &cfg,
            &objective,
            RunOptions {
                budget_usd: args.budget_usd,
                max_agents: None,
                model: args.model.clone(),
                provider: None,
                interactive: false,
                pause_after: vec![],
                permission_mode: None,
                pipeline: None,
                conversation_id: None,
                continue_last: false,
                db_path: None,
                abort_handle: None,
                issue_id: None,
                issue: None,
                resume_provider_session_id: None,
                on_run_created: None,
                input_handle_callback: None,
                run_control_callback: None,
                disable_phase_gates: false,
            },
            provider,
        )?;

        let json_val = json!({
            "total_issues": total_issues,
            "passed": all_passed,
            "fix_run_id": run_result.run_id,
            "fix_state": run_result.state,
            "results": results.iter().map(|r| json!({
                "linter": r.linter,
                "issues": r.issues.len(),
                "passed": r.passed,
            })).collect::<Vec<_>>(),
        });

        let text = format!(
            "Lint: {total_issues} issue(s) found\nFix run: {} ({})",
            run_result.run_id, run_result.state,
        );
        return Ok(to_text_or_json(ctx.format, text, json_val));
    }

    let json_val = json!({
        "total_issues": total_issues,
        "passed": all_passed,
        "results": results.iter().map(|r| json!({
            "linter": r.linter,
            "issues": r.issues.len(),
            "passed": r.passed,
        })).collect::<Vec<_>>(),
    });

    let mut text = String::new();
    for result in &results {
        text.push_str(&format!(
            "{}: {} issue(s) — {}\n",
            result.linter,
            result.issues.len(),
            if result.passed { "PASS" } else { "FAIL" },
        ));
        for issue in &result.issues {
            let rule = issue
                .rule
                .as_deref()
                .map(|r| format!(" [{r}]"))
                .unwrap_or_default();
            text.push_str(&format!(
                "  {}:{}:{} [{}] {}{rule}\n",
                issue.file,
                issue.line,
                issue.column,
                issue.severity.as_str(),
                issue.message,
            ));
        }
    }
    if all_passed {
        text.push_str("\nAll linters passed.");
    } else {
        text.push_str(&format!(
            "\n{total_issues} issue(s) found. Use --fix to auto-fix."
        ));
    }

    Ok(to_text_or_json(ctx.format, text, json_val))
}
