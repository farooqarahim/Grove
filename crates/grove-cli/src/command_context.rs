use std::path::{Path, PathBuf};

use crate::cli::{Cli, OutputFormat};

#[derive(Debug, Clone)]
pub struct CommandContext {
    pub project_root: PathBuf,
    pub format: OutputFormat,
    pub _verbose: bool,
    pub _no_color: bool,
}

impl CommandContext {
    pub fn from_cli(cli: &Cli) -> Self {
        Self {
            project_root: absolute_path(&cli.project),
            format: cli.format,
            _verbose: cli.verbose,
            _no_color: cli.no_color,
        }
    }
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    match std::env::current_dir() {
        Ok(cwd) => cwd.join(path),
        Err(_) => path.to_path_buf(),
    }
}
