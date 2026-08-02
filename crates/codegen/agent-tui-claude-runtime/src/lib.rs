//! # Claude Agent SDK runtime for Agent TUI
//!
//! Drives the **Claude Code / Claude Agent SDK harness** via the local
//! `claude` CLI (`claude -p --output-format json`). Auth is whatever Claude
//! Code already has (keychain / `~/.claude`) — **no Agent TUI OAuth**.
//!
//! Multi-turn uses sticky `--resume <session_id>`. Idle timeout drops the
//! sticky session so the next turn starts fresh.

mod error;
mod pool;
mod protocol;

pub use error::{ClaudeRuntimeError, Result};
pub use pool::{ClaudeRuntimePool, PermissionMode, PoolConfig};
pub use protocol::{
    ClaudeModelEntry, ClaudeTurnResult, DiscoveredModel, KNOWN_MODELS, discover_models,
};
