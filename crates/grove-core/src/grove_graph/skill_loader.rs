use std::path::Path;
use tracing::{error, warn};

/// Critical graph agent roles that must have skill files for correct operation.
const CRITICAL_ROLES: &[&str] = &[
    "phase-worker",
    "step-builder",
    "phase-validator",
    "phase-judge",
];

/// Load skill instructions for a graph agent role.
///
/// Resolution order:
/// 1. **Project-local**: `{project_root}/skills/graph/{skill_dir}/SKILL.md`
///    — allows per-project skill overrides.
/// 2. **Repo-root relative to exe** (dev builds): walk up from the current
///    executable to find a `skills/graph/` directory in the repo root.
///    In dev, the exe sits at `<repo>/target/debug/grove-gui` so walking two
///    levels up reaches the repo root.
/// 3. **Adjacent to exe** (packaged): `{exe_dir}/skills/graph/{skill_dir}/SKILL.md`
///    — for release bundles that ship skills next to the binary.
///
/// Returns an empty string (with a warning or error) if the skill file cannot
/// be found in any location.
pub fn load_skill(project_root: &Path, skill_dir: &str) -> String {
    let relative = Path::new("skills/graph").join(skill_dir).join("SKILL.md");

    // 1. Project-local
    let project_path = project_root.join(&relative);
    if let Ok(content) = std::fs::read_to_string(&project_path) {
        return content;
    }

    // 2. Repo-root heuristic (dev builds)
    //    exe is at <repo>/target/<profile>/grove-gui
    //    repo root is two levels up from exe dir
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // Walk up looking for skills/graph/ (handles nested target layouts)
            let mut ancestor = Some(exe_dir);
            for _ in 0..5 {
                if let Some(dir) = ancestor {
                    let candidate = dir.join(&relative);
                    if candidate.is_file() {
                        if let Ok(content) = std::fs::read_to_string(&candidate) {
                            return content;
                        }
                    }
                    ancestor = dir.parent();
                }
            }

            // 3. Adjacent to exe (packaged)
            let adjacent = exe_dir
                .join("skills")
                .join("graph")
                .join(skill_dir)
                .join("SKILL.md");
            if let Ok(content) = std::fs::read_to_string(&adjacent) {
                return content;
            }
        }
    }

    if CRITICAL_ROLES.iter().any(|r| skill_dir.contains(r)) {
        error!(
            skill_dir,
            project_root = %project_root.display(),
            "critical skill file missing — agent will have no behavioral guidance"
        );
    } else {
        warn!(
            skill_dir,
            project_root = %project_root.display(),
            "skill file not found in any search path — agent will run without skill instructions"
        );
    }
    String::new()
}
