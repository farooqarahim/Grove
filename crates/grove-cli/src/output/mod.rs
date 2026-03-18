pub mod json;
pub mod text;

#[allow(dead_code)] // Variants used by CLI commands (Tasks 6+)
#[derive(Debug, Clone)]
pub enum OutputMode {
    Text { no_color: bool },
    Json,
}
