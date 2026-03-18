use std::path::Path;

/// Language ecosystem detected from the worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    Rust,
    Node,
    Go,
    Python,
}

/// Probe the worktree root for sentinel files and return all detected project types.
pub fn detect(worktree: &Path) -> Vec<ProjectType> {
    let mut types = Vec::new();

    if worktree.join("Cargo.toml").exists() {
        types.push(ProjectType::Rust);
    }
    if worktree.join("package.json").exists() {
        types.push(ProjectType::Node);
    }
    if worktree.join("go.mod").exists() {
        types.push(ProjectType::Go);
    }
    if worktree.join("pyproject.toml").exists() || worktree.join("setup.py").exists() {
        types.push(ProjectType::Python);
    }

    types
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(detect(tmp.path()).is_empty());
    }

    #[test]
    fn detect_rust_project() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
        let types = detect(tmp.path());
        assert_eq!(types, vec![ProjectType::Rust]);
    }

    #[test]
    fn detect_multiple_types() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "").unwrap();
        std::fs::write(tmp.path().join("package.json"), "{}").unwrap();
        let types = detect(tmp.path());
        assert!(types.contains(&ProjectType::Rust));
        assert!(types.contains(&ProjectType::Node));
    }
}
