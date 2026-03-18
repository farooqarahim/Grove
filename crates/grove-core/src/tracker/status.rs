use serde::{Deserialize, Serialize};

/// Canonical status buckets used for kanban column grouping.
///
/// Each external provider uses its own status strings (e.g. Jira "In Progress",
/// Linear "Todo"). `CanonicalStatus` is the single, normalised enum that the
/// board, sync engine, and write-back layer all operate on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalStatus {
    Open,
    InProgress,
    InReview,
    Blocked,
    Done,
    Cancelled,
}

impl CanonicalStatus {
    /// Serialise to the DB string stored in `issues.canonical_status`.
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::InReview => "in_review",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    /// Deserialise from a DB string.  Returns `None` for unrecognised values.
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "in_progress" => Some(Self::InProgress),
            "in_review" => Some(Self::InReview),
            "blocked" => Some(Self::Blocked),
            "done" => Some(Self::Done),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Human-readable label shown in the kanban column header.
    pub fn display_label(&self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::InProgress => "In Progress",
            Self::InReview => "In Review",
            Self::Blocked => "Blocked",
            Self::Done => "Done",
            Self::Cancelled => "Cancelled",
        }
    }

    /// Ordered list of all variants — used to render kanban columns in the
    /// correct left-to-right sequence.
    pub fn ordered() -> &'static [CanonicalStatus] {
        &[
            Self::Open,
            Self::InProgress,
            Self::InReview,
            Self::Blocked,
            Self::Done,
            Self::Cancelled,
        ]
    }
}

/// Map a provider-specific raw status string to a `CanonicalStatus`.
///
/// Comparison is case-insensitive so that minor provider variations
/// ("IN PROGRESS", "in progress") are handled without separate entries.
/// Unknown statuses fall back to `Open` rather than panicking.
pub fn normalize(provider: &str, raw_status: &str) -> CanonicalStatus {
    let s = raw_status.trim().to_ascii_lowercase();
    match provider {
        "github" => normalize_github(&s),
        "jira" => normalize_jira(&s),
        "linear" => normalize_linear(&s),
        // grove-native issues and linter-generated issues use GitHub-style strings.
        "grove" | "linter" | "external" => normalize_github(&s),
        _ => normalize_fallback(&s),
    }
}

/// Map a `CanonicalStatus` back to the provider's preferred status string.
///
/// Used by the write-back engine when transitioning issues on the external
/// tracker.  Returns the most common / default transition target for each
/// provider when an exact match is unavailable.
pub fn denormalize(provider: &str, canonical: &CanonicalStatus) -> &'static str {
    match provider {
        "github" => denormalize_github(canonical),
        "jira" => denormalize_jira(canonical),
        "linear" => denormalize_linear(canonical),
        "grove" | "linter" | "external" => denormalize_github(canonical),
        _ => canonical.as_db_str(),
    }
}

// ── Provider-specific normalizers ────────────────────────────────────────────

fn normalize_github(s: &str) -> CanonicalStatus {
    match s {
        "open" => CanonicalStatus::Open,
        "closed" => CanonicalStatus::Done,
        _ => normalize_fallback(s),
    }
}

fn normalize_jira(s: &str) -> CanonicalStatus {
    match s {
        "to do" | "open" | "backlog" | "new" | "selected for development" => CanonicalStatus::Open,
        "in progress" | "in development" | "in design" => CanonicalStatus::InProgress,
        "in review" | "code review" | "peer review" | "qa" | "testing" | "in testing" | "in qa" => {
            CanonicalStatus::InReview
        }
        "blocked" | "on hold" | "waiting" | "impediment" => CanonicalStatus::Blocked,
        "done" | "closed" | "resolved" | "fixed" | "complete" | "completed" => {
            CanonicalStatus::Done
        }
        "cancelled" | "canceled" | "won't do" | "wont do" | "invalid" | "duplicate" => {
            CanonicalStatus::Cancelled
        }
        _ => normalize_fallback(s),
    }
}

fn normalize_linear(s: &str) -> CanonicalStatus {
    match s {
        "todo" | "backlog" | "triage" => CanonicalStatus::Open,
        "in progress" | "started" => CanonicalStatus::InProgress,
        "in review" => CanonicalStatus::InReview,
        "blocked" => CanonicalStatus::Blocked,
        "done" | "completed" => CanonicalStatus::Done,
        "cancelled" | "canceled" | "duplicate" => CanonicalStatus::Cancelled,
        _ => normalize_fallback(s),
    }
}

/// Best-effort fuzzy match for unknown providers or unrecognised status strings.
fn normalize_fallback(s: &str) -> CanonicalStatus {
    if s.contains("progress") || s.contains("started") || s.contains("active") {
        CanonicalStatus::InProgress
    } else if s.contains("review") || s.contains("testing") || s.contains("qa") {
        CanonicalStatus::InReview
    } else if s.contains("block") || s.contains("hold") || s.contains("wait") {
        CanonicalStatus::Blocked
    } else if s.contains("done")
        || s.contains("closed")
        || s.contains("resolved")
        || s.contains("complet")
        || s.contains("fixed")
    {
        CanonicalStatus::Done
    } else if s.contains("cancel") || s.contains("duplicate") || s.contains("wont") {
        CanonicalStatus::Cancelled
    } else {
        // Default: treat anything unrecognised as open so issues are never lost.
        CanonicalStatus::Open
    }
}

// ── Provider-specific denormalizers ──────────────────────────────────────────

fn denormalize_github(canonical: &CanonicalStatus) -> &'static str {
    match canonical {
        CanonicalStatus::Open
        | CanonicalStatus::InProgress
        | CanonicalStatus::InReview
        | CanonicalStatus::Blocked => "open",
        CanonicalStatus::Done => "closed",
        CanonicalStatus::Cancelled => "closed",
    }
}

fn denormalize_jira(canonical: &CanonicalStatus) -> &'static str {
    match canonical {
        CanonicalStatus::Open => "To Do",
        CanonicalStatus::InProgress => "In Progress",
        CanonicalStatus::InReview => "In Review",
        CanonicalStatus::Blocked => "Blocked",
        CanonicalStatus::Done => "Done",
        CanonicalStatus::Cancelled => "Cancelled",
    }
}

fn denormalize_linear(canonical: &CanonicalStatus) -> &'static str {
    match canonical {
        CanonicalStatus::Open => "Todo",
        CanonicalStatus::InProgress => "In Progress",
        CanonicalStatus::InReview => "In Review",
        CanonicalStatus::Blocked => "Blocked",
        CanonicalStatus::Done => "Done",
        CanonicalStatus::Cancelled => "Cancelled",
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── GitHub ────────────────────────────────────────────────────────────────

    #[test]
    fn github_open_maps_to_open() {
        assert_eq!(normalize("github", "open"), CanonicalStatus::Open);
    }

    #[test]
    fn github_closed_maps_to_done() {
        assert_eq!(normalize("github", "closed"), CanonicalStatus::Done);
    }

    #[test]
    fn github_unknown_falls_back_to_open() {
        assert_eq!(normalize("github", "weird_custom"), CanonicalStatus::Open);
    }

    // ── Jira ──────────────────────────────────────────────────────────────────

    #[test]
    fn jira_to_do_maps_to_open() {
        assert_eq!(normalize("jira", "To Do"), CanonicalStatus::Open);
    }

    #[test]
    fn jira_in_progress_maps_to_in_progress() {
        assert_eq!(
            normalize("jira", "In Progress"),
            CanonicalStatus::InProgress
        );
    }

    #[test]
    fn jira_in_review_maps_to_in_review() {
        assert_eq!(normalize("jira", "In Review"), CanonicalStatus::InReview);
    }

    #[test]
    fn jira_blocked_maps_to_blocked() {
        assert_eq!(normalize("jira", "Blocked"), CanonicalStatus::Blocked);
    }

    #[test]
    fn jira_done_maps_to_done() {
        assert_eq!(normalize("jira", "Done"), CanonicalStatus::Done);
    }

    #[test]
    fn jira_wont_do_maps_to_cancelled() {
        assert_eq!(normalize("jira", "Won't Do"), CanonicalStatus::Cancelled);
    }

    #[test]
    fn jira_resolved_maps_to_done() {
        assert_eq!(normalize("jira", "Resolved"), CanonicalStatus::Done);
    }

    // ── Linear ────────────────────────────────────────────────────────────────

    #[test]
    fn linear_todo_maps_to_open() {
        assert_eq!(normalize("linear", "Todo"), CanonicalStatus::Open);
    }

    #[test]
    fn linear_in_progress_maps_to_in_progress() {
        assert_eq!(
            normalize("linear", "In Progress"),
            CanonicalStatus::InProgress
        );
    }

    #[test]
    fn linear_in_review_maps_to_in_review() {
        assert_eq!(normalize("linear", "In Review"), CanonicalStatus::InReview);
    }

    #[test]
    fn linear_done_maps_to_done() {
        assert_eq!(normalize("linear", "Done"), CanonicalStatus::Done);
    }

    #[test]
    fn linear_cancelled_maps_to_cancelled() {
        assert_eq!(normalize("linear", "Cancelled"), CanonicalStatus::Cancelled);
    }

    #[test]
    fn linear_duplicate_maps_to_cancelled() {
        assert_eq!(normalize("linear", "Duplicate"), CanonicalStatus::Cancelled);
    }

    // ── Fallback / unknown provider ───────────────────────────────────────────

    #[test]
    fn unknown_provider_with_in_progress_substring() {
        assert_eq!(
            normalize("custom_tracker", "IN PROGRESS"),
            CanonicalStatus::InProgress
        );
    }

    #[test]
    fn unknown_provider_with_completely_unknown_status() {
        assert_eq!(normalize("custom_tracker", "limbo"), CanonicalStatus::Open);
    }

    // ── Denormalize round-trips ───────────────────────────────────────────────

    #[test]
    fn github_done_denormalizes_to_closed() {
        assert_eq!(denormalize("github", &CanonicalStatus::Done), "closed");
    }

    #[test]
    fn github_open_denormalizes_to_open() {
        assert_eq!(denormalize("github", &CanonicalStatus::Open), "open");
    }

    #[test]
    fn jira_denormalize_in_progress() {
        assert_eq!(
            denormalize("jira", &CanonicalStatus::InProgress),
            "In Progress"
        );
    }

    #[test]
    fn linear_denormalize_todo() {
        assert_eq!(denormalize("linear", &CanonicalStatus::Open), "Todo");
    }

    // ── DB round-trip ─────────────────────────────────────────────────────────

    #[test]
    fn db_round_trip_all_variants() {
        for variant in [
            CanonicalStatus::Open,
            CanonicalStatus::InProgress,
            CanonicalStatus::InReview,
            CanonicalStatus::Blocked,
            CanonicalStatus::Done,
            CanonicalStatus::Cancelled,
        ] {
            let db_str = variant.as_db_str();
            let parsed = CanonicalStatus::from_db_str(db_str)
                .unwrap_or_else(|| panic!("from_db_str failed for '{db_str}'"));
            assert_eq!(parsed, variant);
        }
    }

    #[test]
    fn from_db_str_unknown_returns_none() {
        assert!(CanonicalStatus::from_db_str("bogus").is_none());
    }

    // ── Case insensitivity ────────────────────────────────────────────────────

    #[test]
    fn jira_case_insensitive_in_progress() {
        assert_eq!(
            normalize("jira", "IN PROGRESS"),
            CanonicalStatus::InProgress
        );
    }

    #[test]
    fn linear_case_insensitive_todo() {
        assert_eq!(normalize("linear", "TODO"), CanonicalStatus::Open);
    }
}
