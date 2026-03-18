/// Verdict parsing for agent report files.
///
/// Two verdict types remain:
/// - Reviewer → PASS | FAIL (parsed from GROVE_REVIEW_{run_id}.md)
/// - Judge    → APPROVED | NEEDS_WORK | REJECTED (parsed from GROVE_VERDICT_{run_id}.md)
///
/// Artifacts live in `.grove/artifacts/{conversation_id}/{run_id}/`, not in the worktree.
use std::path::Path;

use serde::{Deserialize, Serialize};

// ── Unified verdict type ────────────────────────────────────────────────────

/// A verdict produced by a Reviewer or Judge agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    pub outcome: VerdictOutcome,
    pub summary: String,
    pub issues: Vec<VerdictIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictOutcome {
    Pass,
    Fail,
    Approved,
    NeedsWork,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictIssue {
    pub severity: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub description: String,
}

// ── Legacy verdict enums (kept for engine.rs compatibility during migration) ─

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewVerdict {
    Pass,
    Fail { feedback: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JudgeVerdict {
    Approved,
    NeedsWork { notes: String },
    Rejected { notes: String },
}

// ── Parsing ─────────────────────────────────────────────────────────────────

/// Parse the reviewer's report file for a PASS/FAIL verdict.
///
/// `artifacts_dir` is the `.grove/artifacts/{conversation_id}/{run_id}/` directory.
/// Falls back to checking the worktree for legacy files.
pub fn parse_review_verdict(artifacts_dir: &Path, run_id: &str) -> Option<ReviewVerdict> {
    let short_id = if run_id.len() >= 8 {
        &run_id[..8]
    } else {
        run_id
    };

    // Check artifacts directory first, fall back to legacy worktree names
    let primary_path = artifacts_dir.join(format!("GROVE_REVIEW_{short_id}.md"));
    let legacy_path = artifacts_dir.join(format!("REVIEW_{run_id}.md"));

    let content = std::fs::read_to_string(&primary_path)
        .or_else(|_| std::fs::read_to_string(&legacy_path))
        .ok()?;

    parse_review_verdict_from_str(&content)
}

pub fn parse_review_verdict_from_str(content: &str) -> Option<ReviewVerdict> {
    for line in content.lines() {
        let upper = line.to_uppercase();
        if upper.contains("VERDICT") {
            if upper.contains("FAIL") {
                let feedback = extract_feedback_after(content, "VERDICT");
                return Some(ReviewVerdict::Fail { feedback });
            }
            if upper.contains("PASS") {
                return Some(ReviewVerdict::Pass);
            }
        }
    }
    None
}

/// Parse the judge's verdict file for APPROVED/NEEDS_WORK/REJECTED.
///
/// `artifacts_dir` is the `.grove/artifacts/{conversation_id}/{run_id}/` directory.
pub fn parse_judge_verdict(artifacts_dir: &Path, run_id: &str) -> Option<JudgeVerdict> {
    let short_id = if run_id.len() >= 8 {
        &run_id[..8]
    } else {
        run_id
    };

    let primary_path = artifacts_dir.join(format!("GROVE_VERDICT_{short_id}.md"));
    let legacy_path = artifacts_dir.join(format!("JUDGE_VERDICT_{run_id}.md"));

    let content = std::fs::read_to_string(&primary_path)
        .or_else(|_| std::fs::read_to_string(&legacy_path))
        .ok()?;

    parse_judge_verdict_from_str(&content)
}

pub fn parse_judge_verdict_from_str(content: &str) -> Option<JudgeVerdict> {
    for line in content.lines() {
        let upper = line.to_uppercase();
        if upper.contains("VERDICT") {
            if upper.contains("REJECTED") {
                let notes = extract_feedback_after(content, "VERDICT");
                return Some(JudgeVerdict::Rejected { notes });
            }
            if upper.contains("NEEDS_WORK") || upper.contains("NEEDS WORK") {
                let notes = extract_feedback_after(content, "VERDICT");
                return Some(JudgeVerdict::NeedsWork { notes });
            }
            if upper.contains("APPROVED") {
                return Some(JudgeVerdict::Approved);
            }
        }
    }
    None
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Extract up to 2000 chars of content after the first line containing `marker`.
fn extract_feedback_after(content: &str, marker: &str) -> String {
    let upper = content.to_uppercase();
    if let Some(pos) = upper.find(marker) {
        let rest = &content[pos..];
        if let Some(newline_pos) = rest.find('\n') {
            let after: String = rest[newline_pos + 1..].chars().take(2000).collect();
            let trimmed = after.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_verdict_pass() {
        let content = "## Summary\nLooks good.\n\n## VERDICT: PASS\n";
        assert_eq!(
            parse_review_verdict_from_str(content),
            Some(ReviewVerdict::Pass)
        );
    }

    #[test]
    fn review_verdict_fail_with_feedback() {
        let content =
            "## Summary\nBad code.\n\n## VERDICT: FAIL\n\nRemove the SQL injection on line 42.";
        let verdict = parse_review_verdict_from_str(content).unwrap();
        assert!(matches!(verdict, ReviewVerdict::Fail { .. }));
        if let ReviewVerdict::Fail { feedback } = verdict {
            assert!(feedback.contains("SQL injection"));
        }
    }

    #[test]
    fn review_verdict_missing_returns_none() {
        let content = "## Summary\nNo verdict here.";
        assert_eq!(parse_review_verdict_from_str(content), None);
    }

    #[test]
    fn judge_verdict_approved() {
        let content = "## Overall Assessment\nExcellent work.\n\n## VERDICT: APPROVED\n";
        assert_eq!(
            parse_judge_verdict_from_str(content),
            Some(JudgeVerdict::Approved)
        );
    }

    #[test]
    fn judge_verdict_needs_work() {
        let content = "## Overall Assessment\nClose but issues remain.\n\n## VERDICT: NEEDS_WORK\nFix the auth layer.";
        let verdict = parse_judge_verdict_from_str(content).unwrap();
        assert!(matches!(verdict, JudgeVerdict::NeedsWork { .. }));
        if let JudgeVerdict::NeedsWork { notes } = verdict {
            assert!(notes.contains("auth layer"));
        }
    }

    #[test]
    fn judge_verdict_rejected() {
        let content = "## VERDICT: REJECTED\nObjective was not met at all.";
        let verdict = parse_judge_verdict_from_str(content).unwrap();
        assert!(matches!(verdict, JudgeVerdict::Rejected { .. }));
    }

    #[test]
    fn judge_verdict_missing_returns_none() {
        let content = "## Overall Assessment\nNo verdict here.";
        assert_eq!(parse_judge_verdict_from_str(content), None);
    }
}
