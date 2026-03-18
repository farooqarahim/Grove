use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::errors::{GroveError, GroveResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub todos: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDecomposition {
    pub tasks: Vec<TaskSpec>,
}

/// Read and parse TASKS_{run_id}.json from the architect's worktree.
/// Returns None if the file is absent or unparseable (graceful fallback).
pub fn read_tasks_file(worktree: &Path, run_id: &str) -> Option<TaskDecomposition> {
    let path = worktree.join(format!("TASKS_{run_id}.json"));
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str::<TaskDecomposition>(&content)
        .ok()
        .filter(|d| !d.tasks.is_empty())
}

/// Insert TaskSpec list into the subtasks table.
/// IDs are formatted as "sub_{run_id}_{task_id}".
pub fn insert_subtasks(conn: &Connection, run_id: &str, tasks: &[TaskSpec]) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    for (priority, task) in tasks.iter().enumerate() {
        let id = format!("sub_{}_{}", run_id, task.id);
        let depends_on_json =
            serde_json::to_string(&task.depends_on).unwrap_or_else(|_| "[]".to_string());
        let files_hint_json =
            serde_json::to_string(&task.files).unwrap_or_else(|_| "[]".to_string());
        let todos_json = serde_json::to_string(&task.todos).unwrap_or_else(|_| "[]".to_string());

        conn.execute(
            "INSERT OR IGNORE INTO subtasks
             (id, run_id, session_id, title, description, status, priority,
              depends_on_json, assigned_agent, files_hint_json, todos_json,
              result_summary, created_at, updated_at)
             VALUES (?1, ?2, NULL, ?3, ?4, 'pending', ?5,
                     ?6, NULL, ?7, ?8, NULL, ?9, ?9)",
            params![
                id,
                run_id,
                task.title,
                task.description,
                priority as i64,
                depends_on_json,
                files_hint_json,
                todos_json,
                now,
            ],
        )?;
    }
    Ok(())
}

/// Topological sort of tasks into parallel execution waves.
/// Returns `Vec<Vec<usize>>` — indices into `tasks`, grouped by wave.
/// Returns `Err` if a dependency cycle is detected.
pub fn compute_waves(tasks: &[TaskSpec]) -> GroveResult<Vec<Vec<usize>>> {
    // Map task.id → index in tasks slice.
    let id_to_idx: HashMap<&str, usize> = tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.as_str(), i))
        .collect();

    // in_degree[i] = number of declared dependencies task[i] has.
    let mut in_degree: Vec<usize> = tasks
        .iter()
        .map(|t| {
            t.depends_on
                .iter()
                .filter(|dep| id_to_idx.contains_key(dep.as_str()))
                .count()
        })
        .collect();

    let mut waves: Vec<Vec<usize>> = Vec::new();
    let mut completed: Vec<bool> = vec![false; tasks.len()];
    let mut remaining = tasks.len();

    while remaining > 0 {
        // Collect all tasks with in_degree == 0 that haven't been completed yet.
        let wave: Vec<usize> = (0..tasks.len())
            .filter(|&i| !completed[i] && in_degree[i] == 0)
            .collect();

        if wave.is_empty() {
            return Err(GroveError::Runtime(
                "dependency cycle detected in TASKS file".to_string(),
            ));
        }

        for &idx in &wave {
            completed[idx] = true;
            remaining -= 1;

            // Decrement in_degree for all tasks that depend on this one.
            let completed_id = tasks[idx].id.as_str();
            for (j, other_task) in tasks.iter().enumerate() {
                if !completed[j]
                    && other_task
                        .depends_on
                        .iter()
                        .any(|dep| dep.as_str() == completed_id)
                {
                    in_degree[j] = in_degree[j].saturating_sub(1);
                }
            }
        }

        waves.push(wave);
    }

    Ok(waves)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: &str, deps: &[&str]) -> TaskSpec {
        TaskSpec {
            id: id.to_string(),
            title: id.to_string(),
            description: String::new(),
            files: vec![],
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            todos: vec![],
        }
    }

    #[test]
    fn three_independent_tasks_are_one_wave() {
        let tasks = vec![
            make_task("t1", &[]),
            make_task("t2", &[]),
            make_task("t3", &[]),
        ];
        let waves = compute_waves(&tasks).unwrap();
        assert_eq!(waves.len(), 1);
        assert_eq!(waves[0].len(), 3);
    }

    #[test]
    fn linear_chain_produces_one_task_per_wave() {
        let tasks = vec![
            make_task("t1", &[]),
            make_task("t2", &["t1"]),
            make_task("t3", &["t2"]),
        ];
        let waves = compute_waves(&tasks).unwrap();
        assert_eq!(waves.len(), 3);
        assert_eq!(waves[0], vec![0]);
        assert_eq!(waves[1], vec![1]);
        assert_eq!(waves[2], vec![2]);
    }

    #[test]
    fn diamond_dependency_produces_correct_waves() {
        // t1 → t2, t3 → t4
        let tasks = vec![
            make_task("t1", &[]),
            make_task("t2", &["t1"]),
            make_task("t3", &["t1"]),
            make_task("t4", &["t2", "t3"]),
        ];
        let waves = compute_waves(&tasks).unwrap();
        assert_eq!(waves.len(), 3);
        assert_eq!(waves[0], vec![0]); // t1
        assert_eq!(waves[1].len(), 2); // t2 and t3 in parallel
        assert_eq!(waves[2], vec![3]); // t4
    }

    #[test]
    fn cycle_returns_error() {
        let tasks = vec![make_task("t1", &["t2"]), make_task("t2", &["t1"])];
        assert!(compute_waves(&tasks).is_err());
    }

    #[test]
    fn empty_tasks_returns_empty_waves() {
        let waves = compute_waves(&[]).unwrap();
        assert!(waves.is_empty());
    }

    #[test]
    fn read_tasks_file_returns_none_for_missing_file() {
        let dir = std::path::Path::new("/tmp");
        assert!(read_tasks_file(dir, "nonexistent_run_xyz").is_none());
    }

    #[test]
    fn read_tasks_file_parses_valid_json() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let run_id = "testrun";
        let path = dir.path().join(format!("TASKS_{run_id}.json"));
        let content = r#"{"tasks":[{"id":"t1","title":"Auth","description":"Build auth","files":["src/auth/"],"depends_on":[],"todos":["Create model"]}]}"#;
        std::fs::File::create(&path)
            .unwrap()
            .write_all(content.as_bytes())
            .unwrap();

        let decomp = read_tasks_file(dir.path(), run_id).unwrap();
        assert_eq!(decomp.tasks.len(), 1);
        assert_eq!(decomp.tasks[0].id, "t1");
        assert_eq!(decomp.tasks[0].todos, vec!["Create model"]);
    }
}
