//! Active agent **runtime** selection (Grok vs Codex app-server vs Claude).
//!
//! Persisted to `~/.agent-tui/runtime.toml`. Used by `/runtime`, `/model` (when
//! Codex), and the send-prompt path to route turns without Agent TUI OAuth.

use agent_client_protocol as acp;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::sync::OnceLock;

/// Which harness handles user turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeBackend {
    /// Built-in Grok / xAI agent (default).
    #[default]
    Grok,
    /// Local `codex app-server` (uses ~/.codex auth).
    Codex,
    /// Claude Agent SDK (detect-only until sidecar lands).
    Claude,
}

impl RuntimeBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Grok => "grok",
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Grok => "Grok (xAI)",
            Self::Codex => "Codex (app-server)",
            Self::Claude => "Claude (Agent SDK)",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "grok" | "xai" | "default" => Some(Self::Grok),
            "codex" | "chatgpt" | "openai" => Some(Self::Codex),
            "claude" | "anthropic" => Some(Self::Claude),
            _ => None,
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Grok, Self::Codex, Self::Claude]
    }
}

impl fmt::Display for RuntimeBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RuntimeFile {
    #[serde(default)]
    active: RuntimeBackend,
    /// Last selected Codex model id (`model/list` id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    codex_model: Option<String>,
    /// Last selected Claude model id / alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    claude_model: Option<String>,
}

#[derive(Debug, Clone)]
struct RuntimeMem {
    active: RuntimeBackend,
    codex_model: Option<String>,
    claude_model: Option<String>,
}

static MEM: RwLock<Option<RuntimeMem>> = RwLock::new(None);

/// Cached Codex catalog for `/model` while runtime is Codex.
static CODEX_CATALOG: OnceLock<RwLock<Option<crate::acp::model_state::ModelState>>> =
    OnceLock::new();
/// Grok catalog stashed when switching to Codex so we can restore it.
static GROK_STASH: OnceLock<RwLock<Option<crate::acp::model_state::ModelState>>> = OnceLock::new();

fn catalog_lock() -> &'static RwLock<Option<crate::acp::model_state::ModelState>> {
    CODEX_CATALOG.get_or_init(|| RwLock::new(None))
}

fn grok_stash_lock() -> &'static RwLock<Option<crate::acp::model_state::ModelState>> {
    GROK_STASH.get_or_init(|| RwLock::new(None))
}

fn runtime_file_path() -> PathBuf {
    agent_tui_config::grok_home().join("runtime.toml")
}

fn load_mem() -> RuntimeMem {
    {
        let guard = MEM.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref m) = *guard {
            return m.clone();
        }
    }
    let file = load_from_disk().unwrap_or_default();
    let mem = RuntimeMem {
        active: file.active,
        codex_model: file.codex_model,
        claude_model: file.claude_model,
    };
    if let Ok(mut g) = MEM.write() {
        *g = Some(mem.clone());
    }
    mem
}

fn store_mem(mem: RuntimeMem) -> Result<(), String> {
    save_to_disk(&RuntimeFile {
        active: mem.active,
        codex_model: mem.codex_model.clone(),
        claude_model: mem.claude_model.clone(),
    })?;
    if let Ok(mut g) = MEM.write() {
        *g = Some(mem);
    }
    Ok(())
}

/// Load active runtime from disk (once) and return it.
pub fn active() -> RuntimeBackend {
    load_mem().active
}

/// Selected Codex model id (if any).
pub fn codex_model() -> Option<String> {
    load_mem().codex_model
}

/// Selected Claude model id / alias (if any).
pub fn claude_model() -> Option<String> {
    load_mem().claude_model
}

/// Persist and set the active runtime.
pub fn set_active(backend: RuntimeBackend) -> Result<(), String> {
    let mut mem = load_mem();
    mem.active = backend;
    store_mem(mem)
}

/// Persist Codex model selection (and reset sticky thread so the next turn uses it).
pub fn set_codex_model(model_id: impl Into<String>) -> Result<(), String> {
    let model_id = model_id.into();
    let mut mem = load_mem();
    let changed = mem.codex_model.as_deref() != Some(model_id.as_str());
    mem.codex_model = Some(model_id.clone());
    store_mem(mem)?;
    if changed {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async {
                codex_pool().reset_thread().await;
            });
        }
    }
    sync_catalog_current(&model_id);
    Ok(())
}

/// Persist Claude model selection (and reset sticky session).
pub fn set_claude_model(model_id: impl Into<String>) -> Result<(), String> {
    let model_id = model_id.into();
    let mut mem = load_mem();
    let changed = mem.claude_model.as_deref() != Some(model_id.as_str());
    mem.claude_model = Some(model_id.clone());
    store_mem(mem)?;
    if changed {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async {
                claude_pool().reset_session().await;
            });
        }
    }
    sync_catalog_current(&model_id);
    Ok(())
}

fn sync_catalog_current(model_id: &str) {
    if let Ok(mut g) = catalog_lock().write() {
        if let Some(ref mut state) = *g {
            let id = acp::ModelId::new(Arc::from(model_id));
            if state.available.contains_key(&id) {
                state.set_current(id, None);
            }
        }
    }
}

fn load_from_disk() -> Option<RuntimeFile> {
    let path = runtime_file_path();
    let raw = std::fs::read_to_string(path).ok()?;
    toml::from_str(&raw).ok()
}

fn save_to_disk(file: &RuntimeFile) -> Result<(), String> {
    let path = runtime_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = toml::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(path, body).map_err(|e| e.to_string())
}

/// Status line for one backend (for `/runtime` listing).
pub struct RuntimeStatus {
    pub backend: RuntimeBackend,
    pub active: bool,
    pub ready: bool,
    pub detail: String,
}

/// Probe readiness of each backend for the UI.
pub fn status_list() -> Vec<RuntimeStatus> {
    let current = active();
    RuntimeBackend::all()
        .iter()
        .copied()
        .map(|backend| {
            let (ready, detail) = match backend {
                RuntimeBackend::Grok => (true, "built-in agent".into()),
                RuntimeBackend::Codex => codex_status(),
                RuntimeBackend::Claude => claude_status(),
            };
            RuntimeStatus {
                backend,
                active: backend == current,
                ready,
                detail,
            }
        })
        .collect()
}

fn codex_status() -> (bool, String) {
    let codex_ok = which_bin("codex");
    let auth_ok = std::path::Path::new(&std::env::var_os("HOME").unwrap_or_default())
        .join(".codex/auth.json")
        .is_file();
    let model = codex_model();
    match (codex_ok, auth_ok, model) {
        (true, true, Some(m)) => (true, format!("codex CLI + auth · model {m}")),
        (true, true, None) => (true, "codex CLI + ~/.codex/auth.json".into()),
        (true, false, _) => (false, "codex found; run `codex login`".into()),
        (false, true, _) => (false, "auth present; install `codex` on PATH".into()),
        (false, false, _) => (false, "install Codex CLI and run `codex login`".into()),
    }
}

fn claude_status() -> (bool, String) {
    // Turns go through the `claude` CLI, which owns OAuth refresh / keychain.
    // Do **not** require our local_cli harvest to see a non-expired access
    // token — that file is often stale while `claude -p` still works.
    if !which_bin("claude") {
        return (
            false,
            "install Claude Code (`claude` on PATH) and log in".into(),
        );
    }
    let model = claude_model();
    let auth_note = match agent_tui_shell::auth::detect_preferred_claude() {
        Some(c) => format!("{}", c.origin),
        None => {
            // Harvest missed or access token expired — CLI still handles auth.
            let home = std::env::var_os("HOME").unwrap_or_default();
            let creds = std::path::Path::new(&home).join(".claude/credentials.json");
            if creds.is_file() {
                "credentials.json (CLI refresh on use)".into()
            } else {
                "auth via Claude Code".into()
            }
        }
    };
    match model {
        Some(m) => (true, format!("claude CLI + {auth_note} · model {m}")),
        None => (true, format!("claude CLI + {auth_note}")),
    }
}

fn which_bin(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let p = dir.join(name);
                p.is_file()
            })
        })
        .unwrap_or(false)
}

/// Global Codex pool (lazy). One warm app-server per pager process.
pub fn codex_pool() -> Arc<agent_tui_codex_runtime::CodexRuntimePool> {
    static POOL: OnceLock<Arc<agent_tui_codex_runtime::CodexRuntimePool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let cwd = std::env::current_dir()
            .ok()
            .map(|p| p.display().to_string());
        agent_tui_codex_runtime::CodexRuntimePool::new(agent_tui_codex_runtime::PoolConfig {
            cwd,
            default_model: codex_model(),
            ..Default::default()
        })
    })
    .clone()
}

/// Global Claude pool (lazy). Sticky Claude Code sessions via `--resume`.
pub fn claude_pool() -> Arc<agent_tui_claude_runtime::ClaudeRuntimePool> {
    static POOL: OnceLock<Arc<agent_tui_claude_runtime::ClaudeRuntimePool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let cwd = std::env::current_dir().ok();
        agent_tui_claude_runtime::ClaudeRuntimePool::new(
            agent_tui_claude_runtime::PoolConfig {
                cwd,
                default_model: claude_model(),
                ..Default::default()
            },
        )
    })
    .clone()
}

/// Cached Codex model catalog (None until first successful fetch).
pub fn codex_catalog() -> Option<crate::acp::model_state::ModelState> {
    catalog_lock()
        .read()
        .ok()
        .and_then(|g| g.clone())
}

/// Stash the current Grok catalog before replacing it with Codex models.
pub fn stash_grok_catalog(state: crate::acp::model_state::ModelState) {
    if let Ok(mut g) = grok_stash_lock().write() {
        if g.is_none() {
            *g = Some(state);
        }
    }
}

/// Take the stashed Grok catalog (if any).
pub fn take_stashed_grok_catalog() -> Option<crate::acp::model_state::ModelState> {
    grok_stash_lock().write().ok().and_then(|mut g| g.take())
}

/// Convert app-server `model/list` rows into pager [`ModelState`].
pub fn model_state_from_codex_entries(
    entries: &[agent_tui_codex_runtime::CodexModelEntry],
    preferred: Option<&str>,
) -> crate::acp::model_state::ModelState {
    use crate::acp::model_state::ModelState;

    let mut available: IndexMap<acp::ModelId, acp::ModelInfo> = IndexMap::new();
    let mut default_id: Option<acp::ModelId> = None;

    for e in entries {
        if e.hidden {
            continue;
        }
        let id = acp::ModelId::new(Arc::from(e.id.as_str()));
        let mut meta = serde_json::Map::new();
        if !e.supported_reasoning_efforts.is_empty() {
            meta.insert("supportsReasoningEffort".into(), serde_json::Value::Bool(true));
            let efforts: Vec<serde_json::Value> = e
                .supported_reasoning_efforts
                .iter()
                .map(|level| {
                    serde_json::json!({
                        "id": level,
                        "name": level,
                        "value": level,
                    })
                })
                .collect();
            meta.insert(
                "reasoningEfforts".into(),
                serde_json::Value::Array(efforts),
            );
        }
        if let Some(ref d) = e.default_reasoning_effort {
            meta.insert(
                "defaultReasoningEffort".into(),
                serde_json::Value::String(d.clone()),
            );
            meta.insert(
                "reasoningEffort".into(),
                serde_json::Value::String(d.clone()),
            );
        }
        if !e.input_modalities.is_empty() {
            meta.insert(
                "inputModalities".into(),
                serde_json::Value::Array(
                    e.input_modalities
                        .iter()
                        .map(|m| serde_json::Value::String(m.clone()))
                        .collect(),
                ),
            );
        }
        let mut info = acp::ModelInfo::new(id.clone(), e.display_name.clone());
        if let Some(ref d) = e.description {
            info = info.description(d.clone());
        }
        if !meta.is_empty() {
            info = info.meta(Some(meta));
        }
        if e.is_default {
            default_id = Some(id.clone());
        }
        available.insert(id, info);
    }

    let preferred_id = preferred
        .map(|s| acp::ModelId::new(Arc::from(s)))
        .filter(|id| available.contains_key(id));
    let current = preferred_id
        .or(default_id)
        .or_else(|| available.keys().next().cloned());

    let mut state = ModelState::default();
    state.update_catalog(available, current.clone());
    if let Some(id) = current {
        state.set_current(id, None);
    }
    state
}

/// Fetch Codex models into the cache. Returns the catalog for dispatch to apply.
pub async fn refresh_codex_models() -> Result<crate::acp::model_state::ModelState, String> {
    let pool = codex_pool();
    let entries = pool
        .list_models(false)
        .await
        .map_err(|e| format!("Codex model/list failed: {e}"))?;
    if entries.is_empty() {
        return Err("Codex model/list returned no models".into());
    }
    let preferred = codex_model();
    let state = model_state_from_codex_entries(&entries, preferred.as_deref());
    if codex_model().is_none() {
        if let Some(id) = state.current_model_id_str() {
            let _ = set_codex_model(id);
        }
    }
    if let Ok(mut g) = catalog_lock().write() {
        *g = Some(state.clone());
    }
    Ok(state)
}

/// Load Claude model catalog (Claude Code cache + built-ins + selected).
pub fn refresh_claude_models() -> Result<crate::acp::model_state::ModelState, String> {
    claude_pool()
        .ensure_binary()
        .map_err(|e| e.to_string())?;
    let preferred = claude_model();
    let state = model_state_from_claude_discovered(preferred.as_deref());
    if claude_model().is_none() {
        if let Some(id) = state.current_model_id_str() {
            let _ = set_claude_model(id);
        }
    }
    if let Ok(mut g) = catalog_lock().write() {
        *g = Some(state.clone());
    }
    Ok(state)
}

/// Build ModelState from Claude discovery (cache + known aliases).
pub fn model_state_from_claude_discovered(
    preferred: Option<&str>,
) -> crate::acp::model_state::ModelState {
    use crate::acp::model_state::ModelState;
    let discovered = agent_tui_claude_runtime::discover_models();
    let mut available: IndexMap<acp::ModelId, acp::ModelInfo> = IndexMap::new();
    let mut default_id: Option<acp::ModelId> = None;
    for e in discovered {
        let id = acp::ModelId::new(Arc::from(e.id.as_str()));
        let mut info = acp::ModelInfo::new(id.clone(), e.display_name);
        if let Some(d) = e.description {
            info = info.description(d);
        }
        if e.is_default {
            default_id = Some(id.clone());
        }
        available.insert(id, info);
    }
    let preferred_id = preferred
        .map(|s| acp::ModelId::new(Arc::from(s)))
        .filter(|id| available.contains_key(id));
    // Custom full id not in the table — still selectable.
    let current = if preferred_id.is_none() {
        if let Some(p) = preferred {
            let id = acp::ModelId::new(Arc::from(p));
            if !available.contains_key(&id) {
                available.insert(
                    id.clone(),
                    acp::ModelInfo::new(id.clone(), p.to_string()),
                );
            }
            Some(id)
        } else {
            default_id.or_else(|| available.keys().next().cloned())
        }
    } else {
        preferred_id.or(default_id)
    };
    let mut state = ModelState::default();
    state.update_catalog(available, current.clone());
    if let Some(id) = current {
        state.set_current(id, None);
    }
    state
}

/// Back-compat alias.
pub fn model_state_from_claude_known(
    preferred: Option<&str>,
) -> crate::acp::model_state::ModelState {
    model_state_from_claude_discovered(preferred)
}

/// Run one text turn on the active non-Grok runtime.
pub async fn run_external_turn(
    runtime: RuntimeBackend,
    text: String,
) -> Result<String, String> {
    match runtime {
        RuntimeBackend::Grok => Err(
            "internal: run_external_turn called with Grok (use ACP path)".into(),
        ),
        RuntimeBackend::Claude => {
            let pool = claude_pool();
            let model = claude_model();
            let result = pool
                .start_text_turn(text, model)
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
            pool.start_text_turn(text, model)
                .await
                .map_err(|e| format!("Codex turn start failed: {e}"))?;
            agent_tui_codex_runtime::collect_turn_text(
                &mut rx,
                std::time::Duration::from_secs(600),
            )
            .await
            .map_err(|e| format!("Codex turn failed: {e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_runtime_aliases() {
        assert_eq!(RuntimeBackend::parse("grok"), Some(RuntimeBackend::Grok));
        assert_eq!(RuntimeBackend::parse("xai"), Some(RuntimeBackend::Grok));
        assert_eq!(RuntimeBackend::parse("codex"), Some(RuntimeBackend::Codex));
        assert_eq!(
            RuntimeBackend::parse("openai"),
            Some(RuntimeBackend::Codex)
        );
        assert_eq!(
            RuntimeBackend::parse("claude"),
            Some(RuntimeBackend::Claude)
        );
        assert_eq!(
            RuntimeBackend::parse("anthropic"),
            Some(RuntimeBackend::Claude)
        );
        assert_eq!(RuntimeBackend::parse("nope"), None);
    }

    #[test]
    fn status_list_includes_all_backends() {
        let list = status_list();
        assert_eq!(list.len(), 3);
        assert!(list.iter().any(|s| s.backend == RuntimeBackend::Grok));
        assert!(list.iter().any(|s| s.backend == RuntimeBackend::Codex));
        assert!(list.iter().any(|s| s.backend == RuntimeBackend::Claude));
        assert!(list.iter().any(|s| s.active));
    }

    #[test]
    fn codex_entries_become_model_state() {
        let entries = vec![
            agent_tui_codex_runtime::CodexModelEntry {
                id: "gpt-5.4".into(),
                display_name: "GPT-5.4".into(),
                description: Some("flagship".into()),
                is_default: true,
                hidden: false,
                default_reasoning_effort: Some("medium".into()),
                supported_reasoning_efforts: vec!["low".into(), "medium".into(), "high".into()],
                input_modalities: vec!["text".into(), "image".into()],
            },
            agent_tui_codex_runtime::CodexModelEntry {
                id: "hidden-x".into(),
                display_name: "Hidden".into(),
                description: None,
                is_default: false,
                hidden: true,
                default_reasoning_effort: None,
                supported_reasoning_efforts: vec![],
                input_modalities: vec!["text".into()],
            },
        ];
        let state = model_state_from_codex_entries(&entries, None);
        assert_eq!(state.available.len(), 1);
        assert_eq!(state.current_model_id_str(), Some("gpt-5.4"));
        assert_eq!(state.current_model_name().as_deref(), Some("GPT-5.4"));
    }
}
