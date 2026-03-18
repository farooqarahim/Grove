//! Q&A source implementations for the engine layer.
//!
//! When an agent emits a blocking question during streaming execution, the
//! engine needs a [`QaSource`] to retrieve the user's answer. This module
//! provides two concrete implementations:
//!
//! - [`DbQaSource`]: Polls the `qa_messages` DB table for answers (GUI mode).
//! - [`CliQaSource`]: Reads answers from stdin (CLI interactive mode).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::db::repositories::qa_messages_repo;
use crate::errors::{GroveError, GroveResult};
use crate::orchestrator::abort_handle::AbortHandle;
use crate::providers::QaSource;

// ── DB-backed Q&A source (GUI) ──────────────────────────────────────────────

/// Polls the `qa_messages` DB table for user answers.
///
/// Used by the GUI: when a blocking question is detected, `DbQaSource`
/// inserts the question into `qa_messages` (so the frontend can render it),
/// then polls for an `"answer"` row until one appears or the timeout expires.
pub struct DbQaSource {
    db_path: PathBuf,
    /// How often to check the DB for a new answer.
    poll_interval: Duration,
    /// Maximum time to wait for an answer before giving up.
    timeout: Duration,
    /// Optional abort handle — when set, the poll loop bails immediately on abort.
    abort_handle: Option<AbortHandle>,
}

impl DbQaSource {
    /// Create a new DB-backed Q&A source.
    ///
    /// `db_path` is the SQLite database file path. The source opens its own
    /// connection on each `wait_for_answer` call (short-lived, avoids holding
    /// a connection across long waits).
    pub fn new(db_path: impl AsRef<Path>) -> Self {
        Self {
            db_path: db_path.as_ref().to_path_buf(),
            poll_interval: Duration::from_secs(1),
            timeout: Duration::from_secs(120), // 2 min default
            abort_handle: None,
        }
    }

    /// Override the poll interval (default: 1 second).
    #[allow(dead_code)]
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Override the timeout (default: 2 minutes).
    #[allow(dead_code)]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Attach an abort handle so the polling loop can bail early on abort.
    pub fn with_abort_handle(mut self, handle: AbortHandle) -> Self {
        self.abort_handle = Some(handle);
        self
    }
}

impl QaSource for DbQaSource {
    fn wait_for_answer(
        &self,
        run_id: &str,
        session_id: Option<&str>,
        question: &str,
        options: &[String],
    ) -> GroveResult<String> {
        let handle = crate::db::DbHandle::from_db_path(self.db_path.clone());
        let conn = handle.connect()?;

        // Serialize options as JSON for storage.
        let options_json = if options.is_empty() {
            None
        } else {
            Some(serde_json::to_string(options).unwrap_or_default())
        };

        // Insert the question so the frontend knows to display it.
        let question_id = qa_messages_repo::insert(
            &conn,
            run_id,
            session_id,
            "question",
            question,
            options_json.as_deref(),
        )?;

        tracing::info!(
            run_id = %run_id,
            question_id = question_id,
            question = %question,
            "Q&A: inserted blocking question — polling for answer"
        );

        // Poll for an answer row that was inserted after our question.
        let start = Instant::now();
        loop {
            if let Some(ref h) = self.abort_handle {
                if h.is_aborted() {
                    tracing::info!(run_id = %run_id, "Q&A: abort detected — returning early");
                    return Err(GroveError::Aborted);
                }
            }

            if start.elapsed() > self.timeout {
                tracing::warn!(
                    run_id = %run_id,
                    question_id = question_id,
                    timeout_secs = self.timeout.as_secs(),
                    "Q&A: timed out waiting for answer"
                );
                return Ok("(question timed out — no answer received)".to_string());
            }

            let messages = qa_messages_repo::list_for_run(&conn, run_id)?;

            // Find the first answer with an id greater than the question's id.
            // This ensures we match the answer to this specific question even
            // if multiple Q&A rounds happen in the same run.
            if let Some(answer) = messages
                .iter()
                .find(|m| m.direction == "answer" && m.id > question_id)
            {
                tracing::info!(
                    run_id = %run_id,
                    answer_id = answer.id,
                    "Q&A: received answer from user"
                );
                return Ok(answer.content.clone());
            }

            std::thread::sleep(self.poll_interval);
        }
    }
}

// ── CLI stdin Q&A source ────────────────────────────────────────────────────

/// Reads answers from stdin for CLI interactive mode.
///
/// Prints the question and options to stderr, then reads a line from stdin.
pub struct CliQaSource;

impl QaSource for CliQaSource {
    fn wait_for_answer(
        &self,
        _run_id: &str,
        _session_id: Option<&str>,
        question: &str,
        options: &[String],
    ) -> GroveResult<String> {
        eprintln!("\n[AGENT QUESTION] {}", question);
        if !options.is_empty() {
            for (i, opt) in options.iter().enumerate() {
                eprintln!("  {}. {}", i + 1, opt);
            }
        }
        eprint!("> ");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).map_err(|e| {
            GroveError::Runtime(format!("failed to read Q&A input from stdin: {e}"))
        })?;

        let trimmed = input.trim().to_string();
        if trimmed.is_empty() {
            return Ok("(no answer provided)".to_string());
        }

        // If the user typed a number and we have options, resolve to the option text.
        if !options.is_empty() {
            if let Ok(idx) = trimmed.parse::<usize>() {
                if idx >= 1 && idx <= options.len() {
                    return Ok(options[idx - 1].clone());
                }
            }
        }

        Ok(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::NoQaSource;

    #[test]
    fn no_qa_source_returns_empty() {
        let source = NoQaSource;
        let result = source
            .wait_for_answer("run-1", None, "What color?", &["red".into(), "blue".into()])
            .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn db_qa_source_aborts_immediately() {
        let dir = tempfile::tempdir().unwrap();
        // Use db::initialize to create a fully-migrated DB (includes qa_messages table).
        let _ = crate::db::initialize(dir.path()).unwrap();
        let db_path = crate::db::db_path(dir.path());

        let abort = crate::orchestrator::abort_handle::AbortHandle::new();
        abort.abort(); // pre-abort

        let source = DbQaSource::new(&db_path)
            .with_abort_handle(abort)
            .with_timeout(Duration::from_secs(30));

        let result = source.wait_for_answer("run-1", None, "Pick a color", &[]);
        assert!(result.is_err());
        match result.unwrap_err() {
            GroveError::Aborted => {} // expected
            other => panic!("expected Aborted, got: {:?}", other),
        }
    }

    #[test]
    fn default_timeout_is_120_seconds() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let source = DbQaSource::new(&db_path);
        assert_eq!(source.timeout, Duration::from_secs(120));
    }
}
