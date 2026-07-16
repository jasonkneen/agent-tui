//! Errors for the Codex app-server client.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodexRuntimeError {
    #[error("codex binary not found on PATH (install Codex CLI)")]
    CodexNotFound,

    #[error("failed to spawn codex app-server: {0}")]
    Spawn(#[source] std::io::Error),

    #[error("app-server I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON encode/decode: {0}")]
    Json(#[from] serde_json::Error),

    #[error("app-server request timed out after {0:?}")]
    Timeout(std::time::Duration),

    #[error("app-server RPC error {code}: {message}")]
    Rpc { code: i64, message: String },

    #[error("app-server closed while waiting for response id={0}")]
    ConnectionClosed(String),

    #[error("app-server not initialized")]
    NotInitialized,

    #[error("app-server already initialized")]
    AlreadyInitialized,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CodexRuntimeError>;
