use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// Permission mode captured from the Agent TUI session for one external turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuntimePermissionMode {
    #[default]
    Ask,
    Auto,
    AlwaysApprove,
}

struct InFlightTurn {
    generation: u64,
    cancel: tokio::sync::oneshot::Sender<()>,
}

static NEXT_TURN_GENERATION: AtomicU64 = AtomicU64::new(1);
static IN_FLIGHT_TURNS: OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, InFlightTurn>>,
> = OnceLock::new();

fn in_flight_turns() -> &'static std::sync::Mutex<std::collections::HashMap<String, InFlightTurn>> {
    IN_FLIGHT_TURNS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn in_flight_key(runtime: RuntimeBackend, sticky_key: &str) -> String {
    format!("{}:{sticky_key}", runtime.as_str())
}

/// Signal cancellation for an external turn. Spawn-per-turn futures drop
/// their kill-on-drop child; Codex also sends `turn/interrupt` from the
/// registered turn task before it finishes.
pub fn cancel_external_turn(runtime: RuntimeBackend, sticky_key: &str) -> bool {
    let key = in_flight_key(runtime, sticky_key);
    let entry = in_flight_turns()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&key);
    entry.is_some_and(|entry| entry.cancel.send(()).is_ok())
}

/// Run one text turn on the active non-Grok runtime.
///
/// `sticky_key` isolates multi-agent continuity for spawn-per-turn runtimes
/// that support sticky resume. Pass the Agent TUI session id.
pub async fn run_external_turn(runtime: RuntimeBackend, text: String) -> Result<String, String> {
    run_external_turn_keyed_with_permission(runtime, text, None, RuntimePermissionMode::Ask).await
}

/// Like [`run_external_turn`], with an optional sticky key (Agent TUI session id).
pub async fn run_external_turn_keyed(
    runtime: RuntimeBackend,
    text: String,
    sticky_key: Option<String>,
) -> Result<String, String> {
    run_external_turn_keyed_with_permission(runtime, text, sticky_key, RuntimePermissionMode::Ask)
        .await
}

pub async fn run_external_turn_keyed_with_permission(
    runtime: RuntimeBackend,
    text: String,
    sticky_key: Option<String>,
    permission_mode: RuntimePermissionMode,
) -> Result<String, String> {
    run_external_turn_keyed_with_delta(runtime, text, sticky_key, permission_mode, None).await
}

/// Like [`run_external_turn_keyed_with_permission`], with a live text sink.
/// Lazar/Claude/Hermes call it while the child streams; Codex while
/// `item/agentMessage/delta` notifications arrive.
pub async fn run_external_turn_keyed_with_delta(
    runtime: RuntimeBackend,
    text: String,
    sticky_key: Option<String>,
    permission_mode: RuntimePermissionMode,
    on_delta: Option<Box<dyn FnMut(&str) + Send>>,
) -> Result<String, String> {
    let Some(registry_key) = sticky_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .map(|key| in_flight_key(runtime, key))
    else {
        return run_external_turn_inner(runtime, text, sticky_key, permission_mode, on_delta).await;
    };
    let generation = NEXT_TURN_GENERATION.fetch_add(1, Ordering::Relaxed);
    let (cancel, cancel_rx) = tokio::sync::oneshot::channel();
    let previous = in_flight_turns()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(registry_key.clone(), InFlightTurn { generation, cancel });
    if let Some(previous) = previous {
        let _ = previous.cancel.send(());
    }

    let sticky_for_cancel = sticky_key.clone().unwrap_or_default();
    let result = tokio::select! {
        result = run_external_turn_inner(runtime, text, sticky_key, permission_mode, on_delta) => result,
        _ = cancel_rx => {
            if runtime == RuntimeBackend::Codex {
                codex_pool()
                    .interrupt_key(&sticky_for_cancel)
                    .await
                    .map_err(|error| format!("Codex turn interrupt failed: {error}"))?;
            }
            Err(format!("{} turn cancelled", runtime.display_name()))
        }
    };

    let mut turns = in_flight_turns()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if turns
        .get(&registry_key)
        .is_some_and(|entry| entry.generation == generation)
    {
        turns.remove(&registry_key);
    }
    result
}

async fn run_external_turn_inner(
    runtime: RuntimeBackend,
    text: String,
    sticky_key: Option<String>,
    permission_mode: RuntimePermissionMode,
    on_delta: Option<Box<dyn FnMut(&str) + Send>>,
) -> Result<String, String> {
    match runtime {
        RuntimeBackend::Grok => {
            Err("internal: run_external_turn called with Grok (use ACP path)".into())
        }
        RuntimeBackend::Claude => {
            let pool = claude_pool();
            let model = claude_model();
            let perm = match permission_mode {
                RuntimePermissionMode::Ask => agent_tui_claude_runtime::PermissionMode::Ask,
                RuntimePermissionMode::Auto => agent_tui_claude_runtime::PermissionMode::Auto,
                RuntimePermissionMode::AlwaysApprove => {
                    agent_tui_claude_runtime::PermissionMode::AlwaysApprove
                }
            };
            let mut on_delta = on_delta.unwrap_or_else(|| Box::new(|_| {}));
            let result = pool
                .start_text_turn_with_delta(text, model, sticky_key.as_deref(), perm, move |c| {
                    on_delta(c)
                })
                .await
                .map_err(|e| format!("Claude turn failed: {e}"))?;
            Ok(result.text)
        }
        RuntimeBackend::Codex => {
            let pool = codex_pool();
            let client = pool
                .ensure_ready()
                .await
                .map_err(|e| format!("Codex app-server: {e}"))?;
            let mut rx = client.subscribe();
            let model = codex_model();
            let (thread_id, turn) = pool
                .start_text_turn_keyed(
                    text,
                    model,
                    sticky_key.as_deref(),
                    match permission_mode {
                        RuntimePermissionMode::Ask => agent_tui_codex_runtime::PermissionMode::Ask,
                        RuntimePermissionMode::Auto => {
                            agent_tui_codex_runtime::PermissionMode::Auto
                        }
                        RuntimePermissionMode::AlwaysApprove => {
                            agent_tui_codex_runtime::PermissionMode::AlwaysApprove
                        }
                    },
                )
                .await
                .map_err(|e| format!("Codex turn start failed: {e}"))?;
            let turn_id = turn
                .pointer("/turn/id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("Codex turn/start missing turn.id: {turn}"))?;
            let mut on_delta = on_delta.unwrap_or_else(|| Box::new(|_| {}));
            agent_tui_codex_runtime::collect_turn_text_for_with_delta(
                &mut rx,
                &thread_id,
                turn_id,
                std::time::Duration::from_secs(600),
                move |c| on_delta(c),
            )
            .await
            .map_err(|e| format!("Codex turn failed: {e}"))
        }
        RuntimeBackend::Lazar => {
            let pool = lazar_pool();
            let model = lazar_model();
            let perm = match permission_mode {
                RuntimePermissionMode::AlwaysApprove => {
                    agent_tui_lazar_runtime::PermissionMode::AlwaysApprove
                }
                RuntimePermissionMode::Auto => agent_tui_lazar_runtime::PermissionMode::Auto,
                RuntimePermissionMode::Ask => agent_tui_lazar_runtime::PermissionMode::Ask,
            };
            let mut on_delta = on_delta.unwrap_or_else(|| Box::new(|_| {}));
            let result = pool
                .start_text_turn_with_delta(
                    &text,
                    model.as_deref(),
                    sticky_key.as_deref(),
                    perm,
                    move |chunk| on_delta(chunk),
                )
                .await
                .map_err(|e| format!("Lazar turn failed: {e}"))?;
            Ok(result.text)
        }
        RuntimeBackend::Hermes => {
            let pool = hermes_pool();
            let model = hermes_model().map(|m| agent_tui_hermes_runtime::normalize_model_id(&m));
            let model_ref = model.as_deref().filter(|m| !m.is_empty());
            let perm = match permission_mode {
                RuntimePermissionMode::AlwaysApprove => {
                    agent_tui_hermes_runtime::PermissionMode::AlwaysApprove
                }
                RuntimePermissionMode::Auto => agent_tui_hermes_runtime::PermissionMode::Auto,
                RuntimePermissionMode::Ask => agent_tui_hermes_runtime::PermissionMode::Ask,
            };
            let mut on_delta = on_delta.unwrap_or_else(|| Box::new(|_| {}));
            let result = pool
                .start_text_turn_with_delta(&text, model_ref, sticky_key.as_deref(), perm, move |c| {
                    on_delta(c)
                })
                .await
                .map_err(|e| format!("Hermes turn failed: {e}"))?;
            Ok(result.text)
        }
    }
}
