//! Warm connection pool for Codex app-server (always-on with idle timeout).

use crate::client::{ClientIdentity, CodexAppServerClient, TransportConfig};
use crate::error::Result;
use crate::protocol::{ThreadStartParams, TurnStartParams, UserInput};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, info};

/// Configuration for the warm pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub transport: TransportConfig,
    pub identity: ClientIdentity,
    /// Drop the connection after this much idle time. Default 15 minutes.
    pub idle_timeout: Duration,
    /// Optional model override for new threads.
    pub default_model: Option<String>,
    /// Working directory for threads (Agent TUI cwd).
    pub cwd: Option<String>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            transport: TransportConfig::default(),
            identity: ClientIdentity::default(),
            idle_timeout: Duration::from_secs(900),
            default_model: None,
            cwd: None,
        }
    }
}

/// Single-slot pool: one warm app-server connection, recreated on demand.
pub struct CodexRuntimePool {
    config: PoolConfig,
    inner: Mutex<PoolState>,
}

struct PoolState {
    client: Option<Arc<CodexAppServerClient>>,
    /// Active thread for quick multi-turn (optional sticky).
    sticky_thread_id: Option<String>,
}

impl CodexRuntimePool {
    pub fn new(config: PoolConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            inner: Mutex::new(PoolState {
                client: None,
                sticky_thread_id: None,
            }),
        })
    }

    /// Ensure a live initialized client exists (spawn if needed; recycle if idle).
    pub async fn ensure_ready(&self) -> Result<Arc<CodexAppServerClient>> {
        let mut state = self.inner.lock().await;

        if let Some(ref c) = state.client {
            if c.idle_for().await < self.config.idle_timeout {
                c.touch().await;
                return Ok(c.clone());
            }
            info!("codex runtime: idle timeout — recycling app-server connection");
            c.shutdown().await;
            state.client = None;
            state.sticky_thread_id = None;
        }

        debug!("codex runtime: spawning app-server");
        let client = CodexAppServerClient::connect(
            self.config.transport.clone(),
            self.config.identity.clone(),
        )
        .await?;
        state.client = Some(client.clone());
        Ok(client)
    }

    /// Start (or reuse sticky) thread and run a text turn. Returns initial turn result JSON.
    ///
    /// Callers that want streaming should [`CodexAppServerClient::subscribe`] **before**
    /// calling this, or use [`Self::start_text_turn_streaming`].
    pub async fn start_text_turn(&self, prompt: impl Into<String>) -> Result<(String, Value)> {
        let prompt = prompt.into();
        let client = self.ensure_ready().await?;

        let thread_id = {
            let mut state = self.inner.lock().await;
            if let Some(id) = state.sticky_thread_id.clone() {
                id
            } else {
                let id = client
                    .thread_start(ThreadStartParams {
                        model: self.config.default_model.clone(),
                        cwd: self.config.cwd.clone(),
                        approval_policy: Some("never".into()),
                        sandbox: Some("workspaceWrite".into()),
                        ephemeral: Some(false),
                        service_name: Some("agent_tui".into()),
                    })
                    .await?;
                state.sticky_thread_id = Some(id.clone());
                id
            }
        };

        let turn = client
            .turn_start(TurnStartParams {
                thread_id: thread_id.clone(),
                input: vec![UserInput::Text { text: prompt }],
                cwd: self.config.cwd.clone(),
            })
            .await?;
        Ok((thread_id, turn))
    }

    /// Force-drop the warm connection (e.g. on logout or config change).
    pub async fn shutdown(&self) {
        let mut state = self.inner.lock().await;
        if let Some(c) = state.client.take() {
            c.shutdown().await;
        }
        state.sticky_thread_id = None;
    }

    /// Clear sticky thread so the next turn starts a fresh conversation.
    pub async fn reset_thread(&self) {
        self.inner.lock().await.sticky_thread_id = None;
    }
}
