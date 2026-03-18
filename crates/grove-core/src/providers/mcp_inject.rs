use std::path::{Path, PathBuf};

use crate::errors::{GroveError, GroveResult};

/// Graph agent roles that should receive MCP tools for graph manipulation.
///
/// These agents need access to the grove-mcp-server so they can call tools
/// like `grove_create_graph`, `grove_add_phase`, `grove_add_step`, etc.
const GRAPH_AGENT_ROLES: &[&str] = &[
    "pre_planner",
    "graph_creator",
    "phase_worker",
    "orchestrator",
    "phase_validator",
    "phase_judge",
    "pipeline_worker",
];

/// Check whether a role string corresponds to a graph agent type that should
/// receive MCP tools.
///
/// Phase worker and orchestrator are the consolidated execution roles.
/// The MCP config is harmless when present for non-graph invocations -- the
/// agent simply has extra tools available that it will not use.
pub fn is_graph_agent_role(role: &str) -> bool {
    GRAPH_AGENT_ROLES.contains(&role)
}

/// Resolve the `grove-mcp-server` binary path at runtime.
///
/// Strategy:
/// 1. Check `GROVE_MCP_SERVER_PATH` env var (explicit override for dev/test).
/// 2. Look for the binary adjacent to the current executable (same directory).
/// 3. Look for the binary in the cargo target directory (dev builds).
///
/// Returns `None` if the binary cannot be found. The caller should log a
/// warning and proceed without MCP tools rather than failing hard.
pub fn resolve_mcp_server_binary() -> Option<PathBuf> {
    // 1. Explicit env override.
    if let Ok(path) = std::env::var("GROVE_MCP_SERVER_PATH") {
        let p = PathBuf::from(&path);
        if p.is_file() {
            return Some(p);
        }
        tracing::warn!(
            path = %path,
            "GROVE_MCP_SERVER_PATH is set but the file does not exist"
        );
    }

    // 2. Adjacent to current executable.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("grove-mcp-server");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // 3. Cargo target directory heuristic: walk up from the current exe
    //    looking for a `debug/grove-mcp-server` or `release/grove-mcp-server`
    //    sibling in the same target profile directory.
    if let Ok(exe) = std::env::current_exe() {
        // In development: exe is typically at
        //   <workspace>/target/<profile>/grove-gui  (or whatever binary)
        // The MCP server would be at
        //   <workspace>/target/<profile>/grove-mcp-server
        // So the adjacent-exe check above already covers this.
        //
        // As a fallback, check one level up (target/) and look in debug/.
        if let Some(target_dir) = exe.parent().and_then(|p| p.parent()) {
            for profile in &["debug", "release"] {
                let candidate = target_dir.join(profile).join("grove-mcp-server");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

/// Generate the MCP configuration JSON for the grove-mcp-server.
///
/// The returned JSON follows the Claude Code `--mcp-config` format:
/// ```json
/// {
///   "mcpServers": {
///     "grove-graph": {
///       "command": "/path/to/grove-mcp-server",
///       "args": ["--db-path", "/path/to/grove.db"]
///     }
///   }
/// }
/// ```
pub fn build_mcp_config_json_named(
    server_name: &str,
    mode: &str,
    mcp_binary: &Path,
    db_path: &Path,
) -> String {
    serde_json::json!({
        "mcpServers": {
            server_name: {
                "command": mcp_binary.to_string_lossy(),
                "args": ["--db-path", db_path.to_string_lossy(), "--mode", mode]
            }
        }
    })
    .to_string()
}

pub fn build_mcp_config_json(mcp_binary: &Path, db_path: &Path) -> String {
    build_mcp_config_json_named("grove-graph", "graph", mcp_binary, db_path)
}

/// Write the MCP configuration to a temporary file and return its path.
///
/// The temp file is placed in the system temp directory with a predictable
/// prefix so it can be identified in logs. The file is NOT automatically
/// deleted -- the caller is responsible for cleanup (or relies on OS temp
/// cleanup). This is intentional: the spawned agent process needs the file
/// to exist for its entire lifetime.
pub fn write_mcp_config_file_named(
    server_name: &str,
    mcp_binary: &Path,
    db_path: &Path,
) -> GroveResult<PathBuf> {
    let mode = if server_name == "grove-run" {
        "run"
    } else {
        "graph"
    };
    let config_json = build_mcp_config_json_named(server_name, mode, mcp_binary, db_path);

    let tmp_dir = std::env::temp_dir();
    let file_name = format!(
        "grove-mcp-config-{}-{}.json",
        std::process::id(),
        uuid::Uuid::new_v4().as_simple()
    );
    let config_path = tmp_dir.join(file_name);

    std::fs::write(&config_path, config_json.as_bytes()).map_err(|e| {
        GroveError::Runtime(format!(
            "failed to write MCP config to {}: {}",
            config_path.display(),
            e
        ))
    })?;

    Ok(config_path)
}

pub fn write_mcp_config_file(mcp_binary: &Path, db_path: &Path) -> GroveResult<PathBuf> {
    write_mcp_config_file_named("grove-graph", mcp_binary, db_path)
}

/// Prepare MCP config for a graph agent and return the path to the config file.
///
/// Returns `Ok(Some(path))` when the role is a graph agent and the MCP server
/// binary can be found. Returns `Ok(None)` when:
/// - The role is not a graph agent (no MCP needed).
/// - The MCP server binary cannot be found (logged as warning, not an error).
///
/// Returns `Err` only on I/O failure writing the config file.
pub fn prepare_mcp_config_for_role(role: &str, db_path: &Path) -> GroveResult<Option<PathBuf>> {
    if !is_graph_agent_role(role) {
        return Ok(None);
    }

    let mcp_binary = match resolve_mcp_server_binary() {
        Some(p) => p,
        None => {
            tracing::warn!(
                role = %role,
                "grove-mcp-server binary not found -- graph agent will run without MCP tools"
            );
            return Ok(None);
        }
    };

    let config_path = write_mcp_config_file(&mcp_binary, db_path)?;

    tracing::info!(
        role = %role,
        mcp_config = %config_path.display(),
        mcp_binary = %mcp_binary.display(),
        db_path = %db_path.display(),
        "MCP config prepared for graph agent"
    );

    Ok(Some(config_path))
}

/// Prepare MCP config for classic `run` execution. This uses the same binary as
/// graph tools, but under a separate server name so prompts and telemetry can
/// distinguish the control surface.
pub fn prepare_run_mcp_config(db_path: &Path) -> GroveResult<Option<PathBuf>> {
    let mcp_binary = match resolve_mcp_server_binary() {
        Some(p) => p,
        None => {
            tracing::warn!(
                "grove-mcp-server binary not found -- classic run host will start without Run MCP"
            );
            return Ok(None);
        }
    };

    let config_path = write_mcp_config_file_named("grove-run", &mcp_binary, db_path)?;
    tracing::info!(
        mcp_config = %config_path.display(),
        mcp_binary = %mcp_binary.display(),
        db_path = %db_path.display(),
        "MCP config prepared for classic run host"
    );
    Ok(Some(config_path))
}

/// Inject `--mcp-config <path>` into a Claude Code argument list.
///
/// This should be called after the base args are assembled but before the
/// prompt is appended. The `mcp_config_path` is the absolute path to the
/// MCP config JSON file (produced by `write_mcp_config_file`).
pub fn inject_mcp_args_claude(args: &mut Vec<String>, mcp_config_path: &Path) {
    args.push("--mcp-config".into());
    args.push(mcp_config_path.to_string_lossy().into_owned());
}

/// Clean up a previously written MCP config file (best-effort).
///
/// Call this after the agent process has exited. Silently ignores errors
/// (the file is in the system temp directory and will be cleaned up eventually).
pub fn cleanup_mcp_config(config_path: &Path) {
    let _ = std::fs::remove_file(config_path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentType;

    #[test]
    fn graph_agent_roles_detected() {
        assert!(is_graph_agent_role("pre_planner"));
        assert!(is_graph_agent_role("graph_creator"));
        assert!(is_graph_agent_role("phase_worker"));
        assert!(is_graph_agent_role("orchestrator"));
        assert!(is_graph_agent_role("phase_validator"));
        assert!(is_graph_agent_role("phase_judge"));
        assert!(is_graph_agent_role("pipeline_worker"));
    }

    #[test]
    fn non_graph_roles_not_detected() {
        assert!(!is_graph_agent_role("build_prd"));
        assert!(!is_graph_agent_role("plan_system_design"));
        assert!(!is_graph_agent_role("reviewer"));
        assert!(!is_graph_agent_role("unknown"));
        // Retired step-level roles:
        assert!(!is_graph_agent_role("builder"));
        assert!(!is_graph_agent_role("fixer"));
        assert!(!is_graph_agent_role("verdict"));
        assert!(!is_graph_agent_role("judge"));
    }

    #[test]
    fn mcp_config_json_structure() {
        let json = build_mcp_config_json(
            Path::new("/usr/local/bin/grove-mcp-server"),
            Path::new("/data/grove.db"),
        );
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let server = &parsed["mcpServers"]["grove-graph"];
        assert_eq!(
            server["command"].as_str().unwrap(),
            "/usr/local/bin/grove-mcp-server"
        );
        let args = server["args"].as_array().unwrap();
        assert_eq!(args[0].as_str().unwrap(), "--db-path");
        assert_eq!(args[1].as_str().unwrap(), "/data/grove.db");
        assert_eq!(args[2].as_str().unwrap(), "--mode");
        assert_eq!(args[3].as_str().unwrap(), "graph");
    }

    #[test]
    fn named_run_mcp_config_uses_run_server_name() {
        let json = build_mcp_config_json_named(
            "grove-run",
            "run",
            Path::new("/usr/local/bin/grove-mcp-server"),
            Path::new("/data/grove.db"),
        );
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["mcpServers"]["grove-run"].is_object());
        assert!(parsed["mcpServers"]["grove-graph"].is_null());
        let args = parsed["mcpServers"]["grove-run"]["args"]
            .as_array()
            .unwrap();
        assert_eq!(args[2].as_str().unwrap(), "--mode");
        assert_eq!(args[3].as_str().unwrap(), "run");
    }

    #[test]
    fn inject_mcp_args_adds_flag() {
        let mut args = vec!["--print".to_string(), "--verbose".to_string()];
        inject_mcp_args_claude(&mut args, Path::new("/tmp/mcp-config.json"));
        assert_eq!(args.len(), 4);
        assert_eq!(args[2], "--mcp-config");
        assert_eq!(args[3], "/tmp/mcp-config.json");
    }

    #[test]
    fn write_and_cleanup_config_file() {
        let mcp_binary = Path::new("/fake/grove-mcp-server");
        let db_path = Path::new("/fake/grove.db");
        let config_path = write_mcp_config_file(mcp_binary, db_path).unwrap();
        assert!(config_path.exists());
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("grove-graph"));
        assert!(content.contains("--db-path"));
        cleanup_mcp_config(&config_path);
        assert!(!config_path.exists());
    }

    #[test]
    fn all_agent_type_graph_variants_covered() {
        // New consolidated roles (no AgentType variant yet):
        assert!(is_graph_agent_role("phase_worker"));
        assert!(is_graph_agent_role("orchestrator"));
        assert!(is_graph_agent_role("pipeline_worker"));
        // Existing roles with AgentType variants:
        assert!(is_graph_agent_role(AgentType::PhaseValidator.as_str()));
        assert!(is_graph_agent_role(AgentType::PhaseJudge.as_str()));
        assert!(is_graph_agent_role(AgentType::PrePlanner.as_str()));
        assert!(is_graph_agent_role(AgentType::GraphCreator.as_str()));
    }
}
