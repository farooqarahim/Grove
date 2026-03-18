use anyhow::Error;
use grove_core::errors::GroveError;
use serde_json::{Value, json};

use crate::exit_codes;

#[derive(Debug, Clone)]
pub struct ClassifiedError {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
    pub exit_code: i32,
    pub details: Value,
}

pub fn classify(err: &Error) -> ClassifiedError {
    if let Some(grove_err) = err.downcast_ref::<GroveError>() {
        return classify_grove_error(grove_err);
    }

    ClassifiedError {
        code: "RUNTIME_ERROR",
        message: err.to_string(),
        hint: "Run with --verbose for additional context".to_string(),
        exit_code: exit_codes::RUNTIME_ERROR,
        details: json!({}),
    }
}

pub fn to_json(classified: &ClassifiedError) -> Value {
    json!({
        "error": {
            "code": classified.code,
            "message": classified.message,
            "hint": classified.hint,
            "details": classified.details
        }
    })
}

fn classify_grove_error(err: &GroveError) -> ClassifiedError {
    match err {
        GroveError::Config(msg) => ClassifiedError {
            code: "CONFIG_INVALID",
            message: msg.clone(),
            hint: "Set providers.default and required fields in .grove/grove.yaml".to_string(),
            exit_code: exit_codes::USER_ERROR,
            details: json!({}),
        },
        GroveError::NotFound(msg) => ClassifiedError {
            code: "NOT_FOUND",
            message: msg.clone(),
            hint: "Verify the provided run id/path exists".to_string(),
            exit_code: exit_codes::USER_ERROR,
            details: json!({}),
        },
        GroveError::InvalidTransition(msg) => ClassifiedError {
            code: "INVARIANT_VIOLATION",
            message: msg.clone(),
            hint: "Inspect run state and resume/abort only from legal states".to_string(),
            exit_code: exit_codes::INVARIANT_ERROR,
            details: json!({}),
        },
        GroveError::Database(msg) => ClassifiedError {
            code: "DATABASE_ERROR",
            message: msg.to_string(),
            hint: "Run `grove doctor --fix` and retry".to_string(),
            exit_code: exit_codes::RUNTIME_ERROR,
            details: json!({}),
        },
        GroveError::Io(msg) => ClassifiedError {
            code: "IO_ERROR",
            message: msg.to_string(),
            hint: "Verify filesystem permissions for project and .grove directories".to_string(),
            exit_code: exit_codes::RUNTIME_ERROR,
            details: json!({}),
        },
        GroveError::SerdeJson(msg) => ClassifiedError {
            code: "SERDE_JSON_ERROR",
            message: msg.to_string(),
            hint: "Check persisted JSON payloads for corruption".to_string(),
            exit_code: exit_codes::RUNTIME_ERROR,
            details: json!({}),
        },
        GroveError::SerdeYaml(msg) => ClassifiedError {
            code: "SERDE_YAML_ERROR",
            message: msg.to_string(),
            hint: "Validate .grove/grove.yaml syntax".to_string(),
            exit_code: exit_codes::USER_ERROR,
            details: json!({}),
        },
        GroveError::Runtime(msg) => ClassifiedError {
            code: "RUNTIME_ERROR",
            message: msg.clone(),
            hint: "Review run logs and provider configuration".to_string(),
            exit_code: exit_codes::RUNTIME_ERROR,
            details: json!({}),
        },
        GroveError::BudgetExceeded { used_usd, limit_usd } => ClassifiedError {
            code: "BUDGET_EXCEEDED",
            message: format!("budget exceeded: used ${used_usd:.4} of ${limit_usd:.4}"),
            hint: "Increase --budget-usd or reduce agent scope".to_string(),
            exit_code: exit_codes::USER_ERROR,
            details: json!({ "used_usd": used_usd, "limit_usd": limit_usd }),
        },
        GroveError::MergeConflict { files, file_count } => {
            let file_list: Vec<&str> = files.split(", ").collect();
            ClassifiedError {
                code: "MERGE_CONFLICT",
                message: format!("merge conflict on {file_count} file(s): {files}"),
                hint: "Two agents modified the same file(s). Re-run with a different plan or resolve manually.".to_string(),
                exit_code: exit_codes::RUNTIME_ERROR,
                details: json!({ "conflicting_files": file_list, "file_count": file_count }),
            }
        },
        GroveError::LlmAuth { provider, message } => ClassifiedError {
            code: "LLM_AUTH_ERROR",
            message: message.clone(),
            hint: format!("Run: grove auth set {provider} <api-key>"),
            exit_code: exit_codes::USER_ERROR,
            details: json!({ "provider": provider }),
        },
        GroveError::LlmRequest { provider, message } => ClassifiedError {
            code: "LLM_REQUEST_ERROR",
            message: message.clone(),
            hint: "Check your network connection and API endpoint".to_string(),
            exit_code: exit_codes::RUNTIME_ERROR,
            details: json!({ "provider": provider }),
        },
        GroveError::LlmApi { provider, status, message } => ClassifiedError {
            code: "LLM_API_ERROR",
            message: message.clone(),
            hint: format!("Check your {provider} API key and quota"),
            exit_code: exit_codes::RUNTIME_ERROR,
            details: json!({ "provider": provider, "http_status": status }),
        },
        GroveError::InsufficientCredits { available_usd, required_usd } => ClassifiedError {
            code: "INSUFFICIENT_CREDITS",
            message: format!(
                "workspace credits too low: have ${available_usd:.4}, need ${required_usd:.4}"
            ),
            hint: "Add credits with: grove llm credits add <amount>".to_string(),
            exit_code: exit_codes::USER_ERROR,
            details: json!({ "available_usd": available_usd, "required_usd": required_usd }),
        },
        GroveError::Aborted => ClassifiedError {
            code: "ABORTED",
            message: "run aborted by user".to_string(),
            hint: "The run was aborted. Resume with: grove resume <run_id>".to_string(),
            exit_code: exit_codes::USER_ERROR,
            details: json!({}),
        },
        GroveError::PoolFull { project_id, active, max } => ClassifiedError {
            code: "CONCURRENCY_LIMIT",
            message: format!("concurrent conversation limit reached ({active}/{max} active)"),
            hint: "Use `grove queue` to queue the run, or increase `runtime.max_concurrent_runs` in grove.yaml".to_string(),
            exit_code: exit_codes::USER_ERROR,
            details: json!({ "project_id": project_id, "active": active, "max": max }),
        },
        GroveError::WorktreeError { operation, message } => ClassifiedError {
            code: "WORKTREE_ERROR",
            message: message.clone(),
            hint: "Verify git is installed and the repository is not corrupt. Run `grove doctor`.".to_string(),
            exit_code: exit_codes::RUNTIME_ERROR,
            details: json!({ "operation": operation }),
        },
        GroveError::OwnershipConflict { path, holder } => ClassifiedError {
            code: "OWNERSHIP_CONFLICT",
            message: format!("file '{path}' is already locked by session {holder}"),
            hint: "Wait for the other agent to finish, or abort the run and retry.".to_string(),
            exit_code: exit_codes::RUNTIME_ERROR,
            details: json!({ "path": path, "holder_session": holder }),
        },
        GroveError::ProviderError { provider, message } => ClassifiedError {
            code: "PROVIDER_ERROR",
            message: message.clone(),
            hint: format!("Check {provider} is installed and the API key is set with: grove auth set {provider} <key>"),
            exit_code: exit_codes::RUNTIME_ERROR,
            details: json!({ "provider": provider }),
        },
        GroveError::HookError { hook, message } => ClassifiedError {
            code: "HOOK_ERROR",
            message: message.clone(),
            hint: "Review the hook script in .grove/grove.yaml and check its exit code.".to_string(),
            exit_code: exit_codes::RUNTIME_ERROR,
            details: json!({ "hook": hook }),
        },
        GroveError::ValidationError { field, message } => ClassifiedError {
            code: "VALIDATION_ERROR",
            message: message.clone(),
            hint: "Check the value of this field in .grove/grove.yaml.".to_string(),
            exit_code: exit_codes::USER_ERROR,
            details: json!({ "field": field }),
        },
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use grove_core::errors::GroveError;

    use super::*;

    fn grove_err(e: GroveError) -> anyhow::Error {
        anyhow!(e)
    }

    #[test]
    fn config_invalid_maps_to_code_and_exit_2() {
        let classified = classify(&grove_err(GroveError::Config("bad field".into())));
        assert_eq!(classified.code, "CONFIG_INVALID");
        assert_eq!(classified.exit_code, exit_codes::USER_ERROR);
        assert!(!classified.hint.is_empty());
    }

    #[test]
    fn not_found_maps_to_code_and_exit_2() {
        let classified = classify(&grove_err(GroveError::NotFound("run_abc".into())));
        assert_eq!(classified.code, "NOT_FOUND");
        assert_eq!(classified.exit_code, exit_codes::USER_ERROR);
    }

    #[test]
    fn invalid_transition_maps_to_invariant_violation_and_exit_4() {
        let classified = classify(&grove_err(GroveError::InvalidTransition("bad move".into())));
        assert_eq!(classified.code, "INVARIANT_VIOLATION");
        assert_eq!(classified.exit_code, exit_codes::INVARIANT_ERROR);
    }

    #[test]
    fn runtime_error_maps_to_runtime_and_exit_3() {
        let classified = classify(&grove_err(GroveError::Runtime("boom".into())));
        assert_eq!(classified.code, "RUNTIME_ERROR");
        assert_eq!(classified.exit_code, exit_codes::RUNTIME_ERROR);
    }

    #[test]
    fn budget_exceeded_maps_to_budget_exceeded_and_exit_2() {
        let classified = classify(&grove_err(GroveError::BudgetExceeded {
            used_usd: 5.0,
            limit_usd: 3.0,
        }));
        assert_eq!(classified.code, "BUDGET_EXCEEDED");
        assert_eq!(classified.exit_code, exit_codes::USER_ERROR);
        assert!(classified.message.contains("5.0000"));
        assert!(classified.message.contains("3.0000"));
        assert!(classified.hint.contains("--budget-usd"));
    }

    #[test]
    fn database_error_maps_to_database_error_and_exit_3() {
        let db_err = rusqlite::Error::QueryReturnedNoRows;
        let classified = classify(&grove_err(GroveError::Database(db_err)));
        assert_eq!(classified.code, "DATABASE_ERROR");
        assert_eq!(classified.exit_code, exit_codes::RUNTIME_ERROR);
    }

    #[test]
    fn io_error_maps_to_io_error_and_exit_3() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let classified = classify(&grove_err(GroveError::Io(io_err)));
        assert_eq!(classified.code, "IO_ERROR");
        assert_eq!(classified.exit_code, exit_codes::RUNTIME_ERROR);
    }

    #[test]
    fn to_json_contains_required_envelope_fields() {
        let classified = ClassifiedError {
            code: "CONFIG_INVALID",
            message: "test message".into(),
            hint: "test hint".into(),
            exit_code: 2,
            details: json!({}),
        };
        let json = to_json(&classified);
        assert_eq!(json["error"]["code"], "CONFIG_INVALID");
        assert_eq!(json["error"]["message"], "test message");
        assert_eq!(json["error"]["hint"], "test hint");
        assert!(json["error"]["details"].is_object());
    }

    #[test]
    fn merge_conflict_maps_to_merge_conflict_and_exit_3() {
        let classified = classify(&grove_err(GroveError::MergeConflict {
            files: "src/main.rs, lib.rs".into(),
            file_count: 2,
        }));
        assert_eq!(classified.code, "MERGE_CONFLICT");
        assert_eq!(classified.exit_code, exit_codes::RUNTIME_ERROR);
        assert!(classified.message.contains("2 file(s)"));
        assert!(classified.message.contains("src/main.rs"));
        assert!(classified.hint.contains("Re-run"));

        // Structured details for programmatic consumption.
        let json = to_json(&classified);
        let details = &json["error"]["details"];
        assert_eq!(details["file_count"], 2);
        let files = details["conflicting_files"].as_array().unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], "src/main.rs");
        assert_eq!(files[1], "lib.rs");
    }

    #[test]
    fn unknown_anyhow_error_maps_to_runtime_error() {
        let err = anyhow!("some unexpected error");
        let classified = classify(&err);
        assert_eq!(classified.code, "RUNTIME_ERROR");
        assert_eq!(classified.exit_code, exit_codes::RUNTIME_ERROR);
    }

    #[test]
    fn worktree_error_maps_to_worktree_error_and_exit_3() {
        let classified = classify(&grove_err(GroveError::WorktreeError {
            operation: "git_worktree_add".into(),
            message: "ref already in use".into(),
        }));
        assert_eq!(classified.code, "WORKTREE_ERROR");
        assert_eq!(classified.exit_code, exit_codes::RUNTIME_ERROR);
        assert_eq!(classified.details["operation"], "git_worktree_add");
    }

    #[test]
    fn ownership_conflict_maps_to_ownership_conflict_and_exit_3() {
        let classified = classify(&grove_err(GroveError::OwnershipConflict {
            path: "src/main.rs".into(),
            holder: "session-abc".into(),
        }));
        assert_eq!(classified.code, "OWNERSHIP_CONFLICT");
        assert_eq!(classified.exit_code, exit_codes::RUNTIME_ERROR);
        assert_eq!(classified.details["path"], "src/main.rs");
        assert_eq!(classified.details["holder_session"], "session-abc");
        assert!(classified.message.contains("session-abc"));
    }

    #[test]
    fn hook_error_maps_to_hook_error_and_exit_3() {
        let classified = classify(&grove_err(GroveError::HookError {
            hook: "pre_merge.sh".into(),
            message: "exit 1: tests failed".into(),
        }));
        assert_eq!(classified.code, "HOOK_ERROR");
        assert_eq!(classified.exit_code, exit_codes::RUNTIME_ERROR);
        assert_eq!(classified.details["hook"], "pre_merge.sh");
    }

    #[test]
    fn validation_error_maps_to_validation_error_and_exit_2() {
        let classified = classify(&grove_err(GroveError::ValidationError {
            field: "runtime.max_concurrent_runs".into(),
            message: "must be a positive integer".into(),
        }));
        assert_eq!(classified.code, "VALIDATION_ERROR");
        assert_eq!(classified.exit_code, exit_codes::USER_ERROR);
        assert_eq!(classified.details["field"], "runtime.max_concurrent_runs");
    }

    #[test]
    fn provider_error_maps_to_provider_error_and_exit_3() {
        let classified = classify(&grove_err(GroveError::ProviderError {
            provider: "claude_code".into(),
            message: "process exited with code 1".into(),
        }));
        assert_eq!(classified.code, "PROVIDER_ERROR");
        assert_eq!(classified.exit_code, exit_codes::RUNTIME_ERROR);
        assert_eq!(classified.details["provider"], "claude_code");
    }
}
