use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::agents::AgentType;

use super::GrovePlanStep;

#[derive(Debug, Deserialize, Serialize)]
struct GroveSpawnFile {
    steps: Vec<GrovePlanStep>,
}

/// Read `GROVE_SPAWN.json` from `worktree`, validate, and atomically consume it
/// (rename to `GROVE_SPAWN.consumed.json` so resume replays are idempotent).
///
/// Returns `None` if:
/// - The file is absent or cannot be read/parsed.
/// - The file is empty (zero steps).
/// - Any step has `agent_type == "planner"`.
/// - Any step has an unrecognised `agent_type`.
/// - Any `depends_on` ID is not present in the same file's steps.
pub fn read_spawn_file(worktree: &Path) -> Option<Vec<GrovePlanStep>> {
    let spawn_path = worktree.join("GROVE_SPAWN.json");
    if !spawn_path.exists() {
        return None;
    }

    let content = match std::fs::read_to_string(&spawn_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[SPAWN] could not read GROVE_SPAWN.json: {e}");
            return None;
        }
    };

    if content.trim().is_empty() {
        let _ = std::fs::rename(&spawn_path, worktree.join("GROVE_SPAWN.consumed.json"));
        return None;
    }

    let file: GroveSpawnFile = match serde_json::from_str(&content) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[SPAWN] invalid GROVE_SPAWN.json (parse error): {e}");
            let _ = std::fs::rename(&spawn_path, worktree.join("GROVE_SPAWN.consumed.json"));
            return None;
        }
    };

    let consumed_path = worktree.join("GROVE_SPAWN.consumed.json");

    if file.steps.is_empty() {
        let _ = std::fs::rename(&spawn_path, &consumed_path);
        return None;
    }

    // Validate: no planner steps.
    for step in &file.steps {
        if step.agent_type == "planner" {
            eprintln!("[SPAWN] rejected GROVE_SPAWN.json: agent_type 'planner' is not allowed");
            let _ = std::fs::rename(&spawn_path, &consumed_path);
            return None;
        }
        if AgentType::from_str(&step.agent_type).is_none() {
            eprintln!(
                "[SPAWN] rejected GROVE_SPAWN.json: unknown agent_type '{}'",
                step.agent_type
            );
            let _ = std::fs::rename(&spawn_path, &consumed_path);
            return None;
        }
    }

    // Validate: all depends_on IDs exist within this file's steps.
    let step_ids: HashSet<&str> = file.steps.iter().map(|s| s.id.as_str()).collect();
    for step in &file.steps {
        for dep in &step.depends_on {
            if !step_ids.contains(dep.as_str()) {
                eprintln!(
                    "[SPAWN] rejected GROVE_SPAWN.json: depends_on ID '{dep}' not found in spawn file"
                );
                let _ = std::fs::rename(&spawn_path, &consumed_path);
                return None;
            }
        }
    }

    // 5.11: Cycle detection — DFS on the depends_on graph.
    let graph: HashMap<&str, &[String]> = file
        .steps
        .iter()
        .map(|s| (s.id.as_str(), s.depends_on.as_slice()))
        .collect();
    if depends_on_has_cycle(&graph) {
        eprintln!("[SPAWN] rejected GROVE_SPAWN.json: circular dependency detected in depends_on");
        let _ = std::fs::rename(&spawn_path, &consumed_path);
        return None;
    }

    // Atomically consume the file before returning steps.
    if let Err(e) = std::fs::rename(&spawn_path, &consumed_path) {
        eprintln!("[SPAWN] warning: could not rename GROVE_SPAWN.json: {e}");
    }

    Some(file.steps)
}

/// Returns `true` if the `depends_on` graph contains a cycle.
///
/// Uses iterative DFS with a three-colour marking scheme:
/// - `0` = unvisited, `1` = in the current DFS path, `2` = fully processed.
fn depends_on_has_cycle(graph: &HashMap<&str, &[String]>) -> bool {
    let mut color: HashMap<&str, u8> = HashMap::new();

    for &start in graph.keys() {
        if color.get(start).copied().unwrap_or(0) == 2 {
            continue;
        }
        // Iterative DFS: stack holds (node, iterator-over-children).
        let mut stack: Vec<(&str, std::slice::Iter<String>)> = Vec::new();
        color.insert(start, 1);
        stack.push((start, graph[start].iter()));

        while let Some((node, children)) = stack.last_mut() {
            if let Some(child) = children.next() {
                let child = child.as_str();
                match color.get(child).copied().unwrap_or(0) {
                    1 => return true, // back-edge → cycle
                    2 => {}           // already done
                    _ => {
                        color.insert(child, 1);
                        let child_deps = graph.get(child).copied().unwrap_or(&[]);
                        stack.push((child, child_deps.iter()));
                    }
                }
            } else {
                // All children processed — mark done and pop.
                let n = *node;
                color.insert(n, 2);
                stack.pop();
            }
        }
    }
    false
}

/// Concise instructions appended to every agent prompt enabling dynamic spawning.
pub fn spawn_instructions() -> &'static str {
    r#"

## Dynamic Agent Spawning

If you discover work that is genuinely outside your current scope and requires a
separate agent (e.g., a new component, an OAuth handler not in the original plan,
a bug that needs dedicated debugging), you may request additional agents by writing
a file called `GROVE_SPAWN.json` in your working directory.

**Rules:**
- Only spawn for truly new, independent work you cannot complete yourself.
- Never create a 'planner' agent.
- Each spawned step must be independently executable.

**Format:**
```json
{
  "steps": [
    {
      "id": "sp1",
      "agent_type": "builder",
      "title": "Short descriptive title",
      "description": "Full description of what this agent must do",
      "todos": ["Implement X()", "Add Y middleware"],
      "files": ["src/auth/refresh.rs"],
      "depends_on": []
    }
  ]
}
```

Valid agent_type values: architect, builder, tester, reviewer, debugger, documenter,
researcher, validator, deployer, monitor, refactorer, security, performance,
data_migrator, api_designer, integrator, coordinator.

If you have no new work to spawn, do not write this file.
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_worktree() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn write_spawn(dir: &Path, content: &str) {
        fs::write(dir.join("GROVE_SPAWN.json"), content).expect("write spawn file");
    }

    #[test]
    fn absent_file_returns_none() {
        let tmp = make_worktree();
        assert!(read_spawn_file(tmp.path()).is_none());
    }

    #[test]
    fn empty_steps_returns_none_and_consumes() {
        let tmp = make_worktree();
        write_spawn(tmp.path(), r#"{"steps":[]}"#);
        assert!(read_spawn_file(tmp.path()).is_none());
        assert!(!tmp.path().join("GROVE_SPAWN.json").exists());
        assert!(tmp.path().join("GROVE_SPAWN.consumed.json").exists());
    }

    #[test]
    fn valid_spawn_file_returns_steps_and_consumes() {
        let tmp = make_worktree();
        write_spawn(
            tmp.path(),
            r#"{"steps":[{"id":"sp1","agent_type":"builder","title":"T","description":"D","todos":["do x"],"files":["f.rs"],"depends_on":[]}]}"#,
        );
        let steps = read_spawn_file(tmp.path()).expect("should parse");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].id, "sp1");
        assert!(!tmp.path().join("GROVE_SPAWN.json").exists());
        assert!(tmp.path().join("GROVE_SPAWN.consumed.json").exists());
    }

    #[test]
    fn planner_type_rejected() {
        let tmp = make_worktree();
        write_spawn(
            tmp.path(),
            r#"{"steps":[{"id":"s1","agent_type":"planner","title":"T","description":"D","todos":[],"files":[],"depends_on":[]}]}"#,
        );
        assert!(read_spawn_file(tmp.path()).is_none());
        assert!(tmp.path().join("GROVE_SPAWN.consumed.json").exists());
    }

    #[test]
    fn unknown_agent_type_rejected() {
        let tmp = make_worktree();
        write_spawn(
            tmp.path(),
            r#"{"steps":[{"id":"s1","agent_type":"unicorn","title":"T","description":"D","todos":[],"files":[],"depends_on":[]}]}"#,
        );
        assert!(read_spawn_file(tmp.path()).is_none());
        assert!(tmp.path().join("GROVE_SPAWN.consumed.json").exists());
    }

    #[test]
    fn bad_depends_on_rejected() {
        let tmp = make_worktree();
        write_spawn(
            tmp.path(),
            r#"{"steps":[{"id":"s1","agent_type":"builder","title":"T","description":"D","todos":[],"files":[],"depends_on":["nonexistent"]}]}"#,
        );
        assert!(read_spawn_file(tmp.path()).is_none());
        assert!(tmp.path().join("GROVE_SPAWN.consumed.json").exists());
    }

    #[test]
    fn consumed_file_not_replayed() {
        let tmp = make_worktree();
        write_spawn(
            tmp.path(),
            r#"{"steps":[{"id":"sp1","agent_type":"tester","title":"T","description":"D","todos":[],"files":[],"depends_on":[]}]}"#,
        );
        // First call consumes
        let first = read_spawn_file(tmp.path());
        assert!(first.is_some());
        // Second call: file is gone, returns None
        let second = read_spawn_file(tmp.path());
        assert!(second.is_none());
    }

    #[test]
    fn invalid_json_returns_none() {
        let tmp = make_worktree();
        write_spawn(tmp.path(), "{ not valid json }");
        assert!(read_spawn_file(tmp.path()).is_none());
        assert!(tmp.path().join("GROVE_SPAWN.consumed.json").exists());
    }

    #[test]
    fn circular_depends_on_rejected() {
        let tmp = make_worktree();
        // A → B → A (cycle)
        write_spawn(
            tmp.path(),
            r#"{"steps":[
                {"id":"a","agent_type":"builder","title":"A","description":"D","todos":[],"files":[],"depends_on":["b"]},
                {"id":"b","agent_type":"builder","title":"B","description":"D","todos":[],"files":[],"depends_on":["a"]}
            ]}"#,
        );
        assert!(read_spawn_file(tmp.path()).is_none());
        assert!(tmp.path().join("GROVE_SPAWN.consumed.json").exists());
    }

    #[test]
    fn self_loop_depends_on_rejected() {
        let tmp = make_worktree();
        write_spawn(
            tmp.path(),
            r#"{"steps":[
                {"id":"a","agent_type":"builder","title":"A","description":"D","todos":[],"files":[],"depends_on":["a"]}
            ]}"#,
        );
        assert!(read_spawn_file(tmp.path()).is_none());
        assert!(tmp.path().join("GROVE_SPAWN.consumed.json").exists());
    }

    #[test]
    fn longer_cycle_rejected() {
        let tmp = make_worktree();
        // A → B → C → A
        write_spawn(
            tmp.path(),
            r#"{"steps":[
                {"id":"a","agent_type":"builder","title":"A","description":"D","todos":[],"files":[],"depends_on":["c"]},
                {"id":"b","agent_type":"tester","title":"B","description":"D","todos":[],"files":[],"depends_on":["a"]},
                {"id":"c","agent_type":"reviewer","title":"C","description":"D","todos":[],"files":[],"depends_on":["b"]}
            ]}"#,
        );
        assert!(read_spawn_file(tmp.path()).is_none());
    }

    #[test]
    fn dag_without_cycle_accepted() {
        let tmp = make_worktree();
        // A → C, B → C (diamond, no cycle)
        write_spawn(
            tmp.path(),
            r#"{"steps":[
                {"id":"a","agent_type":"builder","title":"A","description":"D","todos":[],"files":[],"depends_on":[]},
                {"id":"b","agent_type":"builder","title":"B","description":"D","todos":[],"files":[],"depends_on":[]},
                {"id":"c","agent_type":"tester","title":"C","description":"D","todos":[],"files":[],"depends_on":["a","b"]}
            ]}"#,
        );
        let steps = read_spawn_file(tmp.path()).expect("DAG should be accepted");
        assert_eq!(steps.len(), 3);
    }

    #[test]
    fn has_cycle_unit_no_cycle() {
        let mut graph: HashMap<&str, &[String]> = HashMap::new();
        let empty: &[String] = &[];
        graph.insert("a", empty);
        graph.insert("b", empty);
        assert!(!depends_on_has_cycle(&graph));
    }

    #[test]
    fn has_cycle_unit_self_loop() {
        let self_dep = vec!["a".to_string()];
        let mut graph: HashMap<&str, &[String]> = HashMap::new();
        graph.insert("a", &self_dep);
        assert!(depends_on_has_cycle(&graph));
    }
}
