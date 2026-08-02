//! Warm connection pool for Codex app-server (always-on with idle timeout).

use crate::client::{ClientIdentity, CodexAppServerClient, TransportConfig};
use crate::error::Result;
use crate::protocol::{ThreadStartParams, TurnStartParams, UserInput};
use serde_json::Value;
use std::collections::HashMap;
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

/// Permission contract inherited from the Agent TUI session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionMode {
    #[default]
    Ask,
    Auto,
    AlwaysApprove,
}

impl PermissionMode {
    fn thread_policy(self) -> (&'static str, &'static str, Option<&'static str>) {
        match self {
            // Agent TUI does not yet render app-server approval requests. The
            // client explicitly rejects any request it receives, so Ask fails
            // closed instead of silently bypassing the approval contract.
            Self::Ask => ("on-request", "workspace-write", Some("user")),
            Self::Auto => ("on-request", "workspace-write", Some("auto_review")),
            Self::AlwaysApprove => ("never", "danger-full-access", None),
        }
    }
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
    /// Agent TUI session key → isolated Codex thread.
    sticky_threads: HashMap<String, StickyThread>,
}

struct StickyThread {
    thread_id: String,
    model: Option<String>,
    permission_mode: PermissionMode,
}

const DEFAULT_STICKY_KEY: &str = "default";

impl CodexRuntimePool {
    pub fn new(config: PoolConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            inner: Mutex::new(PoolState {
                client: None,
                sticky_threads: HashMap::new(),
            }),
        })
    }

    /// Ensure a live initialized client exists (spawn if needed; recycle if idle).
    pub async fn ensure_ready(&self) -> Result<Arc<CodexAppServerClient>> {
        let mut state = self.inner.lock().await;

        if let Some(ref c) = state.client {
            if c.idle_for().await < self.config.idle_timeout && c.is_healthy().await {
                c.touch().await;
                return Ok(c.clone());
            }
            info!("codex runtime: recycling idle or unhealthy app-server connection");
            c.shutdown().await;
            state.client = None;
            state.sticky_threads.clear();
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

    /// List models from the warm app-server.
    pub async fn list_models(
        &self,
        include_hidden: bool,
    ) -> Result<Vec<crate::protocol::CodexModelEntry>> {
        let client = self.ensure_ready().await?;
        client.model_list(include_hidden).await
    }

    /// Start (or reuse sticky) thread and run a text turn. Returns initial turn result JSON.
    ///
    /// Callers that want streaming should [`CodexAppServerClient::subscribe`] **before**
    /// calling this. Pass `model` to pin the thread model; if it differs from the
    /// sticky thread's model, a new thread is started.
    pub async fn start_text_turn(
        &self,
        prompt: impl Into<String>,
        model: Option<String>,
    ) -> Result<(String, Value)> {
        self.start_text_turn_keyed(prompt, model, None, PermissionMode::Ask)
            .await
    }

    /// Start a turn with continuity isolated by the Agent TUI session key.
    pub async fn start_text_turn_keyed(
        &self,
        prompt: impl Into<String>,
        model: Option<String>,
        sticky_key: Option<&str>,
        permission_mode: PermissionMode,
    ) -> Result<(String, Value)> {
        let prompt = prompt.into();
        let client = self.ensure_ready().await?;
        let model = model.or_else(|| self.config.default_model.clone());
        let key = sticky_key
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .unwrap_or(DEFAULT_STICKY_KEY)
            .to_string();

        let thread_id = {
            let mut state = self.inner.lock().await;
            let reuse = state
                .sticky_threads
                .get(&key)
                .filter(|thread| thread.model == model && thread.permission_mode == permission_mode)
                .map(|thread| thread.thread_id.clone());
            if let Some(thread_id) = reuse {
                thread_id
            } else {
                let (approval_policy, sandbox, approvals_reviewer) =
                    permission_mode.thread_policy();
                let id = client
                    .thread_start(ThreadStartParams {
                        model: model.clone(),
                        cwd: self.config.cwd.clone(),
                        approval_policy: Some(approval_policy.into()),
                        approvals_reviewer: approvals_reviewer.map(str::to_string),
                        sandbox: Some(sandbox.into()),
                        ephemeral: Some(false),
                        service_name: Some("agent_tui".into()),
                    })
                    .await?;
                state.sticky_threads.insert(
                    key,
                    StickyThread {
                        thread_id: id.clone(),
                        model,
                        permission_mode,
                    },
                );
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
        state.sticky_threads.clear();
    }

    /// Clear sticky thread so the next turn starts a fresh conversation.
    pub async fn reset_thread(&self) {
        self.inner.lock().await.sticky_threads.clear();
    }

    /// Interrupt the active Codex turn for one Agent TUI session.
    pub async fn interrupt_key(&self, sticky_key: &str) -> Result<()> {
        let (client, thread_id) = {
            let state = self.inner.lock().await;
            let client = state.client.clone();
            let thread_id = state
                .sticky_threads
                .get(sticky_key)
                .map(|thread| thread.thread_id.clone());
            (client, thread_id)
        };
        if let (Some(client), Some(thread_id)) = (client, thread_id) {
            client.turn_interrupt(thread_id).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_modes_map_without_implicit_bypass() {
        assert_eq!(
            PermissionMode::Ask.thread_policy(),
            ("on-request", "workspace-write", Some("user"))
        );
        assert_eq!(
            PermissionMode::Auto.thread_policy(),
            ("on-request", "workspace-write", Some("auto_review"))
        );
        assert_eq!(
            PermissionMode::AlwaysApprove.thread_policy(),
            ("never", "danger-full-access", None)
        );
    }
}
