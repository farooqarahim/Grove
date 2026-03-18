/// Markdown-based agent, pipeline, and skill configuration loader.
///
/// Reads `skills/agents/*.md`, `skills/pipelines/*.md`, and `skills/*/SKILL.md`
/// from the project root at runtime instead of relying on hardcoded Rust definitions.
///
/// All config files use Markdown with YAML frontmatter (delimited by `---`).
/// The frontmatter carries structured metadata (id, permissions, tools, etc.)
/// and the Markdown body is the prompt/content. This format is optimal for
/// LLM consumption since models are heavily trained on Markdown.
///
/// The Rust `AgentType` and `PipelineKind` enums remain for type safety and
/// backward compat — the loader maps between config IDs and enum variants.
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agents::AgentType;
use crate::errors::{GroveError, GroveResult};
use crate::orchestrator::pipeline::PipelineKind;

// ── Agent config ────────────────────────────────────────────────────────────

/// Parsed agent config from `skills/agents/<id>.md` frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default = "default_true")]
    pub can_write: bool,
    #[serde(default)]
    pub can_run_commands: bool,
    /// Output artifact template. `{run_id}` is replaced at runtime.
    /// `null` means the agent produces code, not a document.
    pub artifact: Option<String>,
    /// Tool allowlist. `null` means all tools allowed (no restriction).
    pub allowed_tools: Option<Vec<String>>,
    /// Skill IDs loaded into this agent's context.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Upstream artifacts this agent should read.
    #[serde(default)]
    pub upstream_artifacts: Vec<UpstreamArtifact>,
    /// Per-agent scope restrictions (writable/blocked paths, required artifacts).
    #[serde(default)]
    pub scope: Option<crate::orchestrator::scope::ScopeConfig>,
    /// The full Markdown body (after frontmatter) — this IS the prompt.
    #[serde(skip)]
    pub prompt: String,
}

/// An upstream artifact reference in agent config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamArtifact {
    pub label: String,
    /// Filename template with `{run_id}` placeholder.
    pub filename: String,
}

// ── Pipeline config ─────────────────────────────────────────────────────────

/// Parsed pipeline config from `skills/pipelines/<id>.md` frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Ordered sequence of agent IDs.
    pub agents: Vec<String>,
    /// Agent IDs after which execution pauses for user review.
    #[serde(default)]
    pub gates: Vec<String>,
    /// Whether this is the default pipeline.
    #[serde(default)]
    pub default: bool,
    /// Legacy aliases that map to this pipeline.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// The full Markdown body (after frontmatter) — pipeline docs.
    #[serde(skip)]
    pub content: String,
}

// ── Skill config ────────────────────────────────────────────────────────────

/// Parsed skill from `skills/<id>/SKILL.md`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Which agents this skill applies to (if specified in frontmatter).
    #[serde(default)]
    pub applies_to: Vec<String>,
    /// The full Markdown body (after frontmatter).
    #[serde(skip)]
    pub content: String,
}

// ── Loaded config bundle ────────────────────────────────────────────────────

/// All loaded configs for a project.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProjectConfigs {
    pub agents: HashMap<String, AgentConfig>,
    pub pipelines: HashMap<String, PipelineConfig>,
    pub skills: HashMap<String, SkillConfig>,
}

// ── Path helpers ────────────────────────────────────────────────────────────

/// `skills/agents/` directory.
pub fn agents_dir(project_root: &Path) -> PathBuf {
    project_root.join("skills").join("agents")
}

/// `skills/pipelines/` directory.
pub fn pipelines_dir(project_root: &Path) -> PathBuf {
    project_root.join("skills").join("pipelines")
}

/// `skills/` directory (top-level — skills are subdirectories with SKILL.md).
pub fn skills_dir(project_root: &Path) -> PathBuf {
    project_root.join("skills")
}

// ── Frontmatter parser ──────────────────────────────────────────────────────

/// Split a Markdown file into YAML frontmatter and body content.
///
/// Returns `(frontmatter_yaml, markdown_body)`. If no frontmatter is found,
/// returns `("", full_content)`.
fn split_frontmatter(raw: &str) -> (&str, &str) {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return ("", raw);
    }

    let after_first = &trimmed[3..];
    // Skip optional newline after opening ---
    let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);

    if let Some(end_idx) = after_first.find("\n---") {
        let frontmatter = &after_first[..end_idx];
        let body = after_first[end_idx + 4..].trim_start_matches(['\n', '\r']);
        (frontmatter, body)
    } else {
        ("", raw)
    }
}

// ── Loaders ─────────────────────────────────────────────────────────────────

/// Load all agent configs from `skills/agents/*.md`.
pub fn load_agents(project_root: &Path) -> GroveResult<HashMap<String, AgentConfig>> {
    let dir = agents_dir(project_root);
    if !dir.exists() {
        return Ok(HashMap::new());
    }

    let mut agents = HashMap::new();
    for entry in read_dir_sorted(&dir)? {
        let path = entry;
        if path.extension().is_some_and(|ext| ext == "md") {
            let raw = read_file(&path)?;
            let (frontmatter, body) = split_frontmatter(&raw);

            if frontmatter.is_empty() {
                eprintln!(
                    "[WARN] skipping agent file without frontmatter: {}",
                    path.display()
                );
                continue;
            }

            let mut config: AgentConfig = parse_yaml(frontmatter, &path)?;
            config.prompt = body.to_string();
            agents.insert(config.id.clone(), config);
        }
    }

    Ok(agents)
}

/// Load all pipeline configs from `skills/pipelines/*.md`.
pub fn load_pipelines(project_root: &Path) -> GroveResult<HashMap<String, PipelineConfig>> {
    let dir = pipelines_dir(project_root);
    if !dir.exists() {
        return Ok(HashMap::new());
    }

    let mut pipelines = HashMap::new();
    for path in read_dir_sorted(&dir)? {
        if path.extension().is_some_and(|ext| ext == "md") {
            let raw = read_file(&path)?;
            let (frontmatter, body) = split_frontmatter(&raw);

            if frontmatter.is_empty() {
                eprintln!(
                    "[WARN] skipping pipeline file without frontmatter: {}",
                    path.display()
                );
                continue;
            }

            let mut config: PipelineConfig = parse_yaml(frontmatter, &path)?;
            config.content = body.to_string();
            pipelines.insert(config.id.clone(), config);
        }
    }

    Ok(pipelines)
}

/// Load all skill configs from `skills/*/SKILL.md`.
///
/// Skips the `agents/` and `pipelines/` subdirectories (those are agent and
/// pipeline configs, not skills).
pub fn load_skills(project_root: &Path) -> GroveResult<HashMap<String, SkillConfig>> {
    let dir = skills_dir(project_root);
    if !dir.exists() {
        return Ok(HashMap::new());
    }

    // These subdirs hold agent/pipeline configs, not skills
    const RESERVED_DIRS: &[&str] = &["agents", "pipelines", "graph"];

    let mut skills = HashMap::new();
    for entry in fs::read_dir(&dir)
        .map_err(|e| GroveError::Config(format!("failed to read skills dir: {e}")))?
    {
        let entry =
            entry.map_err(|e| GroveError::Config(format!("failed to read skills entry: {e}")))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Skip reserved directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if RESERVED_DIRS.contains(&name) {
                continue;
            }
        }

        let skill_file = path.join("SKILL.md");
        if !skill_file.exists() {
            continue;
        }

        let raw = read_file(&skill_file)?;
        let skill_id = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let (frontmatter, body) = split_frontmatter(&raw);

        if frontmatter.is_empty() {
            // No frontmatter — use directory name as ID, entire file as content
            skills.insert(
                skill_id.clone(),
                SkillConfig {
                    id: skill_id.clone(),
                    name: skill_id,
                    description: String::new(),
                    applies_to: Vec::new(),
                    content: raw,
                },
            );
            continue;
        }

        #[derive(Deserialize)]
        struct SkillFrontmatter {
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            description: Option<String>,
            #[serde(default)]
            applies_to: Vec<String>,
        }

        match serde_yaml::from_str::<SkillFrontmatter>(frontmatter) {
            Ok(fm) => {
                skills.insert(
                    skill_id.clone(),
                    SkillConfig {
                        id: skill_id.clone(),
                        name: fm.name.unwrap_or_else(|| skill_id.clone()),
                        description: fm.description.unwrap_or_default(),
                        applies_to: fm.applies_to,
                        content: body.to_string(),
                    },
                );
            }
            Err(e) => {
                eprintln!("[WARN] skipping skill {}: {}", skill_file.display(), e);
            }
        }
    }

    Ok(skills)
}

/// Load all configs (agents, pipelines, skills) for a project.
pub fn load_all(project_root: &Path) -> GroveResult<ProjectConfigs> {
    Ok(ProjectConfigs {
        agents: load_agents(project_root)?,
        pipelines: load_pipelines(project_root)?,
        skills: load_skills(project_root)?,
    })
}

// ── Save/update helpers ─────────────────────────────────────────────────────

/// Save an agent config to `skills/agents/<id>.md`.
///
/// Reconstructs the Markdown file with YAML frontmatter + prompt body.
pub fn save_agent(project_root: &Path, config: &AgentConfig) -> GroveResult<()> {
    let dir = agents_dir(project_root);
    fs::create_dir_all(&dir)
        .map_err(|e| GroveError::Config(format!("failed to create agents dir: {e}")))?;

    let path = dir.join(format!("{}.md", config.id));
    let content = build_agent_md(config)?;
    fs::write(&path, content)
        .map_err(|e| GroveError::Config(format!("failed to write agent config: {e}")))?;

    Ok(())
}

/// Save a pipeline config to `skills/pipelines/<id>.md`.
pub fn save_pipeline(project_root: &Path, config: &PipelineConfig) -> GroveResult<()> {
    let dir = pipelines_dir(project_root);
    fs::create_dir_all(&dir)
        .map_err(|e| GroveError::Config(format!("failed to create pipelines dir: {e}")))?;

    let path = dir.join(format!("{}.md", config.id));
    let content = build_pipeline_md(config)?;
    fs::write(&path, content)
        .map_err(|e| GroveError::Config(format!("failed to write pipeline config: {e}")))?;

    Ok(())
}

/// Save a skill config to `skills/<id>/SKILL.md`.
pub fn save_skill(project_root: &Path, config: &SkillConfig) -> GroveResult<()> {
    let dir = skills_dir(project_root).join(&config.id);
    fs::create_dir_all(&dir)
        .map_err(|e| GroveError::Config(format!("failed to create skill dir: {e}")))?;

    let path = dir.join("SKILL.md");
    let mut output = String::new();
    output.push_str("---\n");
    output.push_str(&format!("name: {}\n", config.name));
    output.push_str(&format!("description: {}\n", config.description));
    if !config.applies_to.is_empty() {
        output.push_str("applies_to:\n");
        for agent in &config.applies_to {
            output.push_str(&format!("  - {agent}\n"));
        }
    }
    output.push_str("---\n\n");
    output.push_str(&config.content);
    if !output.ends_with('\n') {
        output.push('\n');
    }

    fs::write(&path, output)
        .map_err(|e| GroveError::Config(format!("failed to write skill file: {e}")))?;

    Ok(())
}

/// Delete an agent config.
pub fn delete_agent(project_root: &Path, agent_id: &str) -> GroveResult<()> {
    let path = agents_dir(project_root).join(format!("{agent_id}.md"));
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|e| GroveError::Config(format!("failed to delete agent config: {e}")))?;
    }
    Ok(())
}

/// Delete a pipeline config.
pub fn delete_pipeline(project_root: &Path, pipeline_id: &str) -> GroveResult<()> {
    let path = pipelines_dir(project_root).join(format!("{pipeline_id}.md"));
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|e| GroveError::Config(format!("failed to delete pipeline config: {e}")))?;
    }
    Ok(())
}

/// Delete a skill config.
pub fn delete_skill(project_root: &Path, skill_id: &str) -> GroveResult<()> {
    let dir = skills_dir(project_root).join(skill_id);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|e| GroveError::Config(format!("failed to delete skill: {e}")))?;
    }
    Ok(())
}

// ── Prompt rendering ────────────────────────────────────────────────────────

/// Render an agent's prompt (Markdown body) with runtime variables.
///
/// Replaces `{objective}`, `{run_id}`, `{artifact_filename}`, and `{artifacts_dir}` in the prompt.
/// `{artifact_filename}` resolves to the full path including the artifacts directory.
pub fn render_prompt(
    config: &AgentConfig,
    objective: &str,
    run_id: &str,
    artifacts_dir: &Path,
) -> String {
    let short_id = short_run_id(run_id);
    let artifacts_dir_str = artifacts_dir.display().to_string();

    let artifact_filename = config
        .artifact
        .as_ref()
        .map(|a| {
            let name = a.replace("{run_id}", short_id);
            artifacts_dir.join(&name).display().to_string()
        })
        .unwrap_or_default();

    config
        .prompt
        .replace("{objective}", objective)
        .replace("{run_id}", short_id)
        .replace("{artifact_filename}", &artifact_filename)
        .replace("{artifacts_dir}", &artifacts_dir_str)
}

/// Render upstream artifact context for an agent (checks artifacts directory for existence).
pub fn render_upstream_context(config: &AgentConfig, run_id: &str, artifacts_dir: &Path) -> String {
    let short_id = short_run_id(run_id);

    let mut lines = vec![];
    let mut any_found = false;

    for artifact in &config.upstream_artifacts {
        let filename = artifact.filename.replace("{run_id}", short_id);
        let full_path = artifacts_dir.join(&filename);
        if full_path.exists() {
            lines.push(format!(
                "- {}: `{}` exists — read it before starting.",
                artifact.label,
                full_path.display()
            ));
            any_found = true;
        }
    }

    if !any_found {
        return String::new();
    }

    let mut result = String::from("--- UPSTREAM ARTIFACTS ---\n");
    result.push_str(&lines.join("\n"));
    result.push_str("\n--- END UPSTREAM ARTIFACTS ---");
    result
}

/// Load skills content for an agent, concatenating all applicable skill content.
///
/// Includes skills explicitly listed in the agent's `skills` field,
/// plus skills whose `applies_to` includes this agent's ID.
pub fn load_skills_for_agent(
    agent_config: &AgentConfig,
    all_skills: &HashMap<String, SkillConfig>,
) -> String {
    let mut skill_content = Vec::new();
    let mut included = std::collections::HashSet::new();

    // 1. Explicitly listed skills
    for skill_id in &agent_config.skills {
        if let Some(skill) = all_skills.get(skill_id) {
            if included.insert(skill_id.clone()) {
                skill_content.push(format!("--- SKILL: {} ---\n{}", skill.name, skill.content));
            }
        }
    }

    // 2. Skills that apply_to this agent
    for (id, skill) in all_skills {
        if skill.applies_to.contains(&agent_config.id) && included.insert(id.clone()) {
            skill_content.push(format!("--- SKILL: {} ---\n{}", skill.name, skill.content));
        }
    }

    if skill_content.is_empty() {
        return String::new();
    }

    // Budget: max 30K chars for skills
    let mut result = String::from("\n\n--- LOADED SKILLS ---\n");
    let mut total_len = result.len();
    const SKILL_BUDGET: usize = 30_000;

    for content in &skill_content {
        if total_len + content.len() > SKILL_BUDGET {
            result.push_str("\n[... remaining skills truncated due to budget ...]\n");
            break;
        }
        result.push_str(content);
        result.push('\n');
        total_len += content.len() + 1;
    }

    result.push_str("--- END SKILLS ---");
    result
}

/// Build complete agent instructions from Markdown config files.
///
/// This is the Markdown-based replacement for the hardcoded `instructions::build_agent_instructions`.
/// Falls back to the hardcoded version if no config exists for the agent.
///
/// `artifacts_dir` is the absolute path to `.grove/artifacts/{conversation_id}/{run_id}/`
/// where agents write their pipeline artifacts instead of the worktree root.
pub fn build_instructions_from_config(
    agent: AgentType,
    objective: &str,
    run_id: &str,
    artifacts_dir: &Path,
    handoff_context: Option<&str>,
    project_root: &Path,
    preloaded: Option<&ProjectConfigs>,
) -> String {
    let owned_configs;
    let configs = match preloaded {
        Some(c) => c,
        None => match load_all(project_root) {
            Ok(c) => {
                owned_configs = c;
                &owned_configs
            }
            Err(e) => {
                eprintln!("[WARN] failed to load configs, falling back to hardcoded: {e}");
                return crate::orchestrator::instructions::build_agent_instructions(
                    agent,
                    objective,
                    run_id,
                    artifacts_dir,
                    handoff_context,
                );
            }
        },
    };

    let agent_id = agent.as_str();
    let agent_config = match configs.agents.get(agent_id) {
        Some(c) => c,
        None => {
            return crate::orchestrator::instructions::build_agent_instructions(
                agent,
                objective,
                run_id,
                artifacts_dir,
                handoff_context,
            );
        }
    };

    let mut instructions = render_prompt(agent_config, objective, run_id, artifacts_dir);

    let upstream = render_upstream_context(agent_config, run_id, artifacts_dir);
    if !upstream.is_empty() {
        instructions.push_str("\n\n");
        instructions.push_str(&upstream);
    }

    let skills = load_skills_for_agent(agent_config, &configs.skills);
    if !skills.is_empty() {
        instructions.push_str(&skills);
    }

    if let Some(handoff) = handoff_context {
        if !handoff.is_empty() {
            instructions.push_str("\n\n");
            instructions.push_str(handoff);
        }
    }

    instructions
}

// ── Map helpers ─────────────────────────────────────────────────────────────

/// Map an agent config ID to its `AgentType` enum variant.
pub fn agent_type_from_config_id(id: &str) -> Option<AgentType> {
    AgentType::from_str(id)
}

/// Map a pipeline config ID to its `PipelineKind` enum variant.
pub fn pipeline_kind_from_config_id(id: &str) -> Option<PipelineKind> {
    PipelineKind::from_str(id)
}

// ── Internal helpers ────────────────────────────────────────────────────────

fn short_run_id(run_id: &str) -> &str {
    if run_id.len() >= 8 {
        &run_id[..8]
    } else {
        run_id
    }
}

fn default_true() -> bool {
    true
}

fn read_file(path: &Path) -> GroveResult<String> {
    fs::read_to_string(path)
        .map_err(|e| GroveError::Config(format!("failed to read {}: {e}", path.display())))
}

fn parse_yaml<T: serde::de::DeserializeOwned>(yaml: &str, path: &Path) -> GroveResult<T> {
    serde_yaml::from_str(yaml).map_err(|e| {
        GroveError::Config(format!(
            "failed to parse frontmatter in {}: {e}",
            path.display()
        ))
    })
}

/// Read a directory and return sorted file paths.
fn read_dir_sorted(dir: &Path) -> GroveResult<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| GroveError::Config(format!("failed to read dir {}: {e}", dir.display())))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    Ok(paths)
}

/// Reconstruct a Markdown agent file from config (frontmatter + prompt body).
fn build_agent_md(config: &AgentConfig) -> GroveResult<String> {
    // Build a clean frontmatter struct (without prompt, which goes in the body)
    #[derive(Serialize)]
    struct AgentFrontmatter<'a> {
        id: &'a str,
        name: &'a str,
        description: &'a str,
        can_write: bool,
        can_run_commands: bool,
        artifact: &'a Option<String>,
        allowed_tools: &'a Option<Vec<String>>,
        skills: &'a Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        upstream_artifacts: &'a Vec<UpstreamArtifact>,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: &'a Option<crate::orchestrator::scope::ScopeConfig>,
    }

    let fm = AgentFrontmatter {
        id: &config.id,
        name: &config.name,
        description: &config.description,
        can_write: config.can_write,
        can_run_commands: config.can_run_commands,
        artifact: &config.artifact,
        allowed_tools: &config.allowed_tools,
        skills: &config.skills,
        upstream_artifacts: &config.upstream_artifacts,
        scope: &config.scope,
    };

    let yaml = serde_yaml::to_string(&fm)
        .map_err(|e| GroveError::Config(format!("failed to serialize agent frontmatter: {e}")))?;

    let mut output = String::new();
    output.push_str("---\n");
    output.push_str(&yaml);
    output.push_str("---\n\n");
    output.push_str(&config.prompt);
    if !output.ends_with('\n') {
        output.push('\n');
    }

    Ok(output)
}

/// Reconstruct a Markdown pipeline file from config.
fn build_pipeline_md(config: &PipelineConfig) -> GroveResult<String> {
    #[derive(Serialize)]
    struct PipelineFrontmatter<'a> {
        id: &'a str,
        name: &'a str,
        description: &'a str,
        default: bool,
        agents: &'a Vec<String>,
        gates: &'a Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        aliases: &'a Vec<String>,
    }

    let fm = PipelineFrontmatter {
        id: &config.id,
        name: &config.name,
        description: &config.description,
        default: config.default,
        agents: &config.agents,
        gates: &config.gates,
        aliases: &config.aliases,
    };

    let yaml = serde_yaml::to_string(&fm).map_err(|e| {
        GroveError::Config(format!("failed to serialize pipeline frontmatter: {e}"))
    })?;

    let mut output = String::new();
    output.push_str("---\n");
    output.push_str(&yaml);
    output.push_str("---\n\n");
    output.push_str(&config.content);
    if !output.ends_with('\n') {
        output.push('\n');
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn split_frontmatter_basic() {
        let raw = "---\nid: test\nname: Test\n---\n\n# Body\nContent here.";
        let (fm, body) = split_frontmatter(raw);
        assert!(fm.contains("id: test"));
        assert!(body.contains("# Body"));
    }

    #[test]
    fn split_frontmatter_no_frontmatter() {
        let raw = "# Just content\nNo frontmatter.";
        let (fm, body) = split_frontmatter(raw);
        assert!(fm.is_empty());
        assert_eq!(body, raw);
    }

    #[test]
    fn load_agents_from_md() {
        let tmp = tempfile::TempDir::new().unwrap();
        let agents_dir = tmp.path().join("skills").join("agents");
        fs::create_dir_all(&agents_dir).unwrap();

        let md = r#"---
id: build_prd
name: Build PRD
description: Test agent
can_write: true
can_run_commands: false
artifact: "GROVE_PRD_{run_id}.md"
allowed_tools:
  - Read
  - Write
skills: []
---

# Build PRD Agent

You are the BUILD PRD agent.

Objective: {objective}

Write `{artifact_filename}`.
"#;
        fs::write(agents_dir.join("build_prd.md"), md).unwrap();

        let agents = load_agents(tmp.path()).unwrap();
        assert_eq!(agents.len(), 1);
        let agent = agents.get("build_prd").unwrap();
        assert_eq!(agent.name, "Build PRD");
        assert!(agent.can_write);
        assert!(!agent.can_run_commands);
        assert_eq!(agent.allowed_tools.as_ref().unwrap().len(), 2);
        assert!(agent.prompt.contains("BUILD PRD agent"));
    }

    #[test]
    fn render_prompt_replaces_variables() {
        let config = AgentConfig {
            id: "build_prd".into(),
            name: "Build PRD".into(),
            description: "test".into(),
            can_write: true,
            can_run_commands: false,
            artifact: Some("GROVE_PRD_{run_id}.md".into()),
            allowed_tools: None,
            skills: vec![],
            upstream_artifacts: vec![],
            scope: None,
            prompt: "Agent for {objective}. Write {artifact_filename}. Run: {run_id}. Dir: {artifacts_dir}".into(),
        };

        let artifacts_dir = Path::new("/tmp/artifacts");
        let rendered = render_prompt(&config, "Add auth", "abc12345def", artifacts_dir);
        assert!(rendered.contains("Add auth"));
        assert!(rendered.contains("/tmp/artifacts/GROVE_PRD_abc12345.md"));
        assert!(rendered.contains("abc12345"));
        assert!(rendered.contains("/tmp/artifacts"));
    }

    #[test]
    fn load_pipelines_from_md() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pipelines_dir = tmp.path().join("skills").join("pipelines");
        fs::create_dir_all(&pipelines_dir).unwrap();

        let md = r#"---
id: build
name: Build Mode
description: Implementation + quality gates
agents:
  - builder
  - reviewer
  - judge
gates: []
default: false
aliases:
  - instant
---

# Build Mode Pipeline

Runs three agents sequentially.
"#;
        fs::write(pipelines_dir.join("build.md"), md).unwrap();

        let pipelines = load_pipelines(tmp.path()).unwrap();
        assert_eq!(pipelines.len(), 1);
        let pipeline = pipelines.get("build").unwrap();
        assert_eq!(pipeline.agents.len(), 3);
        assert!(pipeline.gates.is_empty());
        assert!(!pipeline.default);
        assert!(pipeline.content.contains("Build Mode Pipeline"));
    }

    #[test]
    fn load_skills_from_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skills").join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();

        let skill_md = "---\nname: my-skill\ndescription: Test\napplies_to:\n  - builder\n---\n\n# Content\nHello";
        fs::write(skill_dir.join("SKILL.md"), skill_md).unwrap();

        let skills = load_skills(tmp.path()).unwrap();
        assert_eq!(skills.len(), 1);
        let skill = skills.get("my-skill").unwrap();
        assert_eq!(skill.applies_to, vec!["builder"]);
        assert!(skill.content.contains("# Content"));
    }

    #[test]
    fn skills_loaded_for_agent() {
        let agent_config = AgentConfig {
            id: "builder".into(),
            name: "Builder".into(),
            description: "test".into(),
            can_write: true,
            can_run_commands: true,
            artifact: None,
            allowed_tools: None,
            skills: vec!["explicit-skill".into()],
            upstream_artifacts: vec![],
            scope: None,
            prompt: "test".into(),
        };

        let mut all_skills = HashMap::new();
        all_skills.insert(
            "explicit-skill".into(),
            SkillConfig {
                id: "explicit-skill".into(),
                name: "Explicit".into(),
                description: "Explicitly referenced".into(),
                applies_to: vec![],
                content: "Explicit content".into(),
            },
        );
        all_skills.insert(
            "auto-skill".into(),
            SkillConfig {
                id: "auto-skill".into(),
                name: "Auto".into(),
                description: "Auto-applied".into(),
                applies_to: vec!["builder".into()],
                content: "Auto content".into(),
            },
        );
        all_skills.insert(
            "unrelated-skill".into(),
            SkillConfig {
                id: "unrelated-skill".into(),
                name: "Unrelated".into(),
                description: "Not for builder".into(),
                applies_to: vec!["reviewer".into()],
                content: "Unrelated content".into(),
            },
        );

        let result = load_skills_for_agent(&agent_config, &all_skills);
        assert!(result.contains("Explicit content"));
        assert!(result.contains("Auto content"));
        assert!(!result.contains("Unrelated content"));
    }

    #[test]
    fn upstream_context_with_existing_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let artifacts_dir = tmp.path().join("artifacts");
        fs::create_dir_all(&artifacts_dir).unwrap();
        fs::write(artifacts_dir.join("GROVE_PRD_abc12345.md"), "prd").unwrap();

        let config = AgentConfig {
            id: "plan_system_design".into(),
            name: "Plan".into(),
            description: "test".into(),
            can_write: true,
            can_run_commands: false,
            artifact: None,
            allowed_tools: None,
            skills: vec![],
            upstream_artifacts: vec![UpstreamArtifact {
                label: "PRD".into(),
                filename: "GROVE_PRD_{run_id}.md".into(),
            }],
            scope: None,
            prompt: "test".into(),
        };

        let ctx = render_upstream_context(&config, "abc12345def", &artifacts_dir);
        assert!(ctx.contains("GROVE_PRD_abc12345.md"));
        assert!(ctx.contains("UPSTREAM ARTIFACTS"));
    }

    #[test]
    fn save_and_reload_agent() {
        let tmp = tempfile::TempDir::new().unwrap();

        let config = AgentConfig {
            id: "test_agent".into(),
            name: "Test Agent".into(),
            description: "A test agent".into(),
            can_write: true,
            can_run_commands: false,
            artifact: Some("TEST_{run_id}.md".into()),
            allowed_tools: Some(vec!["Read".into(), "Write".into()]),
            skills: vec![],
            upstream_artifacts: vec![],
            scope: None,
            prompt: "# Test Agent\n\nYou are a test agent for {objective}.\n".into(),
        };

        save_agent(tmp.path(), &config).unwrap();

        let agents = load_agents(tmp.path()).unwrap();
        let loaded = agents.get("test_agent").unwrap();
        assert_eq!(loaded.name, "Test Agent");
        assert!(loaded.prompt.contains("test agent"));
    }

    #[test]
    fn save_and_reload_pipeline() {
        let tmp = tempfile::TempDir::new().unwrap();

        let config = PipelineConfig {
            id: "test_pipe".into(),
            name: "Test Pipeline".into(),
            description: "A test pipeline".into(),
            agents: vec!["builder".into(), "reviewer".into()],
            gates: vec![],
            default: false,
            aliases: vec!["test".into()],
            content: "# Test Pipeline\n\nRuns two agents.\n".into(),
        };

        save_pipeline(tmp.path(), &config).unwrap();

        let pipelines = load_pipelines(tmp.path()).unwrap();
        let loaded = pipelines.get("test_pipe").unwrap();
        assert_eq!(loaded.agents.len(), 2);
        assert!(loaded.content.contains("Test Pipeline"));
    }
}
