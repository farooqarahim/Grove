use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

pub mod markdown;
pub mod report_model;

pub use markdown::render_markdown;
pub use report_model::RunReport;

use crate::config::grove_dir;
use crate::db::DbHandle;
use crate::errors::GroveResult;

pub fn report_path(project_root: &Path, run_id: &str) -> PathBuf {
    grove_dir(project_root)
        .join("reports")
        .join(format!("{run_id}.json"))
}

/// Build a full `RunReport` from the DB for the given `run_id`.
pub fn build_report(conn: &Connection, run_id: &str) -> GroveResult<RunReport> {
    RunReport::from_db(conn, run_id)
}

/// Generate a JSON report file for `run_id` and return its path.
pub fn generate_report(project_root: &Path, run_id: &str) -> GroveResult<PathBuf> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    generate_report_with_conn(&conn, project_root, run_id)
}

/// Generate a JSON report using an existing DB connection.
///
/// `data_root` is the directory whose `.grove/reports/` will hold the output.
/// In centralized mode this is the virtual workspace root; in CLI mode it is
/// the project root.
pub fn generate_report_with_conn(
    conn: &Connection,
    data_root: &Path,
    run_id: &str,
) -> GroveResult<PathBuf> {
    let reports_dir = grove_dir(data_root).join("reports");
    fs::create_dir_all(&reports_dir)?;

    let report = RunReport::from_db(conn, run_id)?;
    let body = serde_json::to_string_pretty(&report)?;

    let path = report_path(data_root, run_id);
    fs::write(&path, body)?;
    Ok(path)
}
