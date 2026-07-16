//! # Codex app-server runtime for Agent TUI
//!
//! Long-lived JSON-RPC client to `codex app-server` with a warm connection
//! pool (idle timeout). Uses the local Codex CLI login — **no OAuth** in
//! this process.
//!
//! ```ignore
//! use agent_tui_codex_runtime::{CodexRuntimePool, PoolConfig};
//!
//! # async fn demo() -> agent_tui_codex_runtime::Result<()> {
//! let pool = CodexRuntimePool::new(PoolConfig::default());
//! let client = pool.ensure_ready().await?;
//! let mut _events = client.subscribe();
//! let (_thread_id, _turn) = pool.start_text_turn("Summarize this repo", None).await?;
//! # Ok(())
//! # }
//! ```
//!
//! See `docs/LOCAL_CLI_AUTH.md` for the multi-runtime design.

mod client;
mod error;
mod pool;
mod protocol;

pub use client::{
    ClientIdentity, CodexAppServerClient, TransportConfig, collect_turn_text,
};
pub use error::{CodexRuntimeError, Result};
pub use pool::{CodexRuntimePool, PoolConfig};
pub use protocol::{
    ClientInfo, CodexModelEntry, InitializeParams, ModelListParams, RuntimeEvent,
    ThreadStartParams, TurnStartParams, UserInput, map_notification,
};
