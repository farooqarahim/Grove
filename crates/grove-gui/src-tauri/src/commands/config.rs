use tauri::State;

use grove_core::llm::{LlmAuthMode, LlmProviderKind, LlmRouter, LlmSelection};

use super::{
    AgentCatalogEntryDto, AgentConfigDto, CapabilityCheckDto, CapabilityReportDto,
    EditorIntegrationStatusDto, HookConfigDto, LastSessionInfo, LlmSelectionDto, ModelDefDto,
    ModelEntryDto, PhaseCheckpointDto, PipelineConfigDto, PipelineDto, ProviderStatusDto,
    SkillConfigDto, UpstreamArtifactDto, emit, resolve_project_root_from_state, shell_path,
};
use crate::state::AppState;

// ── LLM Providers ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_providers() -> Result<Vec<ProviderStatusDto>, String> {
    let providers = LlmRouter::providers();
    Ok(providers
        .into_iter()
        .map(|p| ProviderStatusDto {
            kind: p.kind.id().to_string(),
            name: p.name.to_string(),
            authenticated: p.authenticated,
            model_count: p.model_count,
            default_model: p.default_model.to_string(),
        })
        .collect())
}

#[tauri::command]
pub fn list_models(provider: String) -> Result<Vec<ModelDefDto>, String> {
    let kind = LlmProviderKind::from_str(&provider)
        .ok_or_else(|| format!("unknown provider: {provider}"))?;
    let models = LlmRouter::models(kind);
    Ok(models
        .iter()
        .map(|m| ModelDefDto {
            id: m.id.to_string(),
            name: m.name.to_string(),
            context_window: m.context_window,
            max_output_tokens: m.max_output_tokens,
            cost_input_per_m: m.cost_input_per_m,
            cost_output_per_m: m.cost_output_per_m,
            vision: m.capabilities.vision,
            tools: m.capabilities.tools,
            reasoning: m.capabilities.reasoning,
        })
        .collect())
}

#[tauri::command]
pub fn set_api_key(provider: String, key: String) -> Result<(), String> {
    let kind = LlmProviderKind::from_str(&provider)
        .ok_or_else(|| format!("unknown provider: {provider}"))?;
    LlmRouter::set_api_key(kind, key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_api_key(provider: String) -> Result<(), String> {
    let kind = LlmProviderKind::from_str(&provider)
        .ok_or_else(|| format!("unknown provider: {provider}"))?;
    LlmRouter::remove_api_key(kind).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn is_authenticated(state: State<'_, AppState>, provider: String) -> Result<bool, String> {
    if provider == "claude_code" || provider == "claude_code_persistent" {
        let cfg = grove_core::config::GroveConfig::load_or_create(state.workspace_root())
            .map_err(|e| e.to_string())?;
        return grove_core::capability::is_claude_code_authenticated(
            &cfg.providers.claude_code.command,
        )
        .map_err(|e| e.to_string());
    }

    let kind = LlmProviderKind::from_str(&provider)
        .ok_or_else(|| format!("unknown provider: {provider}"))?;
    Ok(LlmRouter::is_authenticated(kind))
}

#[tauri::command]
pub fn get_llm_selection(state: State<'_, AppState>) -> Result<Option<LlmSelectionDto>, String> {
    let workspace = grove_core::orchestrator::get_workspace(state.workspace_root())
        .map_err(|e| e.to_string())?;
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let selection =
        LlmRouter::get_workspace_selection(&conn, &workspace.id).map_err(|e| e.to_string())?;
    Ok(selection.map(|s| LlmSelectionDto {
        provider: s.kind.id().to_string(),
        model: s.model,
        auth_mode: s.auth_mode.as_str().to_string(),
    }))
}

#[tauri::command]
pub fn set_llm_selection(
    state: State<'_, AppState>,
    provider: String,
    model: Option<String>,
    auth_mode: String,
) -> Result<(), String> {
    let kind = LlmProviderKind::from_str(&provider)
        .ok_or_else(|| format!("unknown provider: {provider}"))?;
    let mode = LlmAuthMode::from_str(&auth_mode)
        .ok_or_else(|| format!("unknown auth mode: {auth_mode}"))?;
    let workspace = grove_core::orchestrator::get_workspace(state.workspace_root())
        .map_err(|e| e.to_string())?;
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let selection = LlmSelection {
        kind,
        model,
        auth_mode: mode,
    };
    LlmRouter::set_workspace_selection(&conn, &workspace.id, &selection).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn detect_editors() -> Result<Vec<EditorIntegrationStatusDto>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let definitions: &[(&str, &str, &str, &[&str])] = &[
            (
                "claude_code",
                "Claude Code",
                "Anthropic CLI for agentic coding workflows.",
                &["claude"],
            ),
            (
                "codex",
                "Codex (OpenAI)",
                "OpenAI CLI for terminal-native software work.",
                &["codex"],
            ),
            (
                "gemini",
                "Gemini (Google)",
                "Google's terminal coding assistant.",
                &["gemini"],
            ),
            (
                "aider",
                "Aider",
                "Pair-programming CLI that edits code directly in your repo.",
                &["aider"],
            ),
            (
                "cursor",
                "Cursor",
                "Cursor agent CLI for automated code execution.",
                &["cursor-agent", "cursor"],
            ),
            (
                "copilot",
                "GitHub Copilot",
                "GitHub's CLI entrypoint for Copilot workflows.",
                &["copilot"],
            ),
            (
                "qwen_code",
                "Qwen Code",
                "Qwen terminal agent for code tasks.",
                &["qwen"],
            ),
            (
                "opencode",
                "OpenCode",
                "OpenCode terminal agent for local code operations.",
                &["opencode"],
            ),
            (
                "kimi",
                "Kimi",
                "Moonshot AI's Kimi coding assistant.",
                &["kimi"],
            ),
            (
                "amp",
                "Amp",
                "Sourcegraph Amp terminal coding assistant.",
                &["amp"],
            ),
            (
                "goose",
                "Goose",
                "Block's Goose CLI agent for code tasks.",
                &["goose"],
            ),
            ("cline", "Cline", "Cline AI coding agent.", &["cline"]),
            (
                "continue",
                "Continue",
                "Continue coding assistant CLI.",
                &["cn"],
            ),
            (
                "kiro",
                "Kiro (AWS)",
                "AWS Kiro coding agent.",
                &["kiro-cli", "kiro"],
            ),
            (
                "auggie",
                "Auggie (Augment Code)",
                "Augment Code AI assistant.",
                &["auggie"],
            ),
            (
                "kilocode",
                "Kilocode",
                "Kilocode terminal coding agent.",
                &["kilocode"],
            ),
        ];

        Ok(definitions
            .iter()
            .map(|(id, name, description, commands)| {
                let search_path = shell_path();
                let detected = commands.iter().find_map(|command| {
                    which::which_in(command, Some(search_path), ".")
                        .ok()
                        .map(|path| (*command, path))
                });

                EditorIntegrationStatusDto {
                    id: id.to_string(),
                    name: name.to_string(),
                    description: description.to_string(),
                    command: detected
                        .as_ref()
                        .map(|(command, _)| (*command).to_string())
                        .unwrap_or_else(|| commands[0].to_string()),
                    detected: detected.is_some(),
                    path: detected.map(|(_, path)| path.to_string_lossy().to_string()),
                }
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Config ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let cfg = grove_core::config::GroveConfig::load_or_create(state.workspace_root())
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_hooks_config(state: State<'_, AppState>) -> Result<HookConfigDto, String> {
    let cfg = grove_core::config::GroveConfig::load_or_create(state.workspace_root())
        .map_err(|e| e.to_string())?;
    let hooks = serde_json::to_value(&cfg.hooks.on).map_err(|e| e.to_string())?;
    let guards = serde_json::to_value(&cfg.hooks.guards).map_err(|e| e.to_string())?;
    Ok(HookConfigDto { hooks, guards })
}

#[tauri::command]
pub async fn detect_capabilities(
    state: State<'_, AppState>,
) -> Result<CapabilityReportDto, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;
        let report = grove_core::capability::detect_capabilities(&cfg, &workspace_root);
        Ok(CapabilityReportDto {
            level: format!("{:?}", report.level),
            checks: report
                .checks
                .into_iter()
                .map(|c| CapabilityCheckDto {
                    name: c.name.to_string(),
                    available: c.available,
                    message: c.message,
                })
                .collect(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Return all agents from the static catalog, annotated with whether they are
/// enabled in the current project config.
#[tauri::command]
pub fn get_agent_catalog(state: State<'_, AppState>) -> Result<Vec<AgentCatalogEntryDto>, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
        .map_err(|e| e.to_string())?;

    let search_path = shell_path();
    let entries = grove_core::providers::catalog::all_agents()
        .iter()
        .map(|entry| {
            let enabled = if entry.id == "claude_code" {
                cfg.providers.claude_code.enabled
            } else {
                cfg.providers
                    .coding_agents
                    .get(entry.id)
                    .map(|c| c.enabled)
                    .unwrap_or(true) // all catalog agents default to enabled
            };
            let detected = which::which_in(entry.cli, Some(&search_path), ".").is_ok();
            AgentCatalogEntryDto {
                id: entry.id.to_string(),
                name: entry.name.to_string(),
                cli: entry.cli.to_string(),
                model_flag: entry.model_flag.map(|s| s.to_string()),
                models: entry
                    .models
                    .iter()
                    .map(|m| ModelEntryDto {
                        id: m.id.to_string(),
                        name: m.name.to_string(),
                        description: m.description.to_string(),
                        is_default: m.is_default,
                    })
                    .collect(),
                enabled,
                detected,
            }
        })
        .collect();

    Ok(entries)
}

/// Return available pipelines for the "New Run" modal.
/// Reads from `skills/pipelines/*.md`; falls back to three hardcoded pipelines
/// when no pipeline configs exist on disk.
#[tauri::command]
pub fn get_pipelines(state: State<'_, AppState>) -> Result<Vec<PipelineDto>, String> {
    let root = resolve_project_root_from_state(&state)?;

    let configs =
        grove_core::config::agent_config::load_pipelines(&root).map_err(|e| e.to_string())?;

    if configs.is_empty() {
        // Fallback: return the 3 hardcoded pipelines
        return Ok(vec![
            PipelineDto {
                id: "plan".into(),
                name: "Plan Mode".into(),
                description: "Requirements + design only. No code changes.".into(),
                agents: vec!["build_prd".into(), "plan_system_design".into()],
                gates: vec!["build_prd".into()],
                is_default: false,
            },
            PipelineDto {
                id: "build".into(),
                name: "Build Mode".into(),
                description: "Implementation + quality gates.".into(),
                agents: vec!["builder".into(), "reviewer".into(), "judge".into()],
                gates: vec![],
                is_default: false,
            },
            PipelineDto {
                id: "autonomous".into(),
                name: "Autonomous Mode".into(),
                description: "Full end-to-end pipeline.".into(),
                agents: vec![
                    "build_prd".into(),
                    "plan_system_design".into(),
                    "builder".into(),
                    "reviewer".into(),
                    "judge".into(),
                ],
                gates: vec!["build_prd".into(), "plan_system_design".into()],
                is_default: true,
            },
        ]);
    }

    let mut dtos: Vec<PipelineDto> = configs
        .into_values()
        .map(|c| PipelineDto {
            id: c.id,
            name: c.name,
            description: c.description,
            agents: c.agents,
            gates: c.gates,
            is_default: c.default,
        })
        .collect();
    dtos.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(dtos)
}

/// Return the currently configured default provider/agent ID.
#[tauri::command]
pub fn get_default_provider(state: State<'_, AppState>) -> Result<String, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
        .map_err(|e| e.to_string())?;
    Ok(cfg.providers.default.clone())
}

/// Persist `provider_id` as the default provider in `.grove/grove.yaml`.
/// The `provider_id` must be a recognised agent id (e.g. `"claude_code"`,
/// `"codex"`, `"gemini"`) or the special value `"auto"`.
#[tauri::command]
pub fn set_default_provider(state: State<'_, AppState>, provider_id: String) -> Result<(), String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let mut cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
        .map_err(|e| e.to_string())?;

    let valid =
        provider_id == "auto" || grove_core::providers::catalog::get_agent(&provider_id).is_some();
    if !valid {
        return Err(format!("unknown provider id '{provider_id}'"));
    }

    cfg.providers.default = provider_id;
    cfg.save(&workspace_root).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_last_session_info(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Option<LastSessionInfo>, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;

    let result = conn
        .query_row(
            "SELECT provider, model, provider_thread_id
         FROM runs
         WHERE id = ?1
         LIMIT 1",
            rusqlite::params![run_id],
            |row| {
                let provider: Option<String> = row.get(0)?;
                let model: Option<String> = row.get(1)?;
                let provider_session_id: Option<String> = row.get(2)?;
                let resumable_thread_id = provider.as_deref().and_then(|name| {
                    match grove_core::providers::session_continuity_policy_for_provider_id(name) {
                        grove_core::providers::SessionContinuityPolicy::DetachedResume => {
                            provider_session_id
                        }
                        _ => None,
                    }
                });
                Ok(LastSessionInfo {
                    provider,
                    model,
                    provider_session_id: resumable_thread_id,
                })
            },
        )
        .ok();

    Ok(result)
}

/// Enable or disable a specific coding agent in `.grove/grove.yaml`.
/// For `claude_code`, toggles `providers.claude_code.enabled`.
/// For all other agents, upserts a minimal entry in `providers.coding_agents`.
#[tauri::command]
pub fn set_agent_enabled(
    state: State<'_, AppState>,
    agent_id: String,
    enabled: bool,
) -> Result<(), String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let mut cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
        .map_err(|e| e.to_string())?;

    if agent_id == "claude_code" || agent_id == "claude_code_persistent" {
        cfg.providers.claude_code.enabled = enabled;
    } else {
        // Validate the agent is in the catalog.
        let catalog_entry = grove_core::providers::catalog::get_agent(&agent_id)
            .ok_or_else(|| format!("unknown agent id '{agent_id}'"))?;

        // Upsert: update existing entry or insert a new minimal one.
        let agent_cfg = cfg
            .providers
            .coding_agents
            .entry(agent_id.clone())
            .or_insert_with(|| grove_core::config::CodingAgentConfig {
                enabled: true,
                command: catalog_entry.cli.to_string(),
                timeout_seconds: 300,
                auto_approve_flag: catalog_entry.auto_approve_flag.map(|s| s.to_string()),
                initial_prompt_flag: catalog_entry.initial_prompt_flag.map(|s| s.to_string()),
                use_keystroke_injection: false,
                use_pty: catalog_entry.use_pty,
                default_args: vec![],
                model_flag: catalog_entry.model_flag.map(|s| s.to_string()),
                max_output_bytes: 10 * 1024 * 1024,
                max_file_size_mb: None,
                max_open_files: None,
            });
        agent_cfg.enabled = enabled;
    }

    cfg.save(&workspace_root).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Phase checkpoints (pipeline gates) ───────────────────────────────────────

#[tauri::command]
pub fn list_phase_checkpoints(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<PhaseCheckpointDto>, String> {
    let handle = grove_core::db::DbHandle::new(state.workspace_root());
    let conn = handle.connect().map_err(|e| e.to_string())?;
    let checkpoints =
        grove_core::db::repositories::phase_checkpoints_repo::list_for_run(&conn, &run_id)
            .map_err(|e| e.to_string())?;
    Ok(checkpoints
        .into_iter()
        .map(|cp| PhaseCheckpointDto {
            id: cp.id,
            run_id: cp.run_id,
            agent: cp.agent,
            status: cp.status,
            decision: cp.decision,
            decided_at: cp.decided_at,
            artifact_path: cp.artifact_path,
            created_at: cp.created_at,
        })
        .collect())
}

#[tauri::command]
pub fn get_pending_checkpoint(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Option<PhaseCheckpointDto>, String> {
    let handle = grove_core::db::DbHandle::new(state.workspace_root());
    let conn = handle.connect().map_err(|e| e.to_string())?;
    let cp = grove_core::db::repositories::phase_checkpoints_repo::get_pending(&conn, &run_id)
        .map_err(|e| e.to_string())?;
    Ok(cp.map(|cp| PhaseCheckpointDto {
        id: cp.id,
        run_id: cp.run_id,
        agent: cp.agent,
        status: cp.status,
        decision: cp.decision,
        decided_at: cp.decided_at,
        artifact_path: cp.artifact_path,
        created_at: cp.created_at,
    }))
}

#[tauri::command]
pub fn submit_gate_decision(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    checkpoint_id: i64,
    decision: String,
    notes: Option<String>,
) -> Result<(), String> {
    let handle = grove_core::db::DbHandle::new(state.workspace_root());
    let conn = handle.connect().map_err(|e| e.to_string())?;
    // Read the run_id before submitting so we can include it in the push event.
    let run_id: Option<String> = conn
        .query_row(
            "SELECT run_id FROM phase_checkpoints WHERE id = ?1",
            [checkpoint_id],
            |r| r.get(0),
        )
        .ok();
    grove_core::db::repositories::phase_checkpoints_repo::submit_decision(
        &conn,
        checkpoint_id,
        &decision,
        notes.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    if let Some(ref rid) = run_id {
        if let Some(control) = state.run_controls.lock().get(rid).cloned() {
            let _ = control.tx.send(
                grove_core::providers::claude_code_persistent::RunControlMessage::GateDecision {
                    checkpoint_id,
                    decision: decision.clone(),
                    notes: notes.clone(),
                },
            );
        }
    }
    emit(
        &app,
        "grove://phase-gate-decided",
        serde_json::json!({
            "checkpoint_id": checkpoint_id,
            "decision": decision,
            "run_id": run_id,
        }),
    );
    Ok(())
}

// ── Agents ──

#[tauri::command]
pub fn list_agent_configs(state: State<'_, AppState>) -> Result<Vec<AgentConfigDto>, String> {
    let root = resolve_project_root_from_state(&state)?;
    let agents = grove_core::config::agent_config::load_agents(&root).map_err(|e| e.to_string())?;
    let mut result: Vec<AgentConfigDto> = agents
        .into_values()
        .map(|a| AgentConfigDto {
            id: a.id,
            name: a.name,
            description: a.description,
            can_write: a.can_write,
            can_run_commands: a.can_run_commands,
            artifact: a.artifact,
            allowed_tools: a.allowed_tools,
            skills: a.skills,
            upstream_artifacts: a
                .upstream_artifacts
                .into_iter()
                .map(|u| UpstreamArtifactDto {
                    label: u.label,
                    filename: u.filename,
                })
                .collect(),
            prompt: a.prompt,
        })
        .collect();
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

#[tauri::command]
pub fn get_agent_config(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<AgentConfigDto, String> {
    let root = resolve_project_root_from_state(&state)?;
    let agents = grove_core::config::agent_config::load_agents(&root).map_err(|e| e.to_string())?;
    let agent = agents
        .get(&agent_id)
        .ok_or_else(|| format!("agent not found: {agent_id}"))?;
    Ok(AgentConfigDto {
        id: agent.id.clone(),
        name: agent.name.clone(),
        description: agent.description.clone(),
        can_write: agent.can_write,
        can_run_commands: agent.can_run_commands,
        artifact: agent.artifact.clone(),
        allowed_tools: agent.allowed_tools.clone(),
        skills: agent.skills.clone(),
        upstream_artifacts: agent
            .upstream_artifacts
            .iter()
            .map(|u| UpstreamArtifactDto {
                label: u.label.clone(),
                filename: u.filename.clone(),
            })
            .collect(),
        prompt: agent.prompt.clone(),
    })
}

#[tauri::command]
pub fn save_agent_config(state: State<'_, AppState>, config: AgentConfigDto) -> Result<(), String> {
    let root = resolve_project_root_from_state(&state)?;
    let agent = grove_core::config::agent_config::AgentConfig {
        id: config.id,
        name: config.name,
        description: config.description,
        can_write: config.can_write,
        can_run_commands: config.can_run_commands,
        artifact: config.artifact,
        allowed_tools: config.allowed_tools,
        skills: config.skills,
        upstream_artifacts: config
            .upstream_artifacts
            .into_iter()
            .map(|u| grove_core::config::agent_config::UpstreamArtifact {
                label: u.label,
                filename: u.filename,
            })
            .collect(),
        scope: None,
        prompt: config.prompt,
    };
    grove_core::config::agent_config::save_agent(&root, &agent).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_agent_config(state: State<'_, AppState>, agent_id: String) -> Result<(), String> {
    let root = resolve_project_root_from_state(&state)?;
    grove_core::config::agent_config::delete_agent(&root, &agent_id).map_err(|e| e.to_string())
}

// ── Pipelines ──

#[tauri::command]
pub fn list_pipeline_configs(state: State<'_, AppState>) -> Result<Vec<PipelineConfigDto>, String> {
    let root = resolve_project_root_from_state(&state)?;
    let pipelines =
        grove_core::config::agent_config::load_pipelines(&root).map_err(|e| e.to_string())?;
    let mut result: Vec<PipelineConfigDto> = pipelines
        .into_values()
        .map(|p| PipelineConfigDto {
            id: p.id,
            name: p.name,
            description: p.description,
            agents: p.agents,
            gates: p.gates,
            default: p.default,
            aliases: p.aliases,
            content: p.content,
        })
        .collect();
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

#[tauri::command]
pub fn save_pipeline_config(
    state: State<'_, AppState>,
    config: PipelineConfigDto,
) -> Result<(), String> {
    let root = resolve_project_root_from_state(&state)?;
    let pipeline = grove_core::config::agent_config::PipelineConfig {
        id: config.id,
        name: config.name,
        description: config.description,
        agents: config.agents,
        gates: config.gates,
        default: config.default,
        aliases: config.aliases,
        content: config.content,
    };
    grove_core::config::agent_config::save_pipeline(&root, &pipeline).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_pipeline_config(
    state: State<'_, AppState>,
    pipeline_id: String,
) -> Result<(), String> {
    let root = resolve_project_root_from_state(&state)?;
    grove_core::config::agent_config::delete_pipeline(&root, &pipeline_id)
        .map_err(|e| e.to_string())
}

// ── Skills ──

#[tauri::command]
pub fn list_skill_configs(state: State<'_, AppState>) -> Result<Vec<SkillConfigDto>, String> {
    let root = resolve_project_root_from_state(&state)?;
    let skills = grove_core::config::agent_config::load_skills(&root).map_err(|e| e.to_string())?;
    let mut result: Vec<SkillConfigDto> = skills
        .into_values()
        .map(|s| SkillConfigDto {
            id: s.id,
            name: s.name,
            description: s.description,
            applies_to: s.applies_to,
            content: s.content,
        })
        .collect();
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

#[tauri::command]
pub fn save_skill_config(state: State<'_, AppState>, config: SkillConfigDto) -> Result<(), String> {
    let root = resolve_project_root_from_state(&state)?;
    let skill = grove_core::config::agent_config::SkillConfig {
        id: config.id,
        name: config.name,
        description: config.description,
        applies_to: config.applies_to,
        content: config.content,
    };
    grove_core::config::agent_config::save_skill(&root, &skill).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_skill_config(state: State<'_, AppState>, skill_id: String) -> Result<(), String> {
    let root = resolve_project_root_from_state(&state)?;
    grove_core::config::agent_config::delete_skill(&root, &skill_id).map_err(|e| e.to_string())
}

/// Preview how an agent prompt renders with test variables.
#[tauri::command]
pub fn preview_agent_prompt(
    state: State<'_, AppState>,
    agent_id: String,
    objective: String,
) -> Result<String, String> {
    let root = resolve_project_root_from_state(&state)?;
    let agents = grove_core::config::agent_config::load_agents(&root).map_err(|e| e.to_string())?;
    let agent = agents
        .get(&agent_id)
        .ok_or_else(|| format!("agent not found: {agent_id}"))?;

    let test_run_id = "test1234";
    let artifacts_dir = grove_core::config::paths::grove_dir(&root)
        .join("artifacts")
        .join("_preview")
        .join(test_run_id);
    let rendered = grove_core::config::agent_config::render_prompt(
        agent,
        &objective,
        test_run_id,
        &artifacts_dir,
    );

    // Also load skills for this agent
    let skills = grove_core::config::agent_config::load_skills(&root).map_err(|e| e.to_string())?;
    let skills_text = grove_core::config::agent_config::load_skills_for_agent(agent, &skills);

    if skills_text.is_empty() {
        Ok(rendered)
    } else {
        Ok(format!("{rendered}{skills_text}"))
    }
}
