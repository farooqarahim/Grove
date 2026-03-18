use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)] // Variants used by CLI commands (Tasks 6+)
pub enum CliError {
    // grove_core::GroveError is re-exported via grove_core::lib.rs
    #[error("grove-core: {0}")]
    Core(#[from] grove_core::GroveError),

    #[error("transport: {0}")]
    Transport(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid argument: {0}")]
    BadArg(String),

    #[error("{0}")]
    Other(String),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::BadArg(_)    => 2,
            CliError::NotFound(_)  => 3,
            CliError::Transport(_) => 4,
            _                      => 1,
        }
    }
}

pub type CliResult<T> = std::result::Result<T, CliError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_arg_exits_2() {
        assert_eq!(CliError::BadArg("x".into()).exit_code(), 2);
    }

    #[test]
    fn not_found_exits_3() {
        assert_eq!(CliError::NotFound("run".into()).exit_code(), 3);
    }

    #[test]
    fn transport_exits_4() {
        assert_eq!(CliError::Transport("sock".into()).exit_code(), 4);
    }

    #[test]
    fn other_exits_1() {
        assert_eq!(CliError::Other("oops".into()).exit_code(), 1);
    }
}
