use tauri::State;

use super::{FileDiffEntry, shell_path};
use crate::state::AppState;

const PROJECT_ROOT_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);
const RUN_CWD_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Debug, Clone)]
pub(crate) struct RunWorkspaceMeta {
    pub(crate) project_root: std::path::PathBuf,
    pub(crate) conversation_id: Option<String>,
    pub(crate) branch_name: Option<String>,
    pub(crate) recorded_worktree_path: Option<std::path::PathBuf>,
}

/// Resolve the real project root from a run_id by looking up its conversation → project chain.
/// Falls back to workspace_root if lookup fails (shouldn't happen in practice).
/// Result is cached for 30 s to avoid repeated SQL + mutex acquisitions per poll cycle.
pub(crate) fn resolve_project_root(state: &AppState, run_id: &str) -> std::path::PathBuf {
    // Check cache first (short-lived lock, then drop before any DB access).
    {
        let cache = state.project_root_cache.lock();
        if let Some((path, fetched_at)) = cache.get(run_id) {
            if fetched_at.elapsed() < PROJECT_ROOT_CACHE_TTL {
                return path.clone();
            }
        }
    }
    let path = {
        let conn = match state.pool().get() {
            Ok(c) => c,
            Err(_) => return state.workspace_root().to_path_buf(),
        };
        // run → conversation → project → root_path
        let result: Result<String, _> = conn.query_row(
            "SELECT p.root_path FROM runs r
             JOIN conversations c ON r.conversation_id = c.id
             JOIN projects p ON c.project_id = p.id
             WHERE r.id = ?1",
            [run_id],
            |row| row.get(0),
        );
        match result {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => state.workspace_root().to_path_buf(),
        }
    };
    state.project_root_cache.lock().insert(
        run_id.to_string(),
        (path.clone(), std::time::Instant::now()),
    );
    path
}

pub(crate) fn load_run_workspace_meta(state: &AppState, run_id: &str) -> RunWorkspaceMeta {
    let result = {
        let conn = match state.pool().get() {
            Ok(c) => c,
            Err(_) => {
                return RunWorkspaceMeta {
                    project_root: state.workspace_root().to_path_buf(),
                    conversation_id: None,
                    branch_name: None,
                    recorded_worktree_path: None,
                };
            }
        };
        conn.query_row(
            "SELECT p.root_path, r.conversation_id, c.branch_name, c.worktree_path
             FROM runs r
             LEFT JOIN conversations c ON r.conversation_id = c.id
             LEFT JOIN projects p ON c.project_id = p.id
             WHERE r.id = ?1",
            [run_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
    };

    match result {
        Ok((project_root, conversation_id, branch_name, worktree_path)) => RunWorkspaceMeta {
            project_root: project_root
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| state.workspace_root().to_path_buf()),
            conversation_id,
            branch_name,
            recorded_worktree_path: worktree_path
                .filter(|path| !path.trim().is_empty())
                .map(std::path::PathBuf::from),
        },
        Err(_) => RunWorkspaceMeta {
            project_root: state.workspace_root().to_path_buf(),
            conversation_id: None,
            branch_name: None,
            recorded_worktree_path: None,
        },
    }
}

pub(crate) fn resolve_conversation_worktree(
    project_root: &std::path::Path,
    conversation_id: Option<&str>,
    recorded_worktree_path: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    if let Some(path) = recorded_worktree_path.filter(|path| path.is_dir()) {
        return Some(path.to_path_buf());
    }

    let conv_id = conversation_id?;
    let worktrees_base = grove_core::config::grove_dir(project_root).join("worktrees");
    let expected = grove_core::worktree::paths::conv_worktree_path(&worktrees_base, conv_id);
    expected.is_dir().then_some(expected)
}

pub(crate) fn resolve_run_branch_name(state: &AppState, run_id: &str) -> Option<String> {
    let meta = load_run_workspace_meta(state, run_id);
    if let Some(branch_name) = meta.branch_name.filter(|branch| !branch.trim().is_empty()) {
        return Some(branch_name);
    }

    let cwd = resolve_run_cwd(state, run_id);
    grove_core::worktree::git_ops::git_current_branch(&cwd).ok()
}

/// Resolve the working directory for git operations on a run.
/// Prefers the conversation's persistent worktree. Falls back to legacy run
/// worktrees, then to the project root.
/// Result is cached for 10 s.
pub(crate) fn resolve_run_cwd(state: &AppState, run_id: &str) -> std::path::PathBuf {
    // Check cache first.
    {
        let cache = state.run_cwd_cache.lock();
        if let Some((path, fetched_at)) = cache.get(run_id) {
            if fetched_at.elapsed() < RUN_CWD_CACHE_TTL {
                return path.clone();
            }
        }
    }
    let meta = load_run_workspace_meta(state, run_id);
    let project_root = meta.project_root.clone();
    let short = &run_id[..8.min(run_id.len())];
    let legacy_run_wt = project_root
        .join(".grove")
        .join("worktrees")
        .join(format!("run_{short}"));
    let cwd = resolve_conversation_worktree(
        &project_root,
        meta.conversation_id.as_deref(),
        meta.recorded_worktree_path.as_deref(),
    )
    .or_else(|| legacy_run_wt.is_dir().then_some(legacy_run_wt))
    .unwrap_or(project_root);
    state
        .run_cwd_cache
        .lock()
        .insert(run_id.to_string(), (cwd.clone(), std::time::Instant::now()));
    cwd
}

#[derive(serde::Serialize)]
pub struct GitStatusEntry {
    pub path: String,
    /// "staged" or "unstaged"
    pub area: String,
    /// Single char: M, A, D, R, C, ?
    pub status: String,
    pub additions: i32,
    pub deletions: i32,
}

#[tauri::command]
pub async fn git_status_detailed(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<GitStatusEntry>, String> {
    let cwd = resolve_run_cwd(&state, &run_id);
    tauri::async_runtime::spawn_blocking(move || git_status_detailed_sync(&cwd))
        .await
        .map_err(|e| e.to_string())?
}

pub(crate) fn git_status_detailed_sync(
    cwd: &std::path::Path,
) -> Result<Vec<GitStatusEntry>, String> {
    let cwd = cwd.to_path_buf();

    // Run all three git commands in parallel.
    let cwd1 = cwd.clone();
    let cwd2 = cwd.clone();
    let cwd3 = cwd.clone();
    let (status_result, numstat_staged, numstat_unstaged) = std::thread::scope(|s| {
        let h1 = s.spawn(move || {
            std::process::Command::new("git")
                .args(["status", "--porcelain=v1", "--branch"])
                .current_dir(&cwd1)
                .output()
        });
        let h2 = s.spawn(move || {
            std::process::Command::new("git")
                .args(["diff", "--numstat", "--cached"])
                .current_dir(&cwd2)
                .output()
                .ok()
        });
        let h3 = s.spawn(move || {
            std::process::Command::new("git")
                .args(["diff", "--numstat"])
                .current_dir(&cwd3)
                .output()
                .ok()
        });
        (h1.join().unwrap(), h2.join().unwrap(), h3.join().unwrap())
    });

    let output = status_result.map_err(|e| format!("git status failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Build path → (additions, deletions) maps
    fn parse_numstat(
        output: Option<std::process::Output>,
    ) -> std::collections::HashMap<String, (i32, i32)> {
        let mut map = std::collections::HashMap::new();
        if let Some(out) = output {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 3 {
                        let adds = parts[0].parse::<i32>().unwrap_or(0);
                        let dels = parts[1].parse::<i32>().unwrap_or(0);
                        map.insert(parts[2].to_string(), (adds, dels));
                    }
                }
            }
        }
        map
    }

    let staged_stats = parse_numstat(numstat_staged);
    let unstaged_stats = parse_numstat(numstat_unstaged);

    let text = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    for line in text.lines() {
        if line.starts_with("##") || line.is_empty() {
            continue;
        }
        if line.len() < 4 {
            continue;
        }
        let index_status = line.as_bytes()[0] as char;
        let worktree_status = line.as_bytes()[1] as char;
        let path = line[3..].trim().to_string();

        // Staged change
        if index_status != ' ' && index_status != '?' {
            let (adds, dels) = staged_stats.get(&path).copied().unwrap_or((0, 0));
            entries.push(GitStatusEntry {
                path: path.clone(),
                area: "staged".to_string(),
                status: index_status.to_string(),
                additions: adds,
                deletions: dels,
            });
        }
        // Unstaged change
        if worktree_status != ' ' {
            let (adds, dels) = unstaged_stats.get(&path).copied().unwrap_or((0, 0));
            entries.push(GitStatusEntry {
                path,
                area: "unstaged".to_string(),
                status: if worktree_status == '?' {
                    "?".to_string()
                } else {
                    worktree_status.to_string()
                },
                additions: adds,
                deletions: dels,
            });
        }
    }
    Ok(entries)
}

/// Git status for the project root (no run ID needed).
/// Returns changed files relative to the project root working tree.
#[tauri::command]
pub async fn git_project_files(project_root: String) -> Result<Vec<FileDiffEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || git_project_files_sync(&project_root))
        .await
        .map_err(|e| e.to_string())?
}

pub(crate) fn git_project_files_sync(project_root: &str) -> Result<Vec<FileDiffEntry>, String> {
    let cwd = std::path::Path::new(project_root);

    // Use grove_core::git for in-process status (staged + unstaged + untracked).
    let mut uncommitted: Vec<FileDiffEntry> = grove_core::git::worktree_status(cwd)
        .unwrap_or_default()
        .into_iter()
        .map(|c| FileDiffEntry {
            status: c.status,
            path: c.path,
            committed: false,
            area: c.area,
        })
        .collect();

    // Committed-not-pushed files (ahead of upstream tracking ref).
    let uncommitted_paths: std::collections::HashSet<String> =
        uncommitted.iter().map(|f| f.path.clone()).collect();

    for candidate in [
        "@{u}..HEAD",
        &format!("origin/{}..HEAD", detect_default_branch(cwd)),
    ] {
        if let Ok(out) = std::process::Command::new("git")
            .args(["diff", "--name-status", candidate])
            .current_dir(cwd)
            .output()
        {
            if out.status.success() && !out.stdout.is_empty() {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    let mut parts = line.splitn(2, '\t');
                    let status = parts.next().unwrap_or("M").trim().to_string();
                    if let Some(path) = parts.next() {
                        let p = path.trim().to_string();
                        if !uncommitted_paths.contains(&p) {
                            uncommitted.push(FileDiffEntry {
                                status,
                                path: p,
                                committed: true,
                                area: "committed".to_string(),
                            });
                        }
                    }
                }
                break;
            }
        }
    }

    Ok(uncommitted)
}

/// Git status details for the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_status(project_root: String) -> Result<Vec<GitStatusEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);

        let output = std::process::Command::new("git")
            .args(["status", "--porcelain=v1"])
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git status failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "git status failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Collect numstat
        fn parse_numstat_project(
            cwd: &std::path::Path,
            cached: bool,
        ) -> std::collections::HashMap<String, (i32, i32)> {
            let mut args = vec!["diff", "--numstat"];
            if cached {
                args.push("--cached");
            }
            let mut map = std::collections::HashMap::new();
            if let Ok(out) = std::process::Command::new("git")
                .args(&args)
                .current_dir(cwd)
                .output()
            {
                if out.status.success() {
                    let text = String::from_utf8_lossy(&out.stdout);
                    for line in text.lines() {
                        let parts: Vec<&str> = line.split('\t').collect();
                        if parts.len() >= 3 {
                            let adds = parts[0].parse::<i32>().unwrap_or(0);
                            let dels = parts[1].parse::<i32>().unwrap_or(0);
                            map.insert(parts[2].to_string(), (adds, dels));
                        }
                    }
                }
            }
            map
        }

        let staged_stats = parse_numstat_project(cwd, true);
        let unstaged_stats = parse_numstat_project(cwd, false);

        let text = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();
        for line in text.lines() {
            if line.starts_with("##") || line.is_empty() || line.len() < 4 {
                continue;
            }
            let index_status = line.as_bytes()[0] as char;
            let worktree_status = line.as_bytes()[1] as char;
            let path = line[3..].trim().to_string();

            if index_status != ' ' && index_status != '?' {
                let (adds, dels) = staged_stats.get(&path).copied().unwrap_or((0, 0));
                entries.push(GitStatusEntry {
                    path: path.clone(),
                    area: "staged".to_string(),
                    status: index_status.to_string(),
                    additions: adds,
                    deletions: dels,
                });
            }
            if worktree_status != ' ' {
                let (adds, dels) = unstaged_stats.get(&path).copied().unwrap_or((0, 0));
                entries.push(GitStatusEntry {
                    path,
                    area: "unstaged".to_string(),
                    status: if worktree_status == '?' {
                        "?".to_string()
                    } else {
                        worktree_status.to_string()
                    },
                    additions: adds,
                    deletions: dels,
                });
            }
        }
        Ok(entries)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Commit changes in the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_commit(
    project_root: String,
    message: String,
    include_unstaged: bool,
) -> Result<GitCommitResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);
        let result =
            grove_core::worktree::git_ops::git_commit_user(cwd, &message, include_unstaged)
                .map_err(|e| e.to_string())?;
        Ok(GitCommitResult {
            sha: result.sha,
            message: result.message,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Push changes from the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_push(project_root: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);
        grove_core::worktree::git_ops::git_push_auto(cwd).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Pull from remote for the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_pull(project_root: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);
        let output = std::process::Command::new("git")
            .args(["pull"])
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git pull failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if stderr.contains("CONFLICT") {
                return Err("Pull resulted in merge conflicts. Resolve them manually.".to_string());
            }
            return Err(format!("git pull failed: {stderr}"));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Branch status for the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_branch_status(project_root: String) -> Result<BranchStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);
        let info = grove_core::worktree::git_ops::git_branch_status_full(cwd)
            .map_err(|e| e.to_string())?;
        Ok(BranchStatus {
            branch: info.branch,
            default_branch: info.default_branch,
            ahead: info.ahead,
            behind: info.behind,
            has_upstream: info.has_upstream,
            remote_branch_exists: info.remote_branch_exists,
            comparison_mode: info.comparison_mode,
            remote_registration_state: "local_only".to_string(),
            remote_error: None,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Return `true` if `project_root` contains a Git repository.
#[tauri::command]
pub fn git_project_is_repo(project_root: String) -> bool {
    grove_core::worktree::git_ops::is_git_repo(std::path::Path::new(&project_root))
}

/// Run `git init` inside `project_root`, creating a new repository.
#[tauri::command]
pub async fn git_project_init(project_root: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        grove_core::worktree::git_ops::git_init(std::path::Path::new(&project_root))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Get diff for a single file in the project root (no run context needed).
#[tauri::command]
pub async fn git_project_diff(project_root: String, file_path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);

        let output = std::process::Command::new("git")
            .args(["diff", "HEAD", "--", &file_path])
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git diff failed: {e}"))?;

        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            if !text.is_empty() {
                return Ok(text);
            }
        }

        let output2 = std::process::Command::new("git")
            .args(["diff", "--", &file_path])
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git diff failed: {e}"))?;

        let text2 = String::from_utf8_lossy(&output2.stdout).to_string();
        if !text2.is_empty() {
            return Ok(text2);
        }

        // Fallback for committed-but-not-pushed files: range diff against upstream/default.
        let default_branch = detect_default_branch(cwd);
        let base_candidates = [
            "@{u}".to_string(),
            format!("refs/remotes/origin/{default_branch}"),
            format!("refs/heads/{default_branch}"),
            "refs/remotes/origin/main".to_string(),
            "refs/remotes/origin/master".to_string(),
            "refs/heads/main".to_string(),
            "refs/heads/master".to_string(),
        ];
        for base in &base_candidates {
            if let Ok(diffs) = grove_core::git::committed_range_diffs(cwd, base, "HEAD") {
                if let Some(diff) = diffs.get(&file_path) {
                    if !diff.is_empty() {
                        return Ok(diff.clone());
                    }
                }
            }
        }

        Ok(String::new())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Stage specific files in the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_stage_files(
    project_root: String,
    paths: Vec<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if paths.is_empty() {
            return Ok(());
        }
        let cwd = std::path::Path::new(&project_root);
        let mut args = vec!["add".to_string(), "--".to_string()];
        args.extend(paths);
        let output = std::process::Command::new("git")
            .args(&args)
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git add failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "git add failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Unstage specific files in the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_unstage_files(
    project_root: String,
    paths: Vec<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if paths.is_empty() {
            return Ok(());
        }
        let cwd = std::path::Path::new(&project_root);
        let mut args = vec!["reset".to_string(), "HEAD".to_string(), "--".to_string()];
        args.extend(paths);
        let output = std::process::Command::new("git")
            .args(&args)
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git reset failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "git reset failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Stage all changes in the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_stage_all(project_root: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);
        let output = std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git add -A failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "git add -A failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Revert specific files in the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_revert_files(
    project_root: String,
    paths: Vec<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);
        grove_core::worktree::git_ops::git_revert_paths(cwd, &paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Revert all changes in the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_revert_all(project_root: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);

        // Unstage any staged changes first so checkout . can revert them
        let reset_output = std::process::Command::new("git")
            .args(["reset", "HEAD"])
            .current_dir(&cwd)
            .output()
            .map_err(|e| format!("git reset HEAD failed: {e}"))?;
        if !reset_output.status.success() {
            return Err(format!(
                "git reset HEAD failed: {}",
                String::from_utf8_lossy(&reset_output.stderr)
            ));
        }

        // Revert tracked files
        let output = std::process::Command::new("git")
            .args(["checkout", "."])
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git checkout . failed: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "git checkout . failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Remove untracked files
        let output2 = std::process::Command::new("git")
            .args(["clean", "-fd"])
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git clean -fd failed: {e}"))?;
        if !output2.status.success() {
            return Err(format!(
                "git clean -fd failed: {}",
                String::from_utf8_lossy(&output2.stderr)
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Get PR status for the current branch in the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_get_pr_status(project_root: String) -> Result<Option<PrStatus>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);

        let output = std::process::Command::new("gh")
            .args([
                "pr",
                "view",
                "--json",
                "number,url,state,isDraft,mergeStateStatus,title,additions,deletions,changedFiles,baseRefName,headRefName",
            ])
            .current_dir(cwd)
            .env("PATH", shell_path())
            .output()
            .map_err(|e| format!("gh pr view failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("no pull requests found") || stderr.contains("Could not resolve") {
                return Ok(None);
            }
            return Err(format!("gh pr view failed: {stderr}"));
        }

        let v: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("failed to parse gh output: {e}"))?;
        let merge_state = v["mergeStateStatus"]
            .as_str()
            .unwrap_or("UNKNOWN")
            .to_string();
        let conflicting_files = if merge_state == "DIRTY" {
            let base_ref = v["baseRefName"].as_str().unwrap_or("main");
            let head_ref = v["headRefName"].as_str().unwrap_or("HEAD");
            let source_ref = if head_ref.trim().is_empty() {
                "HEAD"
            } else {
                head_ref
            };
            grove_core::worktree::git_ops::git_detect_merge_conflict_files(
                cwd, base_ref, source_ref,
            )
        } else {
            vec![]
        };

        Ok(Some(PrStatus {
            number: v["number"].as_u64().unwrap_or(0),
            url: v["url"].as_str().unwrap_or("").to_string(),
            state: v["state"].as_str().unwrap_or("UNKNOWN").to_string(),
            is_draft: v["isDraft"].as_bool().unwrap_or(false),
            merge_state,
            title: v["title"].as_str().unwrap_or("").to_string(),
            additions: v["additions"].as_i64().unwrap_or(0) as i32,
            deletions: v["deletions"].as_i64().unwrap_or(0) as i32,
            changed_files: v["changedFiles"].as_i64().unwrap_or(0) as i32,
            conflicting_files,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Create a PR from the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_create_pr(
    project_root: String,
    title: String,
    body: String,
) -> Result<PrResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use grove_core::git::publish as pub_core;

        let cwd = std::path::PathBuf::from(&project_root);
        let branch =
            grove_core::worktree::git_ops::git_current_branch(&cwd).map_err(|e| e.to_string())?;
        let sha =
            grove_core::worktree::git_ops::git_rev_parse_head(&cwd).map_err(|e| e.to_string())?;

        let partial = grove_core::git::publish::PublishResult {
            sha,
            commit_message: title.clone(),
            branch,
            pushed: None,
            pr: None,
        };

        let pushed = pub_core::push(&cwd, partial).map_err(|e| e.to_string())?;
        let result = pub_core::create_pr(&cwd, &title, &body, pushed).map_err(|e| e.to_string())?;

        match result.pr {
            Some(info) => Ok(PrResult {
                url: info.url,
                number: info.number,
                code: if info.already_existed {
                    Some("PR_ALREADY_EXISTS".to_string())
                } else {
                    None
                },
            }),
            None => Err("PR creation completed but no PR info returned.".to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Soft reset (undo last commit) in the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_soft_reset(project_root: String) -> Result<SoftResetResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);

        let parent_check = std::process::Command::new("git")
            .args(["rev-parse", "HEAD~1"])
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git rev-parse HEAD~1 failed: {e}"))?;

        if !parent_check.status.success() {
            return Err("Cannot undo: this is the initial commit.".to_string());
        }

        let pushed_hashes = get_pushed_hashes(cwd);
        let head_out = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git rev-parse HEAD failed: {e}"))?;
        let head_sha = String::from_utf8_lossy(&head_out.stdout).trim().to_string();
        if pushed_hashes.contains(&head_sha) {
            return Err("Cannot undo: this commit has already been pushed.".to_string());
        }

        let msg_out = std::process::Command::new("git")
            .args(["log", "-1", "--pretty=format:%s---GROVE_SEP---%b"])
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git log failed: {e}"))?;
        let msg_text = String::from_utf8_lossy(&msg_out.stdout).to_string();
        let parts: Vec<&str> = msg_text.splitn(2, "---GROVE_SEP---").collect();
        let subject = parts.first().unwrap_or(&"").to_string();
        let body = parts.get(1).unwrap_or(&"").trim().to_string();

        let output = std::process::Command::new("git")
            .args(["reset", "--soft", "HEAD~1"])
            .current_dir(cwd)
            .output()
            .map_err(|e| format!("git reset --soft failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "git reset --soft failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(SoftResetResult { subject, body })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Generate PR content from the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_generate_pr_content(
    project_root: String,
    base: Option<String>,
) -> Result<GeneratedPrContent, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);
        let default_branch = base.unwrap_or_else(|| detect_default_branch(cwd));

        let diff_stat = std::process::Command::new("git")
            .args(["diff", "--stat", &format!("origin/{default_branch}...HEAD")])
            .current_dir(cwd)
            .output()
            .ok()
            .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).to_string()) } else { None })
            .unwrap_or_default();

        let commits = std::process::Command::new("git")
            .args(["log", &format!("origin/{default_branch}..HEAD"), "--pretty=format:%s"])
            .current_dir(cwd)
            .output()
            .ok()
            .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).to_string()) } else { None })
            .unwrap_or_default();

        let prompt = format!(
            "Generate a pull request title and description based on these commits and file changes.\n\n\
             Commits:\n{commits}\n\nFile changes:\n{diff_stat}\n\n\
             Respond with ONLY valid JSON: {{\"title\": \"<short title under 70 chars>\", \"description\": \"<markdown body>\"}}"
        );

        let claude_result = std::process::Command::new("claude")
            .args(["-p", &prompt, "--output-format", "json"])
            .current_dir(cwd)
            .env("PATH", shell_path())
            .output();

        if let Ok(claude_out) = claude_result {
            if claude_out.status.success() {
                let raw = String::from_utf8_lossy(&claude_out.stdout);
                let clean: String = raw.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\r').collect();
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&clean) {
                    let title = v["title"].as_str().unwrap_or("").to_string();
                    let desc = v["description"].as_str().unwrap_or("").to_string();
                    if !title.is_empty() {
                        return Ok(GeneratedPrContent { title, description: desc });
                    }
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&clean) {
                    if let Some(result_str) = v["result"].as_str() {
                        if let Ok(inner) = serde_json::from_str::<serde_json::Value>(result_str) {
                            let title = inner["title"].as_str().unwrap_or("").to_string();
                            let desc = inner["description"].as_str().unwrap_or("").to_string();
                            if !title.is_empty() {
                                return Ok(GeneratedPrContent { title, description: desc });
                            }
                        }
                    }
                }
            }
        }

        let first_commit = commits.lines().next().unwrap_or("Changes").to_string();
        let file_list = diff_stat.lines()
            .filter(|l| !l.trim().is_empty() && !l.contains("files changed"))
            .take(20)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(GeneratedPrContent {
            title: first_commit,
            description: format!("## Changes\n\n{file_list}\n\n---\nGenerated by Grove"),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Merge a PR from the project root (no run ID needed).
#[tauri::command]
pub async fn git_project_merge_pr(
    project_root: String,
    strategy: String,
    admin_override: bool,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cwd = std::path::Path::new(&project_root);
        let merge_flag = match strategy.as_str() {
            "squash" => "--squash",
            "rebase" => "--rebase",
            _ => "--merge",
        };

        let mut args = vec!["pr", "merge", merge_flag];
        if admin_override {
            args.push("--admin");
        }

        let output = std::process::Command::new("gh")
            .args(&args)
            .current_dir(cwd)
            .env("PATH", shell_path())
            .output()
            .map_err(|e| format!("gh pr merge failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if stderr.contains("BLOCKED") || stderr.contains("required status check") {
                return Err("Merge blocked: required status checks have not passed.".to_string());
            }
            if stderr.contains("DIRTY") || stderr.contains("merge conflict") {
                return Err(
                    "Merge blocked: there are merge conflicts that must be resolved first."
                        .to_string(),
                );
            }
            return Err(format!("gh pr merge failed: {stderr}"));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_stage_files(
    state: State<'_, AppState>,
    run_id: String,
    paths: Vec<String>,
) -> Result<(), String> {
    let cwd = resolve_run_cwd(&state, &run_id);
    tauri::async_runtime::spawn_blocking(move || {
        let mut args = vec!["add".to_string(), "--".to_string()];
        args.extend(paths);
        let output = std::process::Command::new("git")
            .args(&args)
            .current_dir(&cwd)
            .output()
            .map_err(|e| format!("git add failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "git add failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_unstage_files(
    state: State<'_, AppState>,
    run_id: String,
    paths: Vec<String>,
) -> Result<(), String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        let mut args = vec!["reset".to_string(), "HEAD".to_string(), "--".to_string()];
        args.extend(paths);
        let output = std::process::Command::new("git")
            .args(&args)
            .current_dir(&cwd)
            .output()
            .map_err(|e| format!("git reset failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "git reset failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_stage_all(state: State<'_, AppState>, run_id: String) -> Result<(), String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        let output = std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&cwd)
            .output()
            .map_err(|e| format!("git add -A failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "git add -A failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_revert_files(
    state: State<'_, AppState>,
    run_id: String,
    paths: Vec<String>,
) -> Result<(), String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        grove_core::worktree::git_ops::git_revert_paths(&cwd, &paths).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_revert_all(state: State<'_, AppState>, run_id: String) -> Result<(), String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        // Revert tracked files
        let output = std::process::Command::new("git")
            .args(["checkout", "."])
            .current_dir(&cwd)
            .output()
            .map_err(|e| format!("git checkout . failed: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "git checkout . failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Remove untracked files
        let output2 = std::process::Command::new("git")
            .args(["clean", "-fd"])
            .current_dir(&cwd)
            .output()
            .map_err(|e| format!("git clean -fd failed: {e}"))?;
        if !output2.status.success() {
            return Err(format!(
                "git clean -fd failed: {}",
                String::from_utf8_lossy(&output2.stderr)
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub struct GitCommitResult {
    pub sha: String,
    pub message: String,
}

#[tauri::command]
pub async fn git_commit(
    state: State<'_, AppState>,
    run_id: String,
    message: String,
    include_unstaged: bool,
) -> Result<GitCommitResult, String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        let result =
            grove_core::worktree::git_ops::git_commit_user(&cwd, &message, include_unstaged)
                .map_err(|e| e.to_string())?;

        Ok(GitCommitResult {
            sha: result.sha,
            message: result.message,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_push(state: State<'_, AppState>, run_id: String) -> Result<String, String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        grove_core::worktree::git_ops::git_push_auto(&cwd).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub struct PrResult {
    pub url: String,
    pub number: u64,
    pub code: Option<String>,
}

#[derive(serde::Serialize)]
pub struct PublishChangesResult {
    pub sha: String,
    pub commit_message: String,
    pub branch: String,
    pub pushed: bool,
    pub pr: Option<PrResult>,
}

#[tauri::command]
pub async fn publish_changes(
    state: State<'_, AppState>,
    run_id: Option<String>,
    project_root: Option<String>,
    step: String,
    message: String,
    include_unstaged: bool,
    pr_title: Option<String>,
    pr_body: Option<String>,
) -> Result<PublishChangesResult, String> {
    // Resolve working directory: run worktree or project root
    let cwd = if let Some(rid) = &run_id {
        resolve_run_cwd(&state, rid)
    } else if let Some(root) = &project_root {
        std::path::PathBuf::from(root)
    } else {
        return Err("Either run_id or project_root must be provided.".to_string());
    };

    tauri::async_runtime::spawn_blocking(move || {
        use grove_core::git::publish as pub_core;

        // Step 1: Commit
        let mut result =
            pub_core::commit(&cwd, &message, include_unstaged).map_err(|e| e.to_string())?;

        if step == "commit" {
            return Ok(PublishChangesResult {
                sha: result.sha,
                commit_message: result.commit_message,
                branch: result.branch,
                pushed: false,
                pr: None,
            });
        }

        // Step 2: Push
        result = pub_core::push(&cwd, result).map_err(|e| e.to_string())?;

        if step == "push" {
            return Ok(PublishChangesResult {
                sha: result.sha,
                commit_message: result.commit_message,
                branch: result.branch,
                pushed: true,
                pr: None,
            });
        }

        // Step 3: PR
        let title = pr_title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| result.commit_message.clone());
        let body = pr_body.filter(|b| !b.trim().is_empty()).unwrap_or_else(|| {
            if let Some(rid) = &run_id {
                format!("Changes from Grove run {}", &rid[..8.min(rid.len())])
            } else {
                String::new()
            }
        });

        result = pub_core::create_pr(&cwd, &title, &body, result).map_err(|e| e.to_string())?;

        let pr = result.pr.map(|info| PrResult {
            url: info.url,
            number: info.number,
            code: if info.already_existed {
                Some("PR_ALREADY_EXISTS".to_string())
            } else {
                None
            },
        });

        Ok(PublishChangesResult {
            sha: result.sha,
            commit_message: result.commit_message,
            branch: result.branch,
            pushed: true,
            pr,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Legacy: kept for callers that only need PR creation (with push built in).
#[tauri::command]
pub async fn git_create_pr(
    state: State<'_, AppState>,
    run_id: String,
    title: String,
    body: String,
) -> Result<PrResult, String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        use grove_core::git::publish as pub_core;

        // Commit a no-op to get branch info, then push + PR
        let branch =
            grove_core::worktree::git_ops::git_current_branch(&cwd).map_err(|e| e.to_string())?;
        let sha =
            grove_core::worktree::git_ops::git_rev_parse_head(&cwd).map_err(|e| e.to_string())?;

        let partial = grove_core::git::publish::PublishResult {
            sha,
            commit_message: title.clone(),
            branch,
            pushed: None,
            pr: None,
        };

        // Push
        let pushed = pub_core::push(&cwd, partial).map_err(|e| e.to_string())?;

        // PR
        let result = pub_core::create_pr(&cwd, &title, &body, pushed).map_err(|e| e.to_string())?;

        match result.pr {
            Some(info) => Ok(PrResult {
                url: info.url,
                number: info.number,
                code: if info.already_existed {
                    Some("PR_ALREADY_EXISTS".to_string())
                } else {
                    None
                },
            }),
            None => Err("PR creation completed but no PR info returned.".to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Delegates to `grove_core::git::publish::detect_default_branch` which has
/// its own 5-minute in-memory cache.
pub(crate) fn detect_default_branch(cwd: &std::path::Path) -> String {
    grove_core::git::publish::detect_default_branch(cwd)
}

#[tauri::command]
pub async fn fork_run_worktree(
    state: State<'_, AppState>,
    run_id: String,
    new_branch_name: Option<String>,
) -> Result<String, String> {
    let project_root = resolve_project_root(&state, &run_id);
    let src_branch = resolve_run_branch_name(&state, &run_id)
        .ok_or_else(|| format!("could not resolve branch for run {run_id}"))?;

    tauri::async_runtime::spawn_blocking(move || {
        let short = &run_id[..8.min(run_id.len())];
        let dest_branch = new_branch_name.unwrap_or_else(|| format!("grove/fork-{short}"));
        let worktrees_dir = project_root.join(".grove").join("worktrees");
        let dest_path = worktrees_dir.join(format!("fork_{short}"));

        let output = std::process::Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                &dest_branch,
                dest_path.to_str().unwrap_or(""),
                &src_branch,
            ])
            .current_dir(&project_root)
            .output()
            .map_err(|e| format!("git worktree add failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "git worktree add failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(dest_path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Merge the run branch into the default branch (main/master).
#[tauri::command]
pub async fn git_merge_run_to_main(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<String, String> {
    let conversation_id = load_run_workspace_meta(&state, &run_id)
        .conversation_id
        .ok_or_else(|| format!("run {run_id} is not attached to a conversation"))?;
    let workspace_root = state.workspace_root().to_path_buf();

    tauri::async_runtime::spawn_blocking(move || {
        let result =
            grove_core::orchestrator::merge_conversation(&workspace_root, &conversation_id)
                .map_err(|e| e.to_string())?;

        let message = match result.outcome.as_str() {
            "merged" => format!(
                "Merged {} into {}",
                result.source_branch, result.target_branch
            ),
            "up_to_date" => format!(
                "{} is already up to date with {}",
                result.source_branch, result.target_branch
            ),
            "conflict" => format!(
                "Merge has conflicts in {} file(s). Resolve them before retrying.",
                result.conflicting_files.len()
            ),
            "pr_opened" | "pr_exists" => result
                .pr_url
                .map(|url| format!("PR ready: {url}"))
                .unwrap_or_else(|| {
                    format!(
                        "PR ready for {} -> {}",
                        result.source_branch, result.target_branch
                    )
                }),
            _ => format!(
                "Processed {} -> {}",
                result.source_branch, result.target_branch
            ),
        };

        Ok(message)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_pull(state: State<'_, AppState>, run_id: String) -> Result<String, String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        let output = std::process::Command::new("git")
            .args(["pull"])
            .current_dir(&cwd)
            .output()
            .map_err(|e| format!("git pull failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if stderr.contains("CONFLICT") {
                return Err("Pull resulted in merge conflicts. Resolve them manually.".to_string());
            }
            return Err(format!("git pull failed: {stderr}"));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub struct BranchStatus {
    pub branch: String,
    pub default_branch: String,
    pub ahead: i32,
    pub behind: i32,
    pub has_upstream: bool,
    pub remote_branch_exists: bool,
    pub comparison_mode: String,
    pub remote_registration_state: String,
    pub remote_error: Option<String>,
}

#[tauri::command]
pub async fn git_branch_status(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<BranchStatus, String> {
    let cwd = resolve_run_cwd(&state, &run_id);
    let meta = load_run_workspace_meta(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        let info = grove_core::worktree::git_ops::git_branch_status_full(&cwd)
            .map_err(|e| e.to_string())?;
        let (remote_registration_state, remote_error) =
            conversation_branch_sync_meta(&meta.project_root, meta.conversation_id.as_deref());
        Ok(BranchStatus {
            branch: info.branch,
            default_branch: info.default_branch,
            ahead: info.ahead,
            behind: info.behind,
            has_upstream: info.has_upstream,
            remote_branch_exists: info.remote_branch_exists,
            comparison_mode: info.comparison_mode,
            remote_registration_state,
            remote_error,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize, Clone)]
pub struct GitLogEntry {
    pub hash: String,
    pub subject: String,
    pub body: String,
    pub author: String,
    pub date: String,
    pub is_pushed: bool,
}

pub(crate) fn git_get_log_sync(
    cwd: &std::path::Path,
    max_count: Option<u32>,
) -> Result<Vec<GitLogEntry>, String> {
    let n = max_count.unwrap_or(20) as usize;
    let entries = grove_core::git::commit_log(cwd, n).map_err(|e| e.to_string())?;
    Ok(entries
        .into_iter()
        .map(|c| GitLogEntry {
            hash: c.hash,
            subject: c.subject,
            body: c.body,
            author: c.author,
            date: c.date,
            is_pushed: c.is_pushed,
        })
        .collect())
}

#[tauri::command]
pub async fn git_get_log(
    state: State<'_, AppState>,
    run_id: String,
    max_count: Option<u32>,
) -> Result<Vec<GitLogEntry>, String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || git_get_log_sync(&cwd, max_count))
        .await
        .map_err(|e| e.to_string())?
}

pub(crate) fn get_pushed_hashes(cwd: &std::path::Path) -> std::collections::HashSet<String> {
    let mut hashes = std::collections::HashSet::new();
    // Get the remote tracking ref
    let upstream = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .current_dir(cwd)
        .output()
        .ok();
    if let Some(out) = upstream {
        if out.status.success() {
            let tracking = String::from_utf8_lossy(&out.stdout).trim().to_string();
            // All commits reachable from upstream are "pushed"
            let log = std::process::Command::new("git")
                .args(["log", "--format=%H", "--max-count=100", &tracking])
                .current_dir(cwd)
                .output()
                .ok();
            if let Some(log_out) = log {
                if log_out.status.success() {
                    for line in String::from_utf8_lossy(&log_out.stdout).lines() {
                        hashes.insert(line.trim().to_string());
                    }
                }
            }
        }
    }
    hashes
}

#[tauri::command]
pub async fn git_get_latest_commit(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Option<GitLogEntry>, String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        let entries = git_get_log_sync(&cwd, Some(1))?;
        Ok(entries.into_iter().next())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub struct SoftResetResult {
    pub subject: String,
    pub body: String,
}

#[tauri::command]
pub async fn git_soft_reset(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<SoftResetResult, String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        // Safety: check that HEAD exists and is not the initial commit
        let parent_check = std::process::Command::new("git")
            .args(["rev-parse", "HEAD~1"])
            .current_dir(&cwd)
            .output()
            .map_err(|e| format!("git rev-parse HEAD~1 failed: {e}"))?;

        if !parent_check.status.success() {
            return Err("Cannot undo: this is the initial commit.".to_string());
        }

        // Safety: check if the commit has been pushed
        let pushed_hashes = get_pushed_hashes(&cwd);
        let head_out = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&cwd)
            .output()
            .map_err(|e| format!("git rev-parse HEAD failed: {e}"))?;
        let head_sha = String::from_utf8_lossy(&head_out.stdout).trim().to_string();
        if pushed_hashes.contains(&head_sha) {
            return Err("Cannot undo: this commit has already been pushed.".to_string());
        }

        // Get commit message before reset
        let msg_out = std::process::Command::new("git")
            .args(["log", "-1", "--pretty=format:%s---GROVE_SEP---%b"])
            .current_dir(&cwd)
            .output()
            .map_err(|e| format!("git log failed: {e}"))?;
        let msg_text = String::from_utf8_lossy(&msg_out.stdout).to_string();
        let parts: Vec<&str> = msg_text.splitn(2, "---GROVE_SEP---").collect();
        let subject = parts.first().unwrap_or(&"").to_string();
        let body = parts.get(1).unwrap_or(&"").trim().to_string();

        // Soft reset
        let output = std::process::Command::new("git")
            .args(["reset", "--soft", "HEAD~1"])
            .current_dir(&cwd)
            .output()
            .map_err(|e| format!("git reset --soft failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "git reset --soft failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(SoftResetResult { subject, body })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub struct PrStatus {
    pub number: u64,
    pub url: String,
    pub state: String,
    pub is_draft: bool,
    pub merge_state: String,
    pub title: String,
    pub additions: i32,
    pub deletions: i32,
    pub changed_files: i32,
    pub conflicting_files: Vec<String>,
}

#[tauri::command]
pub async fn git_get_pr_status(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Option<PrStatus>, String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        let output = std::process::Command::new("gh")
            .args([
                "pr",
                "view",
                "--json",
                "number,url,state,isDraft,mergeStateStatus,title,additions,deletions,changedFiles,baseRefName,headRefName",
            ])
            .current_dir(&cwd)
            .env("PATH", shell_path())
            .output()
            .map_err(|e| format!("gh pr view failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // No PR exists for this branch — not an error
            if stderr.contains("no pull requests found") || stderr.contains("Could not resolve") {
                return Ok(None);
            }
            return Err(format!("gh pr view failed: {stderr}"));
        }

        let v: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("failed to parse gh output: {e}"))?;
        let merge_state = v["mergeStateStatus"]
            .as_str()
            .unwrap_or("UNKNOWN")
            .to_string();
        let conflicting_files = if merge_state == "DIRTY" {
            let base_ref = v["baseRefName"].as_str().unwrap_or("main");
            let head_ref = v["headRefName"].as_str().unwrap_or("HEAD");
            let source_ref = if head_ref.trim().is_empty() {
                "HEAD"
            } else {
                head_ref
            };
            grove_core::worktree::git_ops::git_detect_merge_conflict_files(
                &cwd, base_ref, source_ref,
            )
        } else {
            vec![]
        };

        Ok(Some(PrStatus {
            number: v["number"].as_u64().unwrap_or(0),
            url: v["url"].as_str().unwrap_or("").to_string(),
            state: v["state"].as_str().unwrap_or("UNKNOWN").to_string(),
            is_draft: v["isDraft"].as_bool().unwrap_or(false),
            merge_state,
            title: v["title"].as_str().unwrap_or("").to_string(),
            additions: v["additions"].as_i64().unwrap_or(0) as i32,
            deletions: v["deletions"].as_i64().unwrap_or(0) as i32,
            changed_files: v["changedFiles"].as_i64().unwrap_or(0) as i32,
            conflicting_files,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn git_merge_pr(
    state: State<'_, AppState>,
    run_id: String,
    strategy: String,
    admin_override: bool,
) -> Result<String, String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        let merge_flag = match strategy.as_str() {
            "squash" => "--squash",
            "rebase" => "--rebase",
            _ => "--merge",
        };

        let mut args = vec!["pr", "merge", merge_flag];
        if admin_override {
            args.push("--admin");
        }

        let output = std::process::Command::new("gh")
            .args(&args)
            .current_dir(&cwd)
            .env("PATH", shell_path())
            .output()
            .map_err(|e| format!("gh pr merge failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if stderr.contains("BLOCKED") || stderr.contains("required status check") {
                return Err("Merge blocked: required status checks have not passed.".to_string());
            }
            if stderr.contains("DIRTY") || stderr.contains("merge conflict") {
                return Err(
                    "Merge blocked: there are merge conflicts that must be resolved first."
                        .to_string(),
                );
            }
            return Err(format!("gh pr merge failed: {stderr}"));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub struct GeneratedPrContent {
    pub title: String,
    pub description: String,
}

#[tauri::command]
pub async fn git_generate_pr_content(
    state: State<'_, AppState>,
    run_id: String,
    base: Option<String>,
) -> Result<GeneratedPrContent, String> {
    let cwd = resolve_run_cwd(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        let default_branch = base.unwrap_or_else(|| detect_default_branch(&cwd));

        // Get diff stats
        let diff_stat = std::process::Command::new("git")
            .args(["diff", "--stat", &format!("origin/{default_branch}...HEAD")])
            .current_dir(&cwd)
            .output()
            .ok()
            .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).to_string()) } else { None })
            .unwrap_or_default();

        // Get commit messages
        let commits = std::process::Command::new("git")
            .args(["log", &format!("origin/{default_branch}..HEAD"), "--pretty=format:%s"])
            .current_dir(&cwd)
            .output()
            .ok()
            .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).to_string()) } else { None })
            .unwrap_or_default();

        // Try Claude CLI for AI-generated content
        let prompt = format!(
            "Generate a pull request title and description based on these commits and file changes.\n\n\
             Commits:\n{commits}\n\nFile changes:\n{diff_stat}\n\n\
             Respond with ONLY valid JSON: {{\"title\": \"<short title under 70 chars>\", \"description\": \"<markdown body>\"}}"
        );

        let claude_result = std::process::Command::new("claude")
            .args(["-p", &prompt, "--output-format", "json"])
            .current_dir(&cwd)
            .env("PATH", shell_path())
            .output();

        if let Ok(claude_out) = claude_result {
            if claude_out.status.success() {
                let raw = String::from_utf8_lossy(&claude_out.stdout);
                // Strip ANSI codes
                let clean: String = raw.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\r').collect();
                // Try to parse the JSON response
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&clean) {
                    let title = v["title"].as_str().unwrap_or("").to_string();
                    let desc = v["description"].as_str().unwrap_or("").to_string();
                    if !title.is_empty() {
                        return Ok(GeneratedPrContent { title, description: desc });
                    }
                }
                // Try extracting from result envelope: {"result": "...json..."}
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&clean) {
                    if let Some(result_str) = v["result"].as_str() {
                        if let Ok(inner) = serde_json::from_str::<serde_json::Value>(result_str) {
                            let title = inner["title"].as_str().unwrap_or("").to_string();
                            let desc = inner["description"].as_str().unwrap_or("").to_string();
                            if !title.is_empty() {
                                return Ok(GeneratedPrContent { title, description: desc });
                            }
                        }
                    }
                }
            }
        }

        // Fallback: heuristic generation
        let first_commit = commits.lines().next().unwrap_or("Changes from Grove run").to_string();
        let file_list = diff_stat.lines()
            .filter(|l| !l.trim().is_empty() && !l.contains("files changed"))
            .take(20)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(GeneratedPrContent {
            title: first_commit,
            description: format!("## Changes\n\n{file_list}\n\n---\nGenerated by Grove"),
        })
    }).await.map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub struct RightPanelData {
    pub files: Vec<FileDiffEntry>,
    pub branch: Option<BranchStatus>,
    pub latest_commit: Option<GitLogEntry>,
    pub cwd: String,
    pub diffs: std::collections::HashMap<String, String>,
}

#[derive(serde::Serialize)]
pub struct ProjectPanelData {
    pub files: Vec<FileDiffEntry>,
    pub branch: Option<BranchStatus>,
    pub latest_commit: Option<GitLogEntry>,
    pub cwd: String,
    pub diffs: std::collections::HashMap<String, String>,
}

/// Pure sync version — uses grove_core::git for in-process status.
pub(crate) fn list_run_files_sync(
    cwd: &std::path::Path,
    project_root: &std::path::Path,
    run_id: &str,
) -> Vec<FileDiffEntry> {
    grove_core::git::run_files(cwd, project_root, run_id)
        .unwrap_or_default()
        .into_iter()
        .map(|c| FileDiffEntry {
            status: c.status,
            path: c.path,
            committed: c.committed,
            area: c.area,
        })
        .collect()
}

pub(crate) fn conversation_branch_sync_meta(
    project_root: &std::path::Path,
    conversation_id: Option<&str>,
) -> (String, Option<String>) {
    let Some(conversation_id) = conversation_id else {
        return ("local_only".to_string(), None);
    };
    let handle = grove_core::db::DbHandle::new(project_root);
    let Ok(conn) = handle.connect() else {
        return ("local_only".to_string(), None);
    };
    match grove_core::db::repositories::conversations_repo::get(&conn, conversation_id) {
        Ok(row) => (row.remote_registration_state, row.remote_registration_error),
        Err(_) => ("local_only".to_string(), None),
    }
}

/// Pure sync version of `git_branch_status` — uses grove_core::git in-process.
pub(crate) fn git_branch_status_sync(
    cwd: &std::path::Path,
    project_root: &std::path::Path,
    conversation_id: Option<&str>,
) -> Result<BranchStatus, String> {
    let info =
        grove_core::worktree::git_ops::git_branch_status_full(cwd).map_err(|e| e.to_string())?;
    let (remote_registration_state, remote_error) =
        conversation_branch_sync_meta(project_root, conversation_id);
    Ok(BranchStatus {
        branch: info.branch,
        default_branch: info.default_branch,
        ahead: info.ahead,
        behind: info.behind,
        has_upstream: info.has_upstream,
        remote_branch_exists: info.remote_branch_exists,
        comparison_mode: info.comparison_mode,
        remote_registration_state,
        remote_error,
    })
}
