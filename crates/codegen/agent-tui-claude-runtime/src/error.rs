use std::time::Duration;

pub type Result<T> = std::result::Result<T, ClaudeRuntimeError>;

#[derive(Debug, thiserror::Error)]
pub enum ClaudeRuntimeError {
    #[error("claude CLI not found on PATH (install Claude Code)")]
    BinaryNotFound,
    #[error("failed to spawn claude: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("claude timed out after {0:?}")]
    Timeout(Duration),
    #[error("claude exit status {code}: {stderr}")]
    Exit { code: i32, stderr: String },
    #[error("claude returned invalid JSON: {0}")]
    BadJson(String),
    #[error("claude API/runtime error: {0}")]
    Api(String),
    #[error("{0}")]
    Other(String),
}
