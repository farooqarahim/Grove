use rusqlite::{Connection, OptionalExtension, params};

use crate::errors::GroveResult;

/// Read the current schema version. Returns 0 if the meta table has no entry yet.
pub fn get_schema_version(conn: &Connection) -> GroveResult<i64> {
    let v: Option<i64> = conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(v.unwrap_or(0))
}

/// Persist the schema version.
pub fn set_schema_version(conn: &Connection, version: i64) -> GroveResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES ('schema_version', ?1)",
        [version.to_string()],
    )?;
    Ok(())
}

/// Read an arbitrary meta value by key.
pub fn get_value(conn: &Connection, key: &str) -> GroveResult<Option<String>> {
    let v: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
        .optional()?;
    Ok(v)
}

/// Write an arbitrary meta value.
pub fn set_value(conn: &Connection, key: &str, value: &str) -> GroveResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}
