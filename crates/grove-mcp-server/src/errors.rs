use std::fmt;

#[derive(Debug)]
pub enum McpError {
    /// Database query or write failed.
    Database { operation: String, cause: String },
    /// A required resource (run, graph, phase, step) was not found.
    NotFound { resource: String, id: String },
    /// Gate decision timed out.
    Timeout {
        operation: String,
        elapsed_secs: u64,
    },
    /// Invalid parameters in the tool call.
    InvalidParams { message: String },
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McpError::Database { operation, cause } => {
                write!(f, "database error during {operation}: {cause}")
            }
            McpError::NotFound { resource, id } => write!(f, "{resource} not found: {id}"),
            McpError::Timeout {
                operation,
                elapsed_secs,
            } => write!(f, "timeout after {elapsed_secs}s waiting for {operation}"),
            McpError::InvalidParams { message } => write!(f, "invalid parameters: {message}"),
        }
    }
}
