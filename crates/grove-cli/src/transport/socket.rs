#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
#[cfg(unix)]
use std::time::Duration;

use super::{RunResult, StartRunRequest, Transport};
use crate::error::{CliError, CliResult};

pub struct SocketTransport {
    sock_path: PathBuf,
}

impl SocketTransport {
    #[allow(dead_code)] // called from GroveTransport::detect
    pub fn new(sock_path: PathBuf) -> Self {
        Self { sock_path }
    }

    /// Public passthrough to `call` — used by the `grove daemon` lifecycle
    /// commands that want a raw RPC without going through the `Transport`
    /// trait surface.
    pub fn call_raw(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> CliResult<serde_json::Value> {
        self.call(method, params)
    }

    pub fn can_connect(sock_path: &std::path::Path) -> bool {
        connect_socket(sock_path).is_ok()
    }

    /// Send a JSON-RPC 2.0 request over the Unix socket and return the result.
    fn call(&self, method: &str, params: serde_json::Value) -> CliResult<serde_json::Value> {
        call_socket(&self.sock_path, method, params)
    }
}

#[cfg(unix)]
fn connect_socket(sock_path: &std::path::Path) -> CliResult<UnixStream> {
    UnixStream::connect(sock_path)
        .map_err(|e| CliError::Transport(format!("connect to {}: {e}", sock_path.display())))
}

#[cfg(not(unix))]
fn connect_socket(sock_path: &std::path::Path) -> CliResult<()> {
    Err(CliError::Transport(format!(
        "Unix socket transport is not supported on this platform: {}",
        sock_path.display()
    )))
}

#[cfg(unix)]
fn call_socket(
    sock_path: &std::path::Path,
    method: &str,
    params: serde_json::Value,
) -> CliResult<serde_json::Value> {
    let stream = connect_socket(sock_path)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| CliError::Transport(format!("set write timeout: {e}")))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .map_err(|e| CliError::Transport(format!("set read timeout: {e}")))?;

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    });
    let mut request_bytes = serde_json::to_vec(&request)
        .map_err(|e| CliError::Transport(format!("serialize request: {e}")))?;
    request_bytes.push(b'\n');

    let mut writer = stream
        .try_clone()
        .map_err(|e| CliError::Transport(format!("clone stream: {e}")))?;
    writer
        .write_all(&request_bytes)
        .map_err(|e| CliError::Transport(format!("send request: {e}")))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| CliError::Transport(format!("read response: {e}")))?;

    let response: serde_json::Value = serde_json::from_str(line.trim())
        .map_err(|e| CliError::Transport(format!("parse response: {e}")))?;

    if let Some(error) = response.get("error") {
        let msg = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("rpc error");
        return Err(CliError::Transport(msg.to_string()));
    }

    Ok(response
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

#[cfg(not(unix))]
fn call_socket(
    sock_path: &std::path::Path,
    _method: &str,
    _params: serde_json::Value,
) -> CliResult<serde_json::Value> {
    Err(CliError::Transport(format!(
        "Unix socket transport is not supported on this platform: {}",
        sock_path.display()
    )))
}

impl Transport for SocketTransport {
    fn list_runs(&self, limit: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>> {
        let val = self.call("grove.list_runs", serde_json::json!({"limit": limit}))?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>> {
        let val = self.call("grove.list_tasks", serde_json::json!({}))?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn get_workspace(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>> {
        let val = self.call("grove.get_workspace", serde_json::json!({}))?;
        if val.is_null() {
            return Ok(None);
        }
        serde_json::from_value(val)
            .map(Some)
            .map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn list_projects(
        &self,
    ) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>> {
        let val = self.call("grove.list_projects", serde_json::json!({}))?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn list_conversations(
        &self,
        limit: i64,
    ) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        let val = self.call(
            "grove.list_conversations",
            serde_json::json!({"limit": limit}),
        )?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn list_issues(&self, cached: bool) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call("grove.list_issues", serde_json::json!({"cached": cached}))?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn get_issue(&self, id: &str) -> CliResult<serde_json::Value> {
        self.call("grove.get_issue", serde_json::json!({"id": id}))
    }

    fn create_issue(
        &self,
        title: &str,
        body: Option<&str>,
        labels: Vec<String>,
        priority: Option<i64>,
    ) -> CliResult<serde_json::Value> {
        self.call(
            "grove.create_issue",
            serde_json::json!({
                "title": title,
                "body": body,
                "labels": labels,
                "priority": priority,
            }),
        )
    }

    fn close_issue(&self, id: &str) -> CliResult<()> {
        self.call("grove.close_issue", serde_json::json!({"id": id}))
            .map(|_| ())
    }

    fn search_issues(
        &self,
        query: &str,
        limit: i64,
        provider: Option<&str>,
    ) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call(
            "grove.search_issues",
            serde_json::json!({
                "query": query,
                "limit": limit,
                "provider": provider,
            }),
        )?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn sync_issues(&self, provider: Option<&str>, full: bool) -> CliResult<serde_json::Value> {
        self.call(
            "grove.sync_issues",
            serde_json::json!({"provider": provider, "full": full}),
        )
    }

    fn queue_task(
        &self,
        objective: &str,
        priority: i64,
        model: Option<&str>,
        conversation_id: Option<&str>,
        pipeline: Option<&str>,
        permission_mode: Option<&str>,
    ) -> CliResult<grove_core::orchestrator::TaskRecord> {
        let val = self.call(
            "grove.queue_task",
            serde_json::json!({
                "objective": objective,
                "priority": priority,
                "model": model,
                "conversation_id": conversation_id,
                "pipeline": pipeline,
                "permission_mode": permission_mode,
            }),
        )?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn cancel_task(&self, task_id: &str) -> CliResult<()> {
        self.call("grove.cancel_task", serde_json::json!({"task_id": task_id}))
            .map(|_| ())
    }

    fn start_run(&self, req: StartRunRequest) -> CliResult<RunResult> {
        let val = self.call(
            "grove.start_run",
            serde_json::json!({
                "objective": req.objective,
                "pipeline": req.pipeline,
                "model": req.model,
                "permission_mode": req.permission_mode,
                "conversation_id": req.conversation_id,
                "continue_last": req.continue_last,
            }),
        )?;
        let run_id = val
            .get("run_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let task_id = val
            .get("task_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let state = val
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("queued")
            .to_string();
        let objective = val
            .get("objective")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok(RunResult {
            run_id,
            task_id,
            state,
            objective,
        })
    }

    fn drain_queue(&self, _project: &std::path::Path) -> CliResult<()> {
        self.call("grove.drain_queue", serde_json::json!({}))
            .map(|_| ())
    }

    fn get_logs(&self, run_id: &str, all: bool) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call(
            "grove.get_logs",
            serde_json::json!({"run_id": run_id, "all": all}),
        )?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn get_report(&self, run_id: &str) -> CliResult<serde_json::Value> {
        self.call("grove.get_report", serde_json::json!({"run_id": run_id}))
    }

    fn get_plan(&self, run_id: Option<&str>) -> CliResult<serde_json::Value> {
        self.call("grove.get_plan", serde_json::json!({"run_id": run_id}))
    }

    fn get_subtasks(&self, run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call("grove.get_subtasks", serde_json::json!({"run_id": run_id}))?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn get_sessions(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call("grove.get_sessions", serde_json::json!({"run_id": run_id}))?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn abort_run(&self, run_id: &str) -> CliResult<()> {
        self.call("grove.abort_run", serde_json::json!({"run_id": run_id}))
            .map(|_| ())
    }

    fn resume_run(&self, run_id: &str) -> CliResult<()> {
        self.call("grove.resume_run", serde_json::json!({"run_id": run_id}))
            .map(|_| ())
    }

    fn list_providers(&self) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call("grove.list_providers", serde_json::json!({}))?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn set_api_key(&self, provider: &str, key: &str) -> CliResult<()> {
        self.call(
            "grove.set_api_key",
            serde_json::json!({"provider": provider, "key": key}),
        )
        .map(|_| ())
    }

    fn remove_api_key(&self, provider: &str) -> CliResult<()> {
        self.call(
            "grove.remove_api_key",
            serde_json::json!({"provider": provider}),
        )
        .map(|_| ())
    }

    fn list_models(&self, provider: &str) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call(
            "grove.list_models",
            serde_json::json!({"provider": provider}),
        )?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn select_llm(&self, provider: &str, model: Option<&str>) -> CliResult<()> {
        self.call(
            "grove.select_llm",
            serde_json::json!({"provider": provider, "model": model}),
        )
        .map(|_| ())
    }

    fn update_issue(
        &self,
        id: &str,
        title: Option<&str>,
        status: Option<&str>,
        label: Option<&str>,
        assignee: Option<&str>,
        priority: Option<&str>,
    ) -> CliResult<serde_json::Value> {
        self.call(
            "grove.update_issue",
            serde_json::json!({
                "id": id,
                "title": title,
                "status": status,
                "label": label,
                "assignee": assignee,
                "priority": priority,
            }),
        )
    }

    fn comment_issue(&self, id: &str, body: &str) -> CliResult<serde_json::Value> {
        self.call(
            "grove.comment_issue",
            serde_json::json!({"id": id, "body": body}),
        )
    }

    fn assign_issue(&self, id: &str, assignee: &str) -> CliResult<()> {
        self.call(
            "grove.assign_issue",
            serde_json::json!({"id": id, "assignee": assignee}),
        )
        .map(|_| ())
    }

    fn move_issue(&self, id: &str, status: &str) -> CliResult<()> {
        self.call(
            "grove.move_issue",
            serde_json::json!({"id": id, "status": status}),
        )
        .map(|_| ())
    }

    fn reopen_issue(&self, id: &str) -> CliResult<()> {
        self.call("grove.reopen_issue", serde_json::json!({"id": id}))
            .map(|_| ())
    }

    fn activity_issue(&self, id: &str) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call("grove.activity_issue", serde_json::json!({"id": id}))?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn push_issue(&self, id: &str, provider: &str) -> CliResult<serde_json::Value> {
        self.call(
            "grove.push_issue",
            serde_json::json!({"id": id, "provider": provider}),
        )
    }

    fn issue_ready(&self, id: &str) -> CliResult<serde_json::Value> {
        self.call("grove.issue_ready", serde_json::json!({"id": id}))
    }

    fn connect_status(&self) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call("grove.connect_status", serde_json::json!({}))?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn connect_provider(
        &self,
        provider: &str,
        token: Option<&str>,
        site: Option<&str>,
        email: Option<&str>,
    ) -> CliResult<()> {
        self.call(
            "grove.connect_provider",
            serde_json::json!({
                "provider": provider,
                "token": token,
                "site": site,
                "email": email,
            }),
        )
        .map(|_| ())
    }

    fn disconnect_provider(&self, provider: &str) -> CliResult<()> {
        self.call(
            "grove.disconnect_provider",
            serde_json::json!({"provider": provider}),
        )
        .map(|_| ())
    }

    fn run_lint(&self, fix: bool, model: Option<&str>) -> CliResult<serde_json::Value> {
        self.call(
            "grove.run_lint",
            serde_json::json!({"fix": fix, "model": model}),
        )
    }

    fn run_ci(
        &self,
        branch: Option<&str>,
        wait: bool,
        timeout: Option<u64>,
        fix: bool,
        model: Option<&str>,
    ) -> CliResult<serde_json::Value> {
        self.call(
            "grove.run_ci",
            serde_json::json!({
                "branch": branch,
                "wait": wait,
                "timeout": timeout,
                "fix": fix,
                "model": model,
            }),
        )
    }

    fn set_workspace_name(&self, name: &str) -> CliResult<()> {
        self.call(
            "grove.set_workspace_name",
            serde_json::json!({"name": name}),
        )
        .map(|_| ())
    }

    fn archive_workspace(&self, id: &str) -> CliResult<()> {
        self.call("grove.archive_workspace", serde_json::json!({"id": id}))
            .map(|_| ())
    }

    fn delete_workspace(&self, id: &str) -> CliResult<()> {
        self.call("grove.delete_workspace", serde_json::json!({"id": id}))
            .map(|_| ())
    }

    fn get_project(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::projects_repo::ProjectRow>> {
        let val = self.call("grove.get_project", serde_json::json!({}))?;
        if val.is_null() {
            return Ok(None);
        }
        serde_json::from_value(val)
            .map(Some)
            .map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn set_project_name(&self, name: &str) -> CliResult<()> {
        self.call("grove.set_project_name", serde_json::json!({"name": name}))
            .map(|_| ())
    }

    fn set_project_settings(
        &self,
        provider: Option<&str>,
        parallel: Option<i64>,
        pipeline: Option<&str>,
        permission_mode: Option<&str>,
    ) -> CliResult<()> {
        self.call(
            "grove.set_project_settings",
            serde_json::json!({
                "provider": provider,
                "parallel": parallel,
                "pipeline": pipeline,
                "permission_mode": permission_mode,
            }),
        )
        .map(|_| ())
    }

    fn archive_project(&self, id: Option<&str>) -> CliResult<()> {
        self.call("grove.archive_project", serde_json::json!({"id": id}))
            .map(|_| ())
    }

    fn delete_project(&self, id: Option<&str>) -> CliResult<()> {
        self.call("grove.delete_project", serde_json::json!({"id": id}))
            .map(|_| ())
    }

    fn get_conversation(
        &self,
        id: &str,
    ) -> CliResult<Option<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        let val = self.call("grove.get_conversation", serde_json::json!({"id": id}))?;
        if val.is_null() {
            return Ok(None);
        }
        serde_json::from_value(val)
            .map(Some)
            .map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn archive_conversation(&self, id: &str) -> CliResult<()> {
        self.call("grove.archive_conversation", serde_json::json!({"id": id}))
            .map(|_| ())
    }

    fn delete_conversation(&self, id: &str) -> CliResult<()> {
        self.call("grove.delete_conversation", serde_json::json!({"id": id}))
            .map(|_| ())
    }

    fn rebase_conversation(&self, id: &str) -> CliResult<()> {
        self.call("grove.rebase_conversation", serde_json::json!({"id": id}))
            .map(|_| ())
    }

    fn merge_conversation(&self, id: &str) -> CliResult<()> {
        self.call("grove.merge_conversation", serde_json::json!({"id": id}))
            .map(|_| ())
    }

    fn send_signal(
        &self,
        run_id: &str,
        from: &str,
        to: &str,
        signal_type: &str,
        payload: Option<&str>,
        priority: Option<i64>,
    ) -> CliResult<()> {
        self.call(
            "grove.send_signal",
            serde_json::json!({
                "run_id": run_id,
                "from": from,
                "to": to,
                "signal_type": signal_type,
                "payload": payload,
                "priority": priority,
            }),
        )
        .map(|_| ())
    }

    fn check_signals(&self, run_id: &str, agent: &str) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call(
            "grove.check_signals",
            serde_json::json!({"run_id": run_id, "agent": agent}),
        )?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn list_signals(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call("grove.list_signals", serde_json::json!({"run_id": run_id}))?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn run_hook(
        &self,
        event: &str,
        agent_type: Option<&str>,
        run_id: Option<&str>,
        session_id: Option<&str>,
        tool: Option<&str>,
        file_path: Option<&str>,
    ) -> CliResult<()> {
        self.call(
            "grove.run_hook",
            serde_json::json!({
                "event": event,
                "agent_type": agent_type,
                "run_id": run_id,
                "session_id": session_id,
                "tool": tool,
                "file_path": file_path,
            }),
        )
        .map(|_| ())
    }

    fn list_worktrees(&self) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call("grove.list_worktrees", serde_json::json!({}))?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn clean_worktrees(&self) -> CliResult<serde_json::Value> {
        self.call("grove.clean_worktrees", serde_json::json!({}))
    }

    fn delete_worktree(&self, id: &str) -> CliResult<()> {
        self.call("grove.delete_worktree", serde_json::json!({"id": id}))
            .map(|_| ())
    }

    fn delete_all_worktrees(&self) -> CliResult<serde_json::Value> {
        self.call("grove.delete_all_worktrees", serde_json::json!({}))
    }

    fn run_cleanup(
        &self,
        project: bool,
        conversation: bool,
        dry_run: bool,
        yes: bool,
        force: bool,
    ) -> CliResult<serde_json::Value> {
        self.call(
            "grove.run_cleanup",
            serde_json::json!({
                "project": project,
                "conversation": conversation,
                "dry_run": dry_run,
                "yes": yes,
                "force": force,
            }),
        )
    }

    fn run_gc(&self, dry_run: bool) -> CliResult<serde_json::Value> {
        self.call("grove.run_gc", serde_json::json!({"dry_run": dry_run}))
    }

    fn get_run(&self, run_id: &str) -> CliResult<Option<grove_core::orchestrator::RunRecord>> {
        let val = self.call("grove.get_run", serde_json::json!({"run_id": run_id}))?;
        if val.is_null() {
            return Ok(None);
        }
        serde_json::from_value(val)
            .map(Some)
            .map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn list_ownership_locks(&self, run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call(
            "grove.list_ownership_locks",
            serde_json::json!({"run_id": run_id}),
        )?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn list_merge_queue(&self, conversation_id: &str) -> CliResult<Vec<serde_json::Value>> {
        let val = self.call(
            "grove.list_merge_queue",
            serde_json::json!({"conversation_id": conversation_id}),
        )?;
        serde_json::from_value(val).map_err(|e| CliError::Transport(format!("deserialize: {e}")))
    }

    fn retry_publish_run(&self, run_id: &str) -> CliResult<()> {
        self.call(
            "grove.retry_publish_run",
            serde_json::json!({"run_id": run_id}),
        )
        .map(|_| ())
    }
}
