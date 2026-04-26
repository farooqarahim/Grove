mod graph;
mod helpers;
mod phase;
mod run;

use crate::errors::McpError;
use rusqlite::Connection;
use serde_json::{json, Value};

pub fn tool_definitions(run_mode: bool) -> Vec<Value> {
    let tools = vec![
        json!({
            "name": "grove_create_graph",
            "description": "Create a new execution graph for organizing phases and steps of work.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Title for the graph" },
                    "description": { "type": "string", "description": "Optional description of the graph" },
                    "conversation_id": { "type": "string", "description": "ID of the conversation this graph belongs to" }
                },
                "required": ["title", "conversation_id"]
            }
        }),
        json!({
            "name": "grove_add_phase",
            "description": "Add a phase to an existing graph. Phases group related steps and execute in ordinal order.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "graph_id": { "type": "string", "description": "ID of the graph to add the phase to" },
                    "title": { "type": "string", "description": "Title/name for the phase (used for duplicate detection)" },
                    "task_objective": { "type": "string", "description": "Objective describing what this phase accomplishes" },
                    "ordinal": { "type": "integer", "description": "Execution order (0-based)" },
                    "depends_on": { "type": "array", "items": { "type": "string" }, "description": "Optional list of phase IDs this phase depends on" },
                    "ref_required": { "type": "boolean", "description": "Whether a reference document is required" },
                    "reference_doc_path": { "type": "string", "description": "Path to a reference document" }
                },
                "required": ["graph_id", "title", "task_objective", "ordinal"]
            }
        }),
        json!({
            "name": "grove_add_step",
            "description": "Add a step to a phase. Steps are the atomic units of work within a phase.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "phase_id": { "type": "string", "description": "ID of the phase to add the step to" },
                    "graph_id": { "type": "string", "description": "ID of the parent graph" },
                    "title": { "type": "string", "description": "Title/name for the step (used for duplicate detection within phase)" },
                    "task_objective": { "type": "string", "description": "Objective describing what this step accomplishes" },
                    "ordinal": { "type": "integer", "description": "Execution order within the phase (0-based)" },
                    "step_type": { "type": "string", "enum": ["code", "config", "docs", "infra", "test"], "description": "Type of work (default: code)" },
                    "execution_mode": { "type": "string", "enum": ["auto", "manual"], "description": "Execution mode (default: auto)" },
                    "depends_on": { "type": "array", "items": { "type": "string" }, "description": "Optional list of step IDs this step depends on" },
                    "ref_required": { "type": "boolean", "description": "Whether a reference document is required" },
                    "reference_doc_path": { "type": "string", "description": "Path to a reference document" }
                },
                "required": ["phase_id", "graph_id", "title", "task_objective", "ordinal"]
            }
        }),
        json!({
            "name": "grove_update_phase_status",
            "description": "Update the status of a phase (open, inprogress, closed, failed).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "phase_id": { "type": "string", "description": "ID of the phase to update" },
                    "status": { "type": "string", "enum": ["open", "inprogress", "closed", "failed"], "description": "New status" }
                },
                "required": ["phase_id", "status"]
            }
        }),
        json!({
            "name": "grove_update_step_status",
            "description": "Update the status of a step (open, inprogress, closed, failed).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "step_id": { "type": "string", "description": "ID of the step to update" },
                    "status": { "type": "string", "enum": ["open", "inprogress", "closed", "failed"], "description": "New status" }
                },
                "required": ["step_id", "status"]
            }
        }),
        json!({
            "name": "grove_set_step_outcome",
            "description": "Record the outcome of a completed step, including AI comments and optional grade.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "step_id": { "type": "string", "description": "ID of the step" },
                    "outcome": { "type": "string", "description": "Description of what was accomplished" },
                    "ai_comments": { "type": "string", "description": "AI-generated commentary on the step execution" },
                    "grade": { "type": "integer", "description": "Optional quality grade (1-10)" }
                },
                "required": ["step_id", "outcome", "ai_comments"]
            }
        }),
        json!({
            "name": "grove_set_phase_outcome",
            "description": "Record the outcome of a completed phase, including AI comments and optional grade.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "phase_id": { "type": "string", "description": "ID of the phase" },
                    "outcome": { "type": "string", "description": "Description of what was accomplished" },
                    "ai_comments": { "type": "string", "description": "AI-generated commentary on the phase execution" },
                    "grade": { "type": "integer", "description": "Optional quality grade (1-10)" }
                },
                "required": ["phase_id", "outcome", "ai_comments"]
            }
        }),
        json!({
            "name": "grove_list_graph_phases",
            "description": "List all phases of a graph, ordered by ordinal.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "graph_id": { "type": "string", "description": "ID of the graph" }
                },
                "required": ["graph_id"]
            }
        }),
        json!({
            "name": "grove_list_graph_steps",
            "description": "List all steps within a phase, ordered by ordinal.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "phase_id": { "type": "string", "description": "ID of the phase" }
                },
                "required": ["phase_id"]
            }
        }),
        json!({
            "name": "grove_get_graph_progress",
            "description": "Get comprehensive progress overview of a graph including all phases and steps with status counts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "graph_id": { "type": "string", "description": "ID of the graph" }
                },
                "required": ["graph_id"]
            }
        }),
        json!({
            "name": "grove_get_step_pipeline_state",
            "description": "Get the current pipeline state of a step, including judge feedback and computed pipeline stage.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "step_id": { "type": "string", "description": "ID of the step" }
                },
                "required": ["step_id"]
            }
        }),
        json!({
            "name": "grove_check_runtime_status",
            "description": "Check the runtime status of a graph. Agents should call this periodically to detect pause/abort signals.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "graph_id": { "type": "string", "description": "ID of the graph" }
                },
                "required": ["graph_id"]
            }
        }),
        json!({
            "name": "grove_get_step_dependencies_status",
            "description": "Check if all dependencies for a step are satisfied (closed). Returns readiness status and any pending dependencies.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "step_id": { "type": "string", "description": "The step ID to check dependencies for" }
                },
                "required": ["step_id"]
            }
        }),
        json!({
            "name": "grove_get_pipeline_stage",
            "description": "Get the next pending pipeline stage for a run. Returns the stage instructions and metadata. Call this to discover what work to do next.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string", "description": "The run ID to get the next stage for" }
                },
                "required": ["run_id"]
            }
        }),
        json!({
            "name": "grove_complete_pipeline_stage",
            "description": "Mark a pipeline stage as completed with a summary of the work done. Call this after finishing all work for the current stage.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string", "description": "The run ID" },
                    "stage_id": { "type": "string", "description": "The stage ID being completed" },
                    "summary": { "type": "string", "description": "Summary of the work completed in this stage" },
                    "artifacts_json": { "type": "string", "description": "Optional JSON array of artifact paths produced" }
                },
                "required": ["run_id", "stage_id", "summary"]
            }
        }),
        json!({
            "name": "grove_check_pipeline_gate",
            "description": "Check whether a pipeline gate has been approved. Call this after completing a gate-required stage to poll for approval before proceeding.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string", "description": "The run ID" },
                    "stage_id": { "type": "string", "description": "The stage ID with a pending gate" }
                },
                "required": ["run_id", "stage_id"]
            }
        }),
        json!({
            "name": "grove_run_get_context",
            "description": "Get structured context for a classic run, including objective, phase history, gate decisions, artifacts, and recent conversation messages.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string", "description": "ID of the run" }
                },
                "required": ["run_id"]
            }
        }),
        json!({
            "name": "grove_run_get_current_phase",
            "description": "Get the current phase/agent and any pending gate for a classic run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string", "description": "ID of the run" }
                },
                "required": ["run_id"]
            }
        }),
        json!({
            "name": "grove_run_get_phase_artifacts",
            "description": "List artifacts produced in a classic run, optionally filtered by agent/phase.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string", "description": "ID of the run" },
                    "agent": { "type": "string", "description": "Optional agent/phase name" }
                },
                "required": ["run_id"]
            }
        }),
        json!({
            "name": "grove_run_record_artifact",
            "description": "Record an artifact for a classic run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" },
                    "agent": { "type": "string" },
                    "filename": { "type": "string" },
                    "content_hash": { "type": "string" },
                    "size_bytes": { "type": "integer" }
                },
                "required": ["run_id", "agent", "filename"]
            }
        }),
        json!({
            "name": "grove_run_request_gate",
            "description": "Create a pending phase gate for a classic run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" },
                    "agent": { "type": "string" },
                    "artifact_path": { "type": "string" }
                },
                "required": ["run_id", "agent"]
            }
        }),
        json!({
            "name": "grove_run_wait_for_gate",
            "description": "Wait for the latest pending phase gate of a classic run to receive a decision.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" },
                    "timeout_ms": { "type": "integer", "description": "Optional timeout in milliseconds (default 900000)" }
                },
                "required": ["run_id"]
            }
        }),
        json!({
            "name": "grove_run_get_next_step",
            "description": "Get the currently assigned next agent/phase for a classic run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" }
                },
                "required": ["run_id"]
            }
        }),
        json!({
            "name": "grove_run_complete_phase",
            "description": "Mark the current phase agent for a classic run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" },
                    "agent": { "type": "string" }
                },
                "required": ["run_id", "agent"]
            }
        }),
        json!({
            "name": "grove_run_abort_check",
            "description": "Check whether a classic run has been aborted or paused.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" }
                },
                "required": ["run_id"]
            }
        }),
        json!({
            "name": "grove_run_budget_status",
            "description": "Get current budget usage for a classic run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" }
                },
                "required": ["run_id"]
            }
        }),
    ];
    if run_mode {
        tools
            .into_iter()
            .filter(|tool| {
                tool.get("name")
                    .and_then(|name| name.as_str())
                    .is_some_and(|name| name.starts_with("grove_run_"))
            })
            .collect()
    } else {
        tools
    }
}

pub async fn dispatch(
    conn: &Connection,
    tool_name: &str,
    params: &Value,
    run_mode: bool,
) -> Result<Value, McpError> {
    match tool_name {
        "grove_create_graph" if !run_mode => graph::create_graph(conn, params),
        "grove_add_phase" if !run_mode => graph::add_phase(conn, params),
        "grove_add_step" if !run_mode => graph::add_step(conn, params),
        "grove_update_phase_status" if !run_mode => graph::update_phase_status(conn, params),
        "grove_update_step_status" if !run_mode => graph::update_step_status(conn, params),
        "grove_set_step_outcome" if !run_mode => graph::set_step_outcome(conn, params),
        "grove_set_phase_outcome" if !run_mode => graph::set_phase_outcome(conn, params),
        "grove_list_graph_phases" if !run_mode => graph::list_graph_phases(conn, params),
        "grove_list_graph_steps" if !run_mode => graph::list_graph_steps(conn, params),
        "grove_get_graph_progress" if !run_mode => graph::get_graph_progress(conn, params),
        "grove_get_step_pipeline_state" if !run_mode => {
            graph::get_step_pipeline_state(conn, params)
        }
        "grove_check_runtime_status" if !run_mode => graph::check_runtime_status(conn, params),
        "grove_get_step_dependencies_status" if !run_mode => {
            graph::get_step_dependencies_status(conn, params)
        }
        "grove_get_pipeline_stage" => phase::get_pipeline_stage(conn, params),
        "grove_complete_pipeline_stage" => phase::complete_pipeline_stage(conn, params),
        "grove_check_pipeline_gate" => phase::check_pipeline_gate(conn, params),
        "grove_run_get_context" => run::run_get_context(conn, params),
        "grove_run_get_current_phase" => run::run_get_current_phase(conn, params),
        "grove_run_get_phase_artifacts" => run::run_get_phase_artifacts(conn, params),
        "grove_run_record_artifact" => run::run_record_artifact(conn, params),
        "grove_run_request_gate" => run::run_request_gate(conn, params),
        "grove_run_wait_for_gate" => run::run_wait_for_gate(conn, params).await,
        "grove_run_get_next_step" => run::run_get_next_step(conn, params),
        "grove_run_complete_phase" => run::run_complete_phase(conn, params),
        "grove_run_abort_check" => run::run_abort_check(conn, params),
        "grove_run_budget_status" => run::run_budget_status(conn, params),
        _ => Err(McpError::InvalidParams {
            message: format!("unknown tool: {tool_name}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definitions_include_run_tools() {
        let defs = tool_definitions(true);
        let names: Vec<&str> = defs
            .iter()
            .filter_map(|d| d.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"grove_run_get_context"));
        assert!(names.contains(&"grove_run_wait_for_gate"));
        assert!(names.contains(&"grove_run_budget_status"));
        assert!(!names.contains(&"grove_create_graph"));
    }

    #[test]
    fn graph_mode_definitions_include_graph_tools() {
        let defs = tool_definitions(false);
        let names: Vec<&str> = defs
            .iter()
            .filter_map(|d| d.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"grove_create_graph"));
        assert!(names.contains(&"grove_run_get_context"));
    }
}
