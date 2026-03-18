//! Test utilities for database access.
//!
//! `TestDb` creates a temporary SQLite database with all migrations applied,
//! suitable for integration tests that need a real (not mocked) database.
//!
//! # Example
//!
//! ```rust,ignore
//! use grove_core::db::test_helpers::TestDb;
//!
//! let test_db = TestDb::new();
//! let conn = test_db.conn();
//! // use conn for queries...
//! ```

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use tempfile::TempDir;

use super::DbHandle;

/// RAII wrapper: creates a temp directory with an initialized grove DB.
///
/// The temp directory (and database) is automatically deleted when `TestDb`
/// is dropped. Open connections must be closed before dropping `TestDb` to
/// avoid dangling file descriptors on Windows.
pub struct TestDb {
    // Held for its Drop impl — deletes the temp directory on scope exit.
    _dir: TempDir,
    db_path: PathBuf,
}

impl TestDb {
    /// Create a new test database with all migrations applied.
    ///
    /// Panics if temp-dir creation or DB initialization fails (acceptable in
    /// test code where a panic is the right failure signal).
    pub fn new() -> Self {
        let dir = TempDir::new().expect("failed to create temp dir for TestDb");
        super::initialize(dir.path()).expect("failed to initialize test DB");
        let db_path = crate::config::paths::db_path(dir.path());
        Self { _dir: dir, db_path }
    }

    /// Open a fresh connection to the test database.
    ///
    /// Uses `connection::open`, which applies all required PRAGMAs (WAL mode,
    /// foreign keys, busy timeout, etc.) before returning the connection.
    pub fn conn(&self) -> Connection {
        DbHandle::from_db_path(self.db_path.clone())
            .connect()
            .expect("failed to open test DB connection")
    }

    /// Return the project root path (the temp directory).
    pub fn project_root(&self) -> &Path {
        self._dir.path()
    }

    /// Return the database file path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_provides_initialized_connection() {
        let test_db = TestDb::new();
        let conn = test_db.conn();
        let version: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key='schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!version.is_empty(), "schema_version must not be empty");
        let version_num: i64 = version.parse().expect("schema_version must be numeric");
        assert!(version_num > 0, "schema_version must be positive");
    }

    #[test]
    fn test_db_project_root_exists() {
        let test_db = TestDb::new();
        assert!(test_db.project_root().exists(), "project root must exist");
    }

    #[test]
    fn test_db_path_ends_with_grove_db() {
        let test_db = TestDb::new();
        assert!(
            test_db.db_path().ends_with(".grove/grove.db"),
            "db_path must end with .grove/grove.db, got: {}",
            test_db.db_path().display()
        );
    }

    #[test]
    fn test_db_multiple_connections_are_independent() {
        let test_db = TestDb::new();
        let conn1 = test_db.conn();
        let conn2 = test_db.conn();
        // Both connections should read the same schema version.
        let v1: i64 = conn1
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let v2: i64 = conn2
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v1, v2, "both connections must see the same schema version");
    }

    #[test]
    fn test_db_instances_are_isolated() {
        // Each TestDb gets its own temp directory — data written to one must
        // not appear in another.
        let db_a = TestDb::new();
        let db_b = TestDb::new();
        assert_ne!(
            db_a.db_path(),
            db_b.db_path(),
            "each TestDb must use a distinct database file"
        );
    }
}
