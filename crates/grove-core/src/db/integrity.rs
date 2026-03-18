use rusqlite::Connection;

use crate::errors::{GroveError, GroveResult};

#[derive(Debug)]
pub struct IntegrityReport {
    pub integrity_ok: bool,
    pub integrity_detail: String,
    pub foreign_key_violations: Vec<FkViolation>,
}

#[derive(Debug)]
pub struct FkViolation {
    pub table: String,
    pub rowid: i64,
    pub parent_table: String,
    pub fk_id: i64,
}

/// Run `PRAGMA integrity_check` and `PRAGMA foreign_key_check`.
/// Returns `Ok(report)` on success; the caller decides whether violations are fatal.
pub fn check(conn: &Connection) -> GroveResult<IntegrityReport> {
    // integrity_check returns one row per problem; first row is "ok" when clean.
    let mut stmt = conn.prepare("PRAGMA integrity_check")?;
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;

    let integrity_ok = rows.len() == 1 && rows[0] == "ok";
    let integrity_detail = rows.join("; ");

    // foreign_key_check returns one row per violation.
    let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
    let violations: Vec<FkViolation> = stmt
        .query_map([], |r| {
            Ok(FkViolation {
                table: r.get(0)?,
                rowid: r.get(1)?,
                parent_table: r.get(2)?,
                fk_id: r.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    Ok(IntegrityReport {
        integrity_ok,
        integrity_detail,
        foreign_key_violations: violations,
    })
}

/// Like `check` but returns `Err` if either check finds problems.
pub fn assert_healthy(conn: &Connection) -> GroveResult<()> {
    let report = check(conn)?;
    if !report.integrity_ok {
        return Err(GroveError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
            Some(report.integrity_detail),
        )));
    }
    if !report.foreign_key_violations.is_empty() {
        let detail = report
            .foreign_key_violations
            .iter()
            .map(|v| format!("{}(rowid={}) -> {}", v.table, v.rowid, v.parent_table))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(GroveError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
            Some(format!("foreign key violations: {detail}")),
        )));
    }
    Ok(())
}
