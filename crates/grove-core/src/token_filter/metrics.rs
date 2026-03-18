//! Token filter metrics — read session stats and persist them to the database.

use std::path::Path;

use rusqlite::Connection;

use super::session::FilterState;
use crate::errors::GroveResult;

/// Read filter statistics from the session state file.
pub fn read_stats(worktree: &Path) -> Option<FilterState> {
    let state_file = worktree.join(".grove-filter-state.json");
    FilterState::load(&state_file)
}

/// Bulk-insert command stats from the filter session into the database.
pub fn record_to_db(conn: &Connection, run_id: &str, state: &FilterState) -> GroveResult<usize> {
    let mut count = 0usize;
    let mut stmt = conn.prepare_cached(
        "INSERT INTO token_filter_stats \
         (run_id, session_id, command, filter_type, raw_bytes, filtered_bytes, compression_level) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;

    for stat in &state.stats {
        stmt.execute(rusqlite::params![
            run_id,
            state.run_id,
            stat.command,
            stat.filter_type,
            stat.raw_bytes as i64,
            stat.filtered_bytes as i64,
            stat.compression_level as i64,
        ])?;
        count += 1;
    }

    tracing::info!(
        run_id = %run_id,
        commands = count,
        total_raw = state.stats.iter().map(|s| s.raw_bytes).sum::<usize>(),
        total_filtered = state.stats.iter().map(|s| s.filtered_bytes).sum::<usize>(),
        "recorded token filter stats"
    );

    Ok(count)
}

/// Compute aggregate savings for a run from the database.
pub fn query_run_savings(conn: &Connection, run_id: &str) -> GroveResult<TokenSavings> {
    let mut stmt = conn.prepare_cached(
        "SELECT \
             COALESCE(SUM(raw_bytes), 0), \
             COALESCE(SUM(filtered_bytes), 0) \
         FROM token_filter_stats WHERE run_id = ?1",
    )?;

    let (raw, filtered): (i64, i64) = stmt.query_row(rusqlite::params![run_id], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;

    let savings_pct = if raw > 0 {
        (1.0 - filtered as f64 / raw as f64) * 100.0
    } else {
        0.0
    };

    Ok(TokenSavings {
        raw_bytes: raw,
        filtered_bytes: filtered,
        savings_pct,
    })
}

/// Aggregate token savings for a run.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TokenSavings {
    pub raw_bytes: i64,
    pub filtered_bytes: i64,
    pub savings_pct: f64,
}

/// Per-filter-type breakdown for a run.
pub fn query_run_savings_by_type(
    conn: &Connection,
    run_id: &str,
) -> GroveResult<Vec<FilterTypeStat>> {
    let mut stmt = conn.prepare_cached(
        "SELECT filter_type, \
             SUM(raw_bytes) AS raw, \
             SUM(filtered_bytes) AS filtered, \
             COUNT(*) AS invocations \
         FROM token_filter_stats \
         WHERE run_id = ?1 \
         GROUP BY filter_type \
         ORDER BY raw DESC",
    )?;

    let rows = stmt.query_map(rusqlite::params![run_id], |row| {
        Ok(FilterTypeStat {
            filter_type: row.get(0)?,
            raw_bytes: row.get(1)?,
            filtered_bytes: row.get(2)?,
            invocations: row.get(3)?,
        })
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Statistics for a single filter type within a run.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FilterTypeStat {
    pub filter_type: String,
    pub raw_bytes: i64,
    pub filtered_bytes: i64,
    pub invocations: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_filter::session::CommandStat;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE runs (id TEXT PRIMARY KEY);
             INSERT INTO runs (id) VALUES ('run-1');
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT);
             INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', '56');",
        )
        .unwrap();
        conn.execute_batch(include_str!("../../../../migrations/0004_token_filter.sql"))
            .unwrap();
        conn
    }

    #[test]
    fn record_and_query() {
        let conn = setup_db();
        let state = FilterState::new("run-1".into(), vec![], 200_000);
        let mut state_with_stats = state;
        state_with_stats.stats.push(CommandStat {
            command: "git diff".into(),
            filter_type: "git".into(),
            raw_bytes: 10_000,
            filtered_bytes: 3_000,
            compression_level: 1,
        });
        state_with_stats.stats.push(CommandStat {
            command: "cargo test".into(),
            filter_type: "cargo".into(),
            raw_bytes: 50_000,
            filtered_bytes: 5_000,
            compression_level: 2,
        });

        let count = record_to_db(&conn, "run-1", &state_with_stats).unwrap();
        assert_eq!(count, 2);

        let savings = query_run_savings(&conn, "run-1").unwrap();
        assert_eq!(savings.raw_bytes, 60_000);
        assert_eq!(savings.filtered_bytes, 8_000);
        assert!(savings.savings_pct > 80.0);

        let by_type = query_run_savings_by_type(&conn, "run-1").unwrap();
        assert_eq!(by_type.len(), 2);
    }
}
