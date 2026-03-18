use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::errors::GroveResult;

/// Broadcast group: all agents in the run.
pub const GROUP_ALL: &str = "@all";
/// Broadcast group: all builder agents in the run.
pub const GROUP_BUILDERS: &str = "@builders";
/// Broadcast group: architect + reviewer + security agents.
pub const GROUP_LEADS: &str = "@leads";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalType {
    Status,
    Question,
    Result,
    Error,
    WorkerDone,
    MergeReady,
    Escalation,
    Dispatch,
    BudgetWarning,
    DesignReady,
    TestResult,
    ReviewResult,
}

impl SignalType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Question => "question",
            Self::Result => "result",
            Self::Error => "error",
            Self::WorkerDone => "worker_done",
            Self::MergeReady => "merge_ready",
            Self::Escalation => "escalation",
            Self::Dispatch => "dispatch",
            Self::BudgetWarning => "budget_warning",
            Self::DesignReady => "design_ready",
            Self::TestResult => "test_result",
            Self::ReviewResult => "review_result",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "status" => Some(Self::Status),
            "question" => Some(Self::Question),
            "result" => Some(Self::Result),
            "error" => Some(Self::Error),
            "worker_done" => Some(Self::WorkerDone),
            "merge_ready" => Some(Self::MergeReady),
            "escalation" => Some(Self::Escalation),
            "dispatch" => Some(Self::Dispatch),
            "budget_warning" => Some(Self::BudgetWarning),
            "design_ready" => Some(Self::DesignReady),
            "test_result" => Some(Self::TestResult),
            "review_result" => Some(Self::ReviewResult),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalPriority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

impl SignalPriority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "low" => Self::Low,
            "high" => Self::High,
            "urgent" => Self::Urgent,
            _ => Self::Normal,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub id: String,
    pub run_id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub signal_type: String,
    pub priority: String,
    pub payload: Value,
    pub read: bool,
    pub created_at: String,
}

/// Send a signal from one agent to another.
pub fn send_signal(
    conn: &Connection,
    run_id: &str,
    from: &str,
    to: &str,
    signal_type: SignalType,
    priority: SignalPriority,
    payload: Value,
) -> GroveResult<String> {
    let id = format!("sig_{}", Uuid::new_v4().simple());
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO signals (id, run_id, from_agent, to_agent, signal_type, priority, payload_json, read, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
        params![
            id,
            run_id,
            from,
            to,
            signal_type.as_str(),
            priority.as_str(),
            serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into()),
            now,
        ],
    )?;

    // Emit observability event for the signal.
    let _ = crate::events::emit(
        conn,
        run_id,
        None,
        crate::events::event_types::SIGNAL_SENT,
        serde_json::json!({
            "signal_id": id,
            "from": from,
            "to": to,
            "signal_type": signal_type.as_str(),
            "priority": priority.as_str(),
        }),
    );

    Ok(id)
}

/// Broadcast a signal to all agents in a group.
///
/// Returns the IDs of all created signal rows.
pub fn broadcast(
    conn: &Connection,
    run_id: &str,
    from: &str,
    group: &str,
    signal_type: SignalType,
    priority: SignalPriority,
    payload: Value,
) -> GroveResult<Vec<String>> {
    let targets = expand_group(conn, run_id, group)?;
    let mut ids = Vec::new();
    for target in targets {
        if target == from {
            continue; // Don't signal yourself
        }
        let id = send_signal(
            conn,
            run_id,
            from,
            &target,
            signal_type,
            priority,
            payload.clone(),
        )?;
        ids.push(id);
    }

    // Emit broadcast event (in addition to per-signal SIGNAL_SENT events).
    if !ids.is_empty() {
        let _ = crate::events::emit(
            conn,
            run_id,
            None,
            crate::events::event_types::SIGNAL_BROADCAST,
            serde_json::json!({
                "from": from,
                "group": group,
                "signal_type": signal_type.as_str(),
                "count": ids.len(),
            }),
        );
    }

    Ok(ids)
}

/// Check for unread signals addressed to `agent_name`.
pub fn check_signals(
    conn: &Connection,
    run_id: &str,
    agent_name: &str,
) -> GroveResult<Vec<Signal>> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, from_agent, to_agent, signal_type, priority, payload_json, read, created_at
         FROM signals
         WHERE run_id = ?1 AND to_agent = ?2 AND read = 0
         ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map(params![run_id, agent_name], map_signal_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Mark a signal as read.
pub fn mark_read(conn: &Connection, signal_id: &str) -> GroveResult<()> {
    conn.execute("UPDATE signals SET read = 1 WHERE id = ?1", [signal_id])?;
    Ok(())
}

/// List all signals for a run (read and unread).
pub fn list_for_run(conn: &Connection, run_id: &str) -> GroveResult<Vec<Signal>> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, from_agent, to_agent, signal_type, priority, payload_json, read, created_at
         FROM signals
         WHERE run_id = ?1
         ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([run_id], map_signal_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn map_signal_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Signal> {
    let payload_str: String = r.get(6)?;
    let payload: Value =
        serde_json::from_str(&payload_str).unwrap_or(Value::Object(Default::default()));
    Ok(Signal {
        id: r.get(0)?,
        run_id: r.get(1)?,
        from_agent: r.get(2)?,
        to_agent: r.get(3)?,
        signal_type: r.get(4)?,
        priority: r.get(5)?,
        payload,
        read: r.get::<_, i64>(7)? != 0,
        created_at: r.get(8)?,
    })
}

/// Expand a broadcast group to a list of agent names (from active sessions).
fn expand_group(conn: &Connection, run_id: &str, group: &str) -> GroveResult<Vec<String>> {
    let sql = match group {
        GROUP_ALL => {
            "SELECT DISTINCT agent_type FROM sessions WHERE run_id = ?1 AND state = 'running'"
        }
        GROUP_BUILDERS => {
            "SELECT DISTINCT agent_type FROM sessions WHERE run_id = ?1 AND state = 'running' AND agent_type = 'builder'"
        }
        GROUP_LEADS => {
            "SELECT DISTINCT agent_type FROM sessions WHERE run_id = ?1 AND state = 'running' AND agent_type IN ('architect', 'reviewer', 'security')"
        }
        _ => {
            // Treat as a direct agent name
            return Ok(vec![group.to_string()]);
        }
    };

    let mut stmt = conn.prepare(sql)?;
    let names: Vec<String> = stmt
        .query_map([run_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::db::DbHandle;

    fn setup_test_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        db::initialize(dir.path()).unwrap();
        let handle = DbHandle::new(dir.path());
        let conn = handle.connect().unwrap();
        (dir, conn)
    }

    fn insert_test_run(conn: &Connection, run_id: &str) {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES (?1, 'test', 'executing', 5.0, 0.0, ?2, ?2)",
            params![run_id, now],
        )
        .unwrap();
    }

    fn insert_running_session(conn: &Connection, session_id: &str, run_id: &str, agent_type: &str) {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO sessions (id, run_id, agent_type, state, worktree_path, started_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'running', '/tmp/wt', ?4, ?4, ?4)",
            params![session_id, run_id, agent_type, now],
        )
        .unwrap();
    }

    #[test]
    fn test_send_and_check() {
        let (_dir, conn) = setup_test_db();
        let run_id = "run_sig1";
        insert_test_run(&conn, run_id);

        let id = send_signal(
            &conn,
            run_id,
            "architect",
            "builder",
            SignalType::DesignReady,
            SignalPriority::Normal,
            serde_json::json!({"phase": "ready"}),
        )
        .unwrap();
        assert!(id.starts_with("sig_"));

        let signals = check_signals(&conn, run_id, "builder").unwrap();
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].from_agent, "architect");
        assert_eq!(signals[0].signal_type, "design_ready");
        assert!(!signals[0].read);
    }

    #[test]
    fn test_mark_read() {
        let (_dir, conn) = setup_test_db();
        let run_id = "run_sig2";
        insert_test_run(&conn, run_id);

        let id = send_signal(
            &conn,
            run_id,
            "tester",
            "builder",
            SignalType::TestResult,
            SignalPriority::High,
            serde_json::json!({}),
        )
        .unwrap();

        mark_read(&conn, &id).unwrap();

        let signals = check_signals(&conn, run_id, "builder").unwrap();
        assert!(signals.is_empty(), "should be empty after mark_read");

        // But list_for_run shows it
        let all = list_for_run(&conn, run_id).unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].read);
    }

    #[test]
    fn test_broadcast_all() {
        let (_dir, conn) = setup_test_db();
        let run_id = "run_sig3";
        insert_test_run(&conn, run_id);
        insert_running_session(&conn, "sess_a1", run_id, "architect");
        insert_running_session(&conn, "sess_b1", run_id, "builder");
        insert_running_session(&conn, "sess_t1", run_id, "tester");

        let ids = broadcast(
            &conn,
            run_id,
            "architect",
            GROUP_ALL,
            SignalType::WorkerDone,
            SignalPriority::Normal,
            serde_json::json!({}),
        )
        .unwrap();

        // Should exclude sender (architect) from recipients
        assert_eq!(ids.len(), 2);

        let builder_signals = check_signals(&conn, run_id, "builder").unwrap();
        assert_eq!(builder_signals.len(), 1);
    }

    #[test]
    fn test_broadcast_excludes_sender() {
        let (_dir, conn) = setup_test_db();
        let run_id = "run_sig4";
        insert_test_run(&conn, run_id);
        insert_running_session(&conn, "sess_only", run_id, "builder");

        let ids = broadcast(
            &conn,
            run_id,
            "builder",
            GROUP_ALL,
            SignalType::WorkerDone,
            SignalPriority::Normal,
            serde_json::json!({}),
        )
        .unwrap();

        // Only agent is sender, so no signals created
        assert!(ids.is_empty());
    }

    #[test]
    fn test_signal_type_roundtrip() {
        assert_eq!(
            SignalType::parse("worker_done"),
            Some(SignalType::WorkerDone)
        );
        assert_eq!(SignalType::parse("bogus"), None);
        assert_eq!(SignalType::WorkerDone.as_str(), "worker_done");

        assert_eq!(SignalPriority::parse("urgent"), SignalPriority::Urgent);
        assert_eq!(SignalPriority::parse("unknown"), SignalPriority::Normal);
    }
}
