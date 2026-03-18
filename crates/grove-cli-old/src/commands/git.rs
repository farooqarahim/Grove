use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde_json::json;

use crate::cli::GitArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &GitArgs) -> Result<CommandOutput> {
    let cwd = resolve_cwd(ctx, args.run_id.as_deref())?;

    match &args.action {
        crate::cli::GitAction::Status => handle_status(ctx, &cwd),
        crate::cli::GitAction::Stage(a) => handle_stage(ctx, &cwd, a),
        crate::cli::GitAction::Unstage(a) => handle_unstage(ctx, &cwd, a),
        crate::cli::GitAction::Revert(a) => handle_revert(ctx, &cwd, a),
        crate::cli::GitAction::Commit(a) => handle_commit(ctx, &cwd, a),
        crate::cli::GitAction::Push => handle_push(ctx, &cwd),
        crate::cli::GitAction::Pull => handle_pull(ctx, &cwd),
        crate::cli::GitAction::Branch => handle_branch(ctx, &cwd),
        crate::cli::GitAction::Log(a) => handle_log(ctx, &cwd, a),
        crate::cli::GitAction::Undo => handle_undo(ctx, &cwd),
        crate::cli::GitAction::Pr(a) => handle_pr(ctx, &cwd, a),
        crate::cli::GitAction::PrStatus => handle_pr_status(ctx, &cwd),
        crate::cli::GitAction::Merge(a) => handle_merge(ctx, &cwd, a),
    }
}

/// Resolve the working directory: explicit run_id → latest run worktree → project root.
fn resolve_cwd(ctx: &CommandContext, run_id: Option<&str>) -> Result<PathBuf> {
    if let Some(rid) = run_id {
        let short = &rid[..8.min(rid.len())];
        let wt = ctx
            .project_root
            .join(".grove")
            .join("worktrees")
            .join(format!("run_{short}"));
        if wt.is_dir() {
            return Ok(wt);
        }
        bail!("Worktree for run {rid} not found at {}", wt.display());
    }

    // Try to find the most recent run worktree
    if let Ok(runs) = grove_core::orchestrator::list_runs(&ctx.project_root, 1) {
        if let Some(run) = runs.first() {
            let short = &run.id[..8.min(run.id.len())];
            let wt = ctx
                .project_root
                .join(".grove")
                .join("worktrees")
                .join(format!("run_{short}"));
            if wt.is_dir() {
                return Ok(wt);
            }
        }
    }

    // Fallback to project root
    Ok(ctx.project_root.clone())
}

fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            stderr.trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_may_fail(cwd: &Path, args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
}

// ── Status ───────────────────────────────────────────────────────────────────

fn handle_status(ctx: &CommandContext, cwd: &Path) -> Result<CommandOutput> {
    let raw = git(cwd, &["status", "--porcelain=v1", "--branch"])?;

    // Numstat for line counts
    let staged_stats = parse_numstat(git_may_fail(cwd, &["diff", "--numstat", "--cached"]));
    let unstaged_stats = parse_numstat(git_may_fail(cwd, &["diff", "--numstat"]));

    let mut entries = Vec::new();
    let mut branch = String::new();

    for line in raw.lines() {
        if line.starts_with("##") {
            branch = line[3..].split("...").next().unwrap_or("").to_string();
            continue;
        }
        if line.len() < 4 {
            continue;
        }

        let idx = line.as_bytes()[0] as char;
        let wt = line.as_bytes()[1] as char;
        let path = line[3..].trim();

        if idx != ' ' && idx != '?' {
            let (a, d) = staged_stats.get(path).copied().unwrap_or((0, 0));
            entries.push(json!({
                "path": path, "area": "staged", "status": idx.to_string(),
                "additions": a, "deletions": d,
            }));
        }
        if wt != ' ' {
            let (a, d) = unstaged_stats.get(path).copied().unwrap_or((0, 0));
            entries.push(json!({
                "path": path, "area": "unstaged",
                "status": if wt == '?' { "?".to_string() } else { wt.to_string() },
                "additions": a, "deletions": d,
            }));
        }
    }

    let staged_count = entries.iter().filter(|e| e["area"] == "staged").count();
    let unstaged_count = entries.iter().filter(|e| e["area"] == "unstaged").count();

    let json_val = json!({ "branch": branch, "staged": staged_count, "unstaged": unstaged_count, "files": entries });

    let mut text = format!("On branch {branch}\n");
    if staged_count > 0 {
        text.push_str(&format!("\nStaged ({staged_count}):\n"));
        for e in entries.iter().filter(|e| e["area"] == "staged") {
            let adds = e["additions"].as_i64().unwrap_or(0);
            let dels = e["deletions"].as_i64().unwrap_or(0);
            text.push_str(&format!(
                "  {} {}  +{adds} -{dels}\n",
                e["status"].as_str().unwrap_or("?"),
                e["path"].as_str().unwrap_or(""),
            ));
        }
    }
    if unstaged_count > 0 {
        text.push_str(&format!("\nUnstaged ({unstaged_count}):\n"));
        for e in entries.iter().filter(|e| e["area"] == "unstaged") {
            let adds = e["additions"].as_i64().unwrap_or(0);
            let dels = e["deletions"].as_i64().unwrap_or(0);
            text.push_str(&format!(
                "  {} {}  +{adds} -{dels}\n",
                e["status"].as_str().unwrap_or("?"),
                e["path"].as_str().unwrap_or(""),
            ));
        }
    }
    if staged_count == 0 && unstaged_count == 0 {
        text.push_str("Nothing to commit, working tree clean.\n");
    }

    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn parse_numstat(output: Option<String>) -> HashMap<String, (i32, i32)> {
    let mut map = HashMap::new();
    if let Some(text) = output {
        for line in text.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let a = parts[0].parse::<i32>().unwrap_or(0);
                let d = parts[1].parse::<i32>().unwrap_or(0);
                map.insert(parts[2].to_string(), (a, d));
            }
        }
    }
    map
}

// ── Stage / Unstage / Revert ─────────────────────────────────────────────────

fn handle_stage(
    ctx: &CommandContext,
    cwd: &Path,
    args: &crate::cli::GitStageArgs,
) -> Result<CommandOutput> {
    if args.paths.len() == 1 && args.paths[0] == "." {
        git(cwd, &["add", "-A"])?;
    } else {
        let mut cmd_args = vec!["add", "--"];
        let refs: Vec<&str> = args.paths.iter().map(|s| s.as_str()).collect();
        cmd_args.extend(&refs);
        git(cwd, &cmd_args)?;
    }
    let text = format!("Staged {} path(s)", args.paths.len());
    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({ "staged": args.paths }),
    ))
}

fn handle_unstage(
    ctx: &CommandContext,
    cwd: &Path,
    args: &crate::cli::GitUnstageArgs,
) -> Result<CommandOutput> {
    let mut cmd_args = vec!["reset", "HEAD", "--"];
    let refs: Vec<&str> = args.paths.iter().map(|s| s.as_str()).collect();
    cmd_args.extend(&refs);
    git(cwd, &cmd_args)?;
    let text = format!("Unstaged {} path(s)", args.paths.len());
    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({ "unstaged": args.paths }),
    ))
}

fn handle_revert(
    ctx: &CommandContext,
    cwd: &Path,
    args: &crate::cli::GitRevertArgs,
) -> Result<CommandOutput> {
    if args.all || args.paths.is_empty() {
        git(cwd, &["checkout", "."])?;
        git(cwd, &["clean", "-fd"])?;
        let text = "Reverted all changes.".to_string();
        Ok(to_text_or_json(
            ctx.format,
            text,
            json!({ "reverted": "all" }),
        ))
    } else {
        let mut cmd_args = vec!["checkout", "--"];
        let refs: Vec<&str> = args.paths.iter().map(|s| s.as_str()).collect();
        cmd_args.extend(&refs);
        git(cwd, &cmd_args)?;
        let text = format!("Reverted {} path(s)", args.paths.len());
        Ok(to_text_or_json(
            ctx.format,
            text,
            json!({ "reverted": args.paths }),
        ))
    }
}

// ── Commit ───────────────────────────────────────────────────────────────────

fn handle_commit(
    ctx: &CommandContext,
    cwd: &Path,
    args: &crate::cli::GitCommitArgs,
) -> Result<CommandOutput> {
    if args.all {
        git(cwd, &["add", "-A"])?;
    }

    let msg = args.message.clone().unwrap_or_else(|| {
        format!(
            "grove: changes from {}",
            cwd.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "run".to_string())
        )
    });

    git(cwd, &["commit", "-m", &msg])?;
    let sha = git(cwd, &["rev-parse", "HEAD"])?.trim().to_string();

    let mut text = format!("[{sha_short}] {msg}", sha_short = &sha[..7.min(sha.len())]);
    let mut json_val = json!({ "sha": sha, "message": msg });

    if args.push {
        match push_smart(cwd) {
            Ok(out) => {
                text.push_str(&format!("\nPushed to origin. {out}"));
                json_val["pushed"] = json!(true);
            }
            Err(e) => {
                text.push_str(&format!("\nPush failed: {e}"));
                json_val["push_error"] = json!(e.to_string());
            }
        }
    }

    Ok(to_text_or_json(ctx.format, text, json_val))
}

// ── Push / Pull ──────────────────────────────────────────────────────────────

fn push_smart(cwd: &Path) -> Result<String> {
    // Try plain push first
    let output = std::process::Command::new("git")
        .args(["push"])
        .current_dir(cwd)
        .output()?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if stderr.contains("no upstream branch")
        || stderr.contains("has no upstream")
        || stderr.contains("--set-upstream")
    {
        let retry = std::process::Command::new("git")
            .args(["push", "--set-upstream", "origin", "HEAD"])
            .current_dir(cwd)
            .output()?;
        if retry.status.success() {
            return Ok(String::from_utf8_lossy(&retry.stderr).trim().to_string());
        }
        let retry_err = String::from_utf8_lossy(&retry.stderr).to_string();
        bail!("{}", friendly_push_error(&retry_err));
    }

    bail!("{}", friendly_push_error(&stderr));
}

fn friendly_push_error(stderr: &str) -> String {
    if stderr.contains("non-fast-forward") {
        "Push rejected: remote has changes you don't have locally. Pull first.".to_string()
    } else if stderr.contains("Permission denied") || stderr.contains("403") {
        "Push failed: permission denied. Check your git credentials.".to_string()
    } else if stderr.contains("could not read Username") {
        "Push failed: authentication required. Run `gh auth login`.".to_string()
    } else {
        format!("git push failed: {stderr}")
    }
}

fn handle_push(ctx: &CommandContext, cwd: &Path) -> Result<CommandOutput> {
    let out = push_smart(cwd)?;
    Ok(to_text_or_json(
        ctx.format,
        format!("Pushed. {out}"),
        json!({ "pushed": true }),
    ))
}

fn handle_pull(ctx: &CommandContext, cwd: &Path) -> Result<CommandOutput> {
    let out = git(cwd, &["pull"])?;
    let text = if out.trim() == "Already up to date." {
        "Already up to date.".to_string()
    } else {
        format!("Pulled.\n{out}")
    };
    Ok(to_text_or_json(ctx.format, text, json!({ "pulled": true })))
}

// ── Branch ───────────────────────────────────────────────────────────────────

fn handle_branch(ctx: &CommandContext, cwd: &Path) -> Result<CommandOutput> {
    let branch = git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    let default_branch = detect_default_branch(cwd);

    let (ahead, behind) = get_ahead_behind(cwd, &format!("{branch}@{{upstream}}"), "HEAD")
        .or_else(|| get_ahead_behind(cwd, &format!("origin/{branch}"), "HEAD"))
        .unwrap_or((0, 0));

    let json_val = json!({
        "branch": branch,
        "default_branch": default_branch,
        "ahead": ahead,
        "behind": behind,
    });

    let mut text = format!("Branch: {branch}  (default: {default_branch})");
    if ahead > 0 {
        text.push_str(&format!("\n  {ahead} commit(s) ahead"));
    }
    if behind > 0 {
        text.push_str(&format!("\n  {behind} commit(s) behind"));
    }
    if ahead == 0 && behind == 0 {
        text.push_str("\n  Up to date with remote.");
    }

    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn detect_default_branch(cwd: &Path) -> String {
    // Try gh first
    if let Some(branch) = git_may_fail(
        cwd,
        &["symbolic-ref", "refs/remotes/origin/HEAD", "--short"],
    ) {
        if let Some(name) = branch.strip_prefix("origin/") {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    "main".to_string()
}

fn get_ahead_behind(cwd: &Path, left: &str, right: &str) -> Option<(i32, i32)> {
    let out = git_may_fail(
        cwd,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{left}...{right}"),
        ],
    )?;
    let parts: Vec<&str> = out.split_whitespace().collect();
    if parts.len() == 2 {
        let behind = parts[0].parse().unwrap_or(0);
        let ahead = parts[1].parse().unwrap_or(0);
        Some((ahead, behind))
    } else {
        None
    }
}

// ── Log ──────────────────────────────────────────────────────────────────────

fn handle_log(
    ctx: &CommandContext,
    cwd: &Path,
    args: &crate::cli::GitLogArgs,
) -> Result<CommandOutput> {
    let sep = "---GROVE_SEP---";
    let format_str = format!("%H{sep}%s{sep}%b{sep}%an{sep}%aI{sep}END");
    let count = args.max_count.to_string();

    let raw = git(
        cwd,
        &[
            "log",
            &format!("--max-count={count}"),
            &format!("--pretty=format:{format_str}"),
        ],
    )?;

    let pushed = get_pushed_hashes(cwd);
    let mut entries = Vec::new();

    for record in raw.split(&format!("{sep}END")) {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let parts: Vec<&str> = record.splitn(5, sep).collect();
        if parts.len() < 5 {
            continue;
        }
        let hash = parts[0].trim();
        if entries
            .iter()
            .any(|e: &serde_json::Value| e["hash"].as_str() == Some(hash))
        {
            continue;
        }
        let is_pushed = pushed.contains(hash);
        entries.push(json!({
            "hash": hash,
            "subject": parts[1],
            "body": parts[2].trim(),
            "author": parts[3],
            "date": parts[4],
            "is_pushed": is_pushed,
        }));
    }

    let json_val = json!({ "commits": entries });

    let mut text = String::new();
    for e in &entries {
        let hash =
            &e["hash"].as_str().unwrap_or("")[..7.min(e["hash"].as_str().unwrap_or("").len())];
        let pushed_marker = if e["is_pushed"].as_bool().unwrap_or(false) {
            " [pushed]"
        } else {
            ""
        };
        let subject = e["subject"].as_str().unwrap_or("");
        let author = e["author"].as_str().unwrap_or("");
        let date = e["date"].as_str().unwrap_or("");
        text.push_str(&format!(
            "{hash} {subject}{pushed_marker}  ({author}, {date})\n"
        ));
    }
    if text.is_empty() {
        text = "No commits found.".to_string();
    }

    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn get_pushed_hashes(cwd: &Path) -> HashSet<String> {
    let mut hashes = HashSet::new();
    if let Some(tracking) = git_may_fail(cwd, &["rev-parse", "--abbrev-ref", "@{upstream}"]) {
        if let Some(log) = git_may_fail(cwd, &["log", "--format=%H", "--max-count=100", &tracking])
        {
            for line in log.lines() {
                hashes.insert(line.trim().to_string());
            }
        }
    }
    hashes
}

// ── Undo ─────────────────────────────────────────────────────────────────────

fn handle_undo(ctx: &CommandContext, cwd: &Path) -> Result<CommandOutput> {
    // Safety: check parent exists
    if git_may_fail(cwd, &["rev-parse", "HEAD~1"]).is_none() {
        bail!("Cannot undo: this is the initial commit.");
    }

    // Safety: check not pushed
    let head = git(cwd, &["rev-parse", "HEAD"])?.trim().to_string();
    let pushed = get_pushed_hashes(cwd);
    if pushed.contains(&head) {
        bail!("Cannot undo: this commit has already been pushed.");
    }

    // Get message before reset
    let msg = git(cwd, &["log", "-1", "--pretty=format:%s"])?
        .trim()
        .to_string();

    git(cwd, &["reset", "--soft", "HEAD~1"])?;

    let text = format!("Undid commit: {msg}\nChanges are now staged.");
    let json_val = json!({ "undone": true, "subject": msg });
    Ok(to_text_or_json(ctx.format, text, json_val))
}

// ── PR Create ────────────────────────────────────────────────────────────────

fn handle_pr(
    ctx: &CommandContext,
    cwd: &Path,
    args: &crate::cli::GitPrArgs,
) -> Result<CommandOutput> {
    let default_branch = args
        .base
        .clone()
        .unwrap_or_else(|| detect_default_branch(cwd));

    // Guard: commits ahead of base
    if let Some(count_str) = git_may_fail(
        cwd,
        &[
            "rev-list",
            "--count",
            &format!("origin/{default_branch}..HEAD"),
        ],
    ) {
        let count = count_str.trim().parse::<u64>().unwrap_or(1);
        if count == 0 {
            bail!("No commits ahead of {default_branch} — nothing to create a PR for.");
        }
    }

    // Push first
    if args.push {
        push_smart(cwd)?;
    }

    // Generate title/body if not provided
    let title = args.title.clone().unwrap_or_else(|| {
        git_may_fail(cwd, &["log", "-1", "--pretty=format:%s"])
            .unwrap_or_else(|| "Changes".to_string())
    });

    let body = args.body.clone().unwrap_or_else(|| {
        let stat = git_may_fail(
            cwd,
            &["diff", "--stat", &format!("origin/{default_branch}...HEAD")],
        )
        .unwrap_or_default();
        format!("## Changes\n\n{stat}\n\n---\nCreated by Grove CLI")
    });

    // Write body to temp file
    let body_file = std::env::temp_dir().join("grove_pr_body.md");
    std::fs::write(&body_file, &body)?;

    let output = std::process::Command::new("gh")
        .args([
            "pr",
            "create",
            "--title",
            &title,
            "--body-file",
            body_file.to_str().unwrap_or(""),
        ])
        .current_dir(cwd)
        .env("PATH", grove_core::capability::shell_path())
        .output()?;

    let _ = std::fs::remove_file(&body_file);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.contains("already exists") {
            // Try to get existing PR URL via gh
            let existing_url = std::process::Command::new("gh")
                .args(["pr", "view", "--json", "url,number"])
                .current_dir(cwd)
                .env("PATH", grove_core::capability::shell_path())
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        serde_json::from_slice::<serde_json::Value>(&o.stdout).ok()
                    } else {
                        None
                    }
                });
            let mut result = json!({ "code": "PR_ALREADY_EXISTS", "pushed": true });
            let mut text =
                "A pull request already exists for this branch. Changes were pushed.".to_string();
            if let Some(v) = existing_url {
                let pr_url = v["url"].as_str().unwrap_or("");
                let pr_num = v["number"].as_u64().unwrap_or(0);
                text = format!("PR #{pr_num} already exists: {pr_url}\nChanges were pushed.");
                result["url"] = json!(pr_url);
                result["number"] = json!(pr_num);
            }
            return Ok(to_text_or_json(ctx.format, text, result));
        }
        bail!("gh pr create failed: {stderr}");
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let number = url
        .rsplit('/')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let text = format!("PR #{number} created: {url}");
    let json_val = json!({ "url": url, "number": number });
    Ok(to_text_or_json(ctx.format, text, json_val))
}

// ── PR Status ────────────────────────────────────────────────────────────────

fn handle_pr_status(ctx: &CommandContext, cwd: &Path) -> Result<CommandOutput> {
    let output = std::process::Command::new("gh")
        .args([
            "pr",
            "view",
            "--json",
            "number,url,state,isDraft,mergeStateStatus,title,additions,deletions,changedFiles",
        ])
        .current_dir(cwd)
        .env("PATH", grove_core::capability::shell_path())
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no pull requests found") || stderr.contains("Could not resolve") {
            let text = "No pull request found for the current branch.".to_string();
            return Ok(to_text_or_json(ctx.format, text, json!({ "pr": null })));
        }
        bail!("gh pr view failed: {stderr}");
    }

    let v: serde_json::Value = serde_json::from_slice(&output.stdout)?;

    let state = v["state"].as_str().unwrap_or("UNKNOWN");
    let is_draft = v["isDraft"].as_bool().unwrap_or(false);
    let merge_state = v["mergeStateStatus"].as_str().unwrap_or("UNKNOWN");
    let number = v["number"].as_u64().unwrap_or(0);
    let title = v["title"].as_str().unwrap_or("");
    let url = v["url"].as_str().unwrap_or("");
    let additions = v["additions"].as_i64().unwrap_or(0);
    let deletions = v["deletions"].as_i64().unwrap_or(0);
    let changed = v["changedFiles"].as_i64().unwrap_or(0);

    let state_label = if is_draft { "Draft" } else { state };
    let merge_label = match merge_state {
        "CLEAN" => "Ready to merge",
        "DIRTY" => "Has conflicts",
        "BLOCKED" => "Checks required",
        "BEHIND" => "Branch behind",
        _ => merge_state,
    };

    let text = format!(
        "PR #{number}: {title}\n  State: {state_label}  |  Merge: {merge_label}\n  +{additions} -{deletions}  {changed} file(s)\n  {url}"
    );

    Ok(to_text_or_json(ctx.format, text, v))
}

// ── PR Merge ─────────────────────────────────────────────────────────────────

fn handle_merge(
    ctx: &CommandContext,
    cwd: &Path,
    args: &crate::cli::GitMergeArgs,
) -> Result<CommandOutput> {
    let merge_flag = match args.strategy.as_str() {
        "squash" => "--squash",
        "rebase" => "--rebase",
        _ => "--merge",
    };

    let mut cmd_args = vec!["pr", "merge", merge_flag];
    if args.admin {
        cmd_args.push("--admin");
    }

    let output = std::process::Command::new("gh")
        .args(&cmd_args)
        .current_dir(cwd)
        .env("PATH", grove_core::capability::shell_path())
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.contains("BLOCKED") || stderr.contains("required status check") {
            bail!("Merge blocked: required status checks have not passed.");
        }
        if stderr.contains("DIRTY") || stderr.contains("merge conflict") {
            bail!("Merge blocked: there are merge conflicts to resolve first.");
        }
        bail!("gh pr merge failed: {stderr}");
    }

    let text = format!("PR merged successfully with {} strategy.", args.strategy);
    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({ "merged": true, "strategy": args.strategy }),
    ))
}
