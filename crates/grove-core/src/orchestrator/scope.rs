//! Scope configuration and contract generation for agent execution discipline.
//!
//! Each agent can declare a `ScopeConfig` that restricts which paths it may
//! write to, which paths are blocked, and which artifacts it must produce.
//! The orchestrator enforces these rules pre- and post-execution.

use std::collections::HashMap;
use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

// ── Enums ───────────────────────────────────────────────────────────────────

/// Controls how permission escalation requests are handled when an agent
/// attempts an action outside its declared scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopePermissionMode {
    /// Silently skip the scope check (no enforcement).
    SkipAll,
    /// Pause execution and ask a human to approve/deny.
    HumanGate,
    /// Let an automated gatekeeper agent decide.
    AutonomousGate,
}

/// What happens when a scope violation is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ViolationPolicy {
    /// Revert the violating changes and retry the agent once.
    #[default]
    RetryOnce,
    /// Immediately fail the run.
    HardFail,
    /// Log a warning but allow the run to continue.
    Warn,
}

impl std::fmt::Display for ViolationPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RetryOnce => write!(f, "retry_once"),
            Self::HardFail => write!(f, "hard_fail"),
            Self::Warn => write!(f, "warn"),
        }
    }
}

/// How a required artifact should be validated after agent execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactValidation {
    /// The artifact file must exist on disk.
    Exists,
    /// The artifact must exist AND contain a verdict (pass/fail/etc.).
    Verdict,
}

// ── ScopeConfig ─────────────────────────────────────────────────────────────

/// Per-agent scope restrictions declared in agent config frontmatter.
///
/// When all fields are at their defaults (empty vecs, no permission_mode),
/// the agent is unrestricted.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScopeConfig {
    /// Glob patterns for paths the agent is allowed to write to.
    /// Empty means no path restrictions (all paths writable).
    #[serde(default)]
    pub writable_paths: Vec<String>,

    /// Glob patterns for paths the agent must never touch.
    #[serde(default)]
    pub blocked_paths: Vec<String>,

    /// Override for permission escalation handling within this agent's scope.
    #[serde(default)]
    pub permission_mode: Option<ScopePermissionMode>,

    /// Artifact filenames that must exist after the agent completes.
    /// Supports `{run_id}` and `{short_id}` placeholders.
    #[serde(default)]
    pub required_artifacts: Vec<String>,

    /// Per-artifact validation mode. Keys are artifact filename patterns
    /// (after placeholder resolution).
    #[serde(default)]
    pub artifact_validation: HashMap<String, ArtifactValidation>,

    /// What to do when a scope violation is detected.
    #[serde(default)]
    pub on_violation: ViolationPolicy,
}

impl ScopeConfig {
    /// Returns `true` if this scope config imposes any restrictions at all.
    pub fn has_restrictions(&self) -> bool {
        !self.writable_paths.is_empty()
            || !self.blocked_paths.is_empty()
            || !self.required_artifacts.is_empty()
            || self.permission_mode.is_some()
    }

    /// Resolve `{run_id}` and `{short_id}` placeholders in `required_artifacts`.
    pub fn resolve_artifact_patterns(&self, run_id: &str) -> Vec<String> {
        let short_id = if run_id.len() >= 8 {
            &run_id[..8]
        } else {
            run_id
        };
        self.required_artifacts
            .iter()
            .map(|pat| {
                pat.replace("{run_id}", run_id)
                    .replace("{short_id}", short_id)
            })
            .collect()
    }

    /// Generate a SCOPE CONTRACT instruction block that is injected into the
    /// agent's system prompt. Returns an empty string if the scope has no
    /// restrictions.
    pub fn build_contract(&self, run_id: &str) -> String {
        if !self.has_restrictions() {
            return String::new();
        }

        let mut lines: Vec<String> = Vec::new();

        lines.push(
            "## SCOPE CONTRACT (enforced by orchestrator \u{2014} violations will be reverted)"
                .to_string(),
        );
        lines.push(String::new());

        if !self.writable_paths.is_empty() {
            lines.push("**Writable paths** (only these may be created/modified):".to_string());
            for p in &self.writable_paths {
                lines.push(format!("- `{p}`"));
            }
            lines.push(String::new());
        }

        if !self.blocked_paths.is_empty() {
            lines.push("**Blocked paths** (must NOT be touched):".to_string());
            for p in &self.blocked_paths {
                lines.push(format!("- `{p}`"));
            }
            lines.push(String::new());
        }

        let resolved = self.resolve_artifact_patterns(run_id);
        if !resolved.is_empty() {
            lines.push("**Required artifacts** (must exist after execution):".to_string());
            for a in &resolved {
                lines.push(format!("- `{a}`"));
            }
            lines.push(String::new());
        }

        lines.push(
            "All changes will be validated post-execution. Out-of-scope writes will be reverted."
                .to_string(),
        );

        lines.join("\n")
    }
}

// ── ScopeValidator ──────────────────────────────────────────────────────────

/// A single scope violation.
#[derive(Debug, Clone, Serialize)]
pub struct ScopeViolationEntry {
    pub file: String,
    pub kind: ViolationKind,
}

/// Classification of a scope violation.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViolationKind {
    /// The file matched a blocked-path pattern.
    BlockedPath { pattern: String },
    /// The file is not covered by any writable-path pattern.
    NotInWritablePaths,
    /// A required artifact was not found on disk.
    MissingArtifact,
    /// An artifact requiring a verdict could not be parsed.
    UnparseableVerdict,
}

/// Result of post-execution scope validation.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub passed: bool,
    pub violations: Vec<ScopeViolationEntry>,
}

/// Post-execution scope validator.
///
/// Checks changed files against blocked/writable path rules and verifies
/// that required artifacts exist on disk.
pub struct ScopeValidator;

impl ScopeValidator {
    /// Validate changed files and artifacts against scope rules.
    ///
    /// # Arguments
    /// - `scope` — the agent's `ScopeConfig` defining restrictions.
    /// - `changed_files` — list of file paths (relative to worktree) that
    ///   were created or modified during execution.
    /// - `run_id` — used to resolve `{run_id}` / `{short_id}` placeholders
    ///   in artifact patterns.
    /// - `worktree_path` — absolute path to the worktree root, used to check
    ///   changed file paths.
    /// - `artifacts_dir` — absolute path to the artifacts directory where
    ///   agents write their pipeline artifacts (PRD, Design, Review, Verdict).
    pub fn validate(
        scope: &ScopeConfig,
        changed_files: &[String],
        run_id: &str,
        _worktree_path: &Path,
        artifacts_dir: &Path,
    ) -> ValidationResult {
        let mut violations = Vec::new();

        // 1. Build blocked-path GlobSet
        let blocked_set = Self::build_glob_set(&scope.blocked_paths);

        // 2. Build writable-path GlobSet (only enforced when non-empty)
        let writable_set = if scope.writable_paths.is_empty() {
            None
        } else {
            Some(Self::build_glob_set(&scope.writable_paths))
        };

        // 3. Check each changed file
        for file in changed_files {
            // Blocked paths take precedence: check first
            if blocked_set.is_match(file) {
                // Find the first matching blocked pattern for the report
                let pattern = scope
                    .blocked_paths
                    .iter()
                    .find(|pat| {
                        Glob::new(pat)
                            .ok()
                            .map(|g| g.compile_matcher().is_match(file))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .unwrap_or_default();

                violations.push(ScopeViolationEntry {
                    file: file.clone(),
                    kind: ViolationKind::BlockedPath { pattern },
                });
                continue;
            }

            // If writable_paths is non-empty, the file must match at least one
            if let Some(ref ws) = writable_set {
                if !ws.is_match(file) {
                    violations.push(ScopeViolationEntry {
                        file: file.clone(),
                        kind: ViolationKind::NotInWritablePaths,
                    });
                }
            }
        }

        // 4. Check required artifacts exist on disk (in the artifacts directory)
        let resolved_artifacts = scope.resolve_artifact_patterns(run_id);
        for artifact in &resolved_artifacts {
            let artifact_path = artifacts_dir.join(artifact);
            if !artifact_path.exists() {
                violations.push(ScopeViolationEntry {
                    file: artifact.clone(),
                    kind: ViolationKind::MissingArtifact,
                });
            }
        }

        ValidationResult {
            passed: violations.is_empty(),
            violations,
        }
    }

    /// Build a `GlobSet` from a list of glob pattern strings.
    /// Invalid patterns are logged as warnings and skipped.
    fn build_glob_set(patterns: &[String]) -> GlobSet {
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            match Glob::new(pattern) {
                Ok(glob) => {
                    builder.add(glob);
                }
                Err(e) => {
                    tracing::warn!(
                        pattern = %pattern,
                        error = %e,
                        "invalid glob pattern in scope config — skipping (this may weaken enforcement)"
                    );
                }
            }
        }
        builder.build().unwrap_or_else(|_| {
            GlobSetBuilder::new()
                .build()
                .expect("empty GlobSet should always build")
        })
    }
}

// ── DisciplineConfig ────────────────────────────────────────────────────────

/// Top-level discipline settings in `grove.yaml`.
///
/// Controls global defaults for scope enforcement behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisciplineConfig {
    /// Default violation policy when an agent's `ScopeConfig` doesn't specify one.
    #[serde(default)]
    pub default_on_violation: ViolationPolicy,

    /// Default permission mode when an agent's `ScopeConfig` doesn't specify one.
    #[serde(default = "default_permission_mode")]
    pub default_permission_mode: ScopePermissionMode,

    /// When `true`, artifact validation in `Verdict` mode requires an explicit
    /// pass/fail verdict string. When `false`, mere existence is sufficient.
    #[serde(default = "default_true")]
    pub strict_verdicts: bool,
}

impl Default for DisciplineConfig {
    fn default() -> Self {
        Self {
            default_on_violation: ViolationPolicy::RetryOnce,
            default_permission_mode: ScopePermissionMode::SkipAll,
            strict_verdicts: true,
        }
    }
}

fn default_permission_mode() -> ScopePermissionMode {
    ScopePermissionMode::SkipAll
}

fn default_true() -> bool {
    true
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scope_config_from_yaml() {
        let yaml = r#"
writable_paths:
  - "docs/**"
  - ".grove/artifacts/**"
blocked_paths:
  - "src/**"
  - "Cargo.toml"
permission_mode: human_gate
required_artifacts:
  - "prd-{run_id}.md"
  - "design-{short_id}.md"
artifact_validation:
  "prd-{run_id}.md": verdict
  "design-{short_id}.md": exists
on_violation: hard_fail
"#;
        let scope: ScopeConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(scope.writable_paths.len(), 2);
        assert_eq!(scope.writable_paths[0], "docs/**");
        assert_eq!(scope.blocked_paths.len(), 2);
        assert_eq!(scope.blocked_paths[1], "Cargo.toml");
        assert_eq!(scope.permission_mode, Some(ScopePermissionMode::HumanGate));
        assert_eq!(scope.required_artifacts.len(), 2);
        assert_eq!(
            scope.artifact_validation.get("prd-{run_id}.md"),
            Some(&ArtifactValidation::Verdict)
        );
        assert_eq!(
            scope.artifact_validation.get("design-{short_id}.md"),
            Some(&ArtifactValidation::Exists)
        );
        assert_eq!(scope.on_violation, ViolationPolicy::HardFail);
    }

    #[test]
    fn empty_scope_is_permissive() {
        let scope = ScopeConfig::default();
        assert!(!scope.has_restrictions());
        assert!(scope.writable_paths.is_empty());
        assert!(scope.blocked_paths.is_empty());
        assert!(scope.required_artifacts.is_empty());
        assert!(scope.permission_mode.is_none());
        assert_eq!(scope.on_violation, ViolationPolicy::RetryOnce);
    }

    #[test]
    fn scope_config_absent_means_none() {
        // Simulates an AgentConfig frontmatter without a `scope` block.
        // The `Option<ScopeConfig>` field should deserialize as `None`.
        #[derive(Debug, Deserialize)]
        struct FakeAgentConfig {
            id: String,
            #[serde(default)]
            scope: Option<ScopeConfig>,
        }

        let yaml = r#"
id: builder
"#;
        let cfg: FakeAgentConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.id, "builder");
        assert!(cfg.scope.is_none());
    }

    #[test]
    fn discipline_config_deserializes_from_yaml() {
        let yaml = r#"
default_on_violation: hard_fail
default_permission_mode: autonomous_gate
strict_verdicts: false
"#;
        let dc: DisciplineConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(dc.default_on_violation, ViolationPolicy::HardFail);
        assert_eq!(
            dc.default_permission_mode,
            ScopePermissionMode::AutonomousGate
        );
        assert!(!dc.strict_verdicts);
    }

    #[test]
    fn discipline_config_defaults() {
        let dc = DisciplineConfig::default();
        assert_eq!(dc.default_on_violation, ViolationPolicy::RetryOnce);
        assert_eq!(dc.default_permission_mode, ScopePermissionMode::SkipAll);
        assert!(dc.strict_verdicts);
    }

    #[test]
    fn scope_contract_generates_instruction_block() {
        let scope = ScopeConfig {
            writable_paths: vec!["docs/**".to_string()],
            blocked_paths: vec!["src/**".to_string()],
            required_artifacts: vec!["prd-{run_id}.md".to_string()],
            artifact_validation: HashMap::new(),
            permission_mode: None,
            on_violation: ViolationPolicy::RetryOnce,
        };

        let contract = scope.build_contract("abcdef1234567890");

        assert!(contract.contains("SCOPE CONTRACT"));
        assert!(contract.contains("violations will be reverted"));
        assert!(contract.contains("Writable paths"));
        assert!(contract.contains("`docs/**`"));
        assert!(contract.contains("Blocked paths"));
        assert!(contract.contains("`src/**`"));
        assert!(contract.contains("Required artifacts"));
        assert!(contract.contains("`prd-abcdef1234567890.md`"));
        assert!(contract.contains("post-execution"));
    }

    #[test]
    fn empty_scope_produces_no_contract() {
        let scope = ScopeConfig::default();
        let contract = scope.build_contract("some-run-id");
        assert!(contract.is_empty());
    }

    // ── ScopeValidator tests ────────────────────────────────────────────────

    #[test]
    fn validator_detects_blocked_path_violation() {
        let scope = ScopeConfig {
            blocked_paths: vec!["*.rs".to_string(), "src/**".to_string()],
            ..Default::default()
        };

        let changed = vec!["src/main.rs".to_string()];
        let tmp = tempfile::tempdir().unwrap();
        let result = ScopeValidator::validate(&scope, &changed, "abc12345", tmp.path(), tmp.path());

        assert!(!result.passed);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].file, "src/main.rs");
        match &result.violations[0].kind {
            ViolationKind::BlockedPath { pattern } => {
                // Should match one of the blocked patterns
                assert!(
                    pattern == "*.rs" || pattern == "src/**",
                    "unexpected pattern: {pattern}"
                );
            }
            other => panic!("expected BlockedPath, got {other:?}"),
        }
    }

    #[test]
    fn validator_detects_writable_path_violation() {
        let scope = ScopeConfig {
            writable_paths: vec!["GROVE_PRD_*.md".to_string(), "docs/**".to_string()],
            ..Default::default()
        };

        let changed = vec!["app.ts".to_string()];
        let tmp = tempfile::tempdir().unwrap();
        let result = ScopeValidator::validate(&scope, &changed, "abc12345", tmp.path(), tmp.path());

        assert!(!result.passed);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].file, "app.ts");
        assert!(matches!(
            result.violations[0].kind,
            ViolationKind::NotInWritablePaths
        ));
    }

    #[test]
    fn validator_detects_missing_artifact() {
        let scope = ScopeConfig {
            required_artifacts: vec!["GROVE_PRD_{short_id}.md".to_string()],
            ..Default::default()
        };

        let tmp = tempfile::tempdir().unwrap();
        let artifacts_dir = tmp.path().join("artifacts");
        std::fs::create_dir_all(&artifacts_dir).unwrap();
        // Do NOT create the artifact file — it should be missing
        let result =
            ScopeValidator::validate(&scope, &[], "abcdef1234567890", tmp.path(), &artifacts_dir);

        assert!(!result.passed);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].file, "GROVE_PRD_abcdef12.md");
        assert!(matches!(
            result.violations[0].kind,
            ViolationKind::MissingArtifact
        ));
    }

    #[test]
    fn validator_passes_when_all_files_in_scope() {
        let scope = ScopeConfig {
            writable_paths: vec!["docs/**".to_string()],
            required_artifacts: vec!["docs/output-{short_id}.md".to_string()],
            ..Default::default()
        };

        let tmp = tempfile::tempdir().unwrap();
        let artifacts_dir = tmp.path().join("artifacts");
        std::fs::create_dir_all(&artifacts_dir).unwrap();
        // Create the required artifact in the artifacts directory so it passes
        std::fs::create_dir_all(artifacts_dir.join("docs")).unwrap();
        std::fs::write(artifacts_dir.join("docs/output-abcdef12.md"), "content").unwrap();

        let changed = vec!["docs/readme.md".to_string()];
        let result = ScopeValidator::validate(
            &scope,
            &changed,
            "abcdef1234567890",
            tmp.path(),
            &artifacts_dir,
        );

        assert!(result.passed);
        assert!(result.violations.is_empty());
    }

    #[test]
    fn end_to_end_scope_validation_flow() {
        // 1. Parse an agent config with scope
        let yaml = r#"
id: build_prd
name: Build PRD
description: Test
can_write: true
can_run_commands: false
scope:
  writable_paths:
    - "GROVE_PRD_*.md"
  blocked_paths:
    - "*.rs"
  required_artifacts:
    - "GROVE_PRD_{short_id}.md"
  on_violation: hard_fail
"#;
        let config: crate::config::agent_config::AgentConfig = serde_yaml::from_str(yaml).unwrap();
        let scope = config.scope.as_ref().unwrap();

        // 2. Build scope contract
        let contract = scope.build_contract("abcdef1234567890");
        assert!(contract.contains("SCOPE CONTRACT"));
        assert!(contract.contains("GROVE_PRD_*.md"));

        // 3. Simulate: agent wrote a .rs file (violation) + wrote PRD (ok)
        //    run_id "abcdef1234567890" → short_id "abcdef12"
        let dir = tempfile::tempdir().unwrap();
        let artifacts_dir = dir.path().join("artifacts");
        std::fs::create_dir_all(&artifacts_dir).unwrap();
        // Write the required artifact to the artifacts directory
        std::fs::write(artifacts_dir.join("GROVE_PRD_abcdef12.md"), "# PRD\n").unwrap();
        let changed_files = vec!["GROVE_PRD_abcdef12.md".to_string(), "main.rs".to_string()];

        let result = ScopeValidator::validate(
            scope,
            &changed_files,
            "abcdef1234567890",
            dir.path(),
            &artifacts_dir,
        );
        assert!(!result.passed);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].file, "main.rs");

        // 4. Simulate: agent wrote only PRD (clean)
        let clean_files = vec!["GROVE_PRD_abcdef12.md".to_string()];
        let clean_result = ScopeValidator::validate(
            scope,
            &clean_files,
            "abcdef1234567890",
            dir.path(),
            &artifacts_dir,
        );
        assert!(clean_result.passed);
    }

    #[test]
    fn discipline_config_from_grove_yaml() {
        // Start from the full default config and override the discipline section
        let yaml = crate::config::DEFAULT_CONFIG_YAML.to_string()
            + "\ndiscipline:\n  default_on_violation: hard_fail\n  strict_verdicts: false\n";
        let cfg: crate::config::GroveConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(
            cfg.discipline.default_on_violation,
            ViolationPolicy::HardFail
        );
        assert!(!cfg.discipline.strict_verdicts);
    }

    #[test]
    fn blocked_paths_take_precedence_over_writable_paths() {
        let scope = ScopeConfig {
            writable_paths: vec!["**".to_string()],
            blocked_paths: vec!["*.rs".to_string()],
            ..Default::default()
        };

        let changed = vec!["main.rs".to_string()];
        let tmp = tempfile::tempdir().unwrap();
        let result = ScopeValidator::validate(&scope, &changed, "abc12345", tmp.path(), tmp.path());

        assert!(!result.passed);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].file, "main.rs");
        assert!(matches!(
            result.violations[0].kind,
            ViolationKind::BlockedPath { .. }
        ));
    }
}
