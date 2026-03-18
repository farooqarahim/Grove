use std::path::Path;

use rusqlite::Connection;

use crate::errors::GroveResult;

use super::pragma;

/// Open a SQLite connection at `path` with all required PRAGMAs applied.
/// The parent directory must already exist (created by `db::initialize`).
pub fn open(path: &Path) -> GroveResult<Connection> {
    let conn = Connection::open(path)?;
    pragma::apply(&conn)?;
    Ok(conn)
}
