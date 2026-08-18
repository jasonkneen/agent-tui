//! Active agent **runtime addon** selection (Grok / Codex / Claude / Lazar / Hermes).
//!
//! Architecture: **ONE CORE + ADDONS** — see [`crate::runtime_addon`] and
//! `docs/CORE_AND_ADDONS.md`. This module holds the active addon id, model
//! picks, readiness probes, and turn dispatch.
//!
//! Persisted to `~/.agent-tui/runtime.toml`. Used by `/runtime`, `/model`, and
//! the send-prompt path to route turns without Agent TUI OAuth.

use agent_client_protocol as acp;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::{Arc, RwLock};

mod turns;
pub use turns::{
    RuntimePermissionMode, cancel_external_turn, run_external_turn, run_external_turn_keyed,
    run_external_turn_keyed_with_delta, run_external_turn_keyed_with_permission,
};

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
    /// Local `lazar` kernel (spawn-per-turn; providers/models owned by the kernel).
    Lazar,
    /// Local Hermes Agent CLI (`hermes chat -q`; sticky `--resume`).
    Hermes,
}

impl RuntimeBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Grok => "grok",
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Lazar => "lazar",
            Self::Hermes => "hermes",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Grok => "Grok (xAI)",
            Self::Codex => "Codex (app-server)",
            Self::Claude => "Claude (Agent SDK)",
            Self::Lazar => "Lazar (kernel)",
            Self::Hermes => "Hermes (agent)",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "grok" | "xai" | "default" => Some(Self::Grok),
            "codex" | "chatgpt" | "openai" => Some(Self::Codex),
            "claude" | "anthropic" => Some(Self::Claude),
            "lazar" => Some(Self::Lazar),
            "hermes" => Some(Self::Hermes),
            _ => None,
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::Grok,
            Self::Codex,
            Self::Claude,
            Self::Lazar,
            Self::Hermes,
        ]
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
    /// Last selected Lazar model id (kernel `--model`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lazar_model: Option<String>,
    /// Last selected Hermes model id (`hermes chat -m`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hermes_model: Option<String>,
}

#[derive(Debug, Clone)]
struct RuntimeMem {
    active: RuntimeBackend,
    codex_model: Option<String>,
    claude_model: Option<String>,
    lazar_model: Option<String>,
    hermes_model: Option<String>,
}

static MEM: RwLock<Option<RuntimeMem>> = RwLock::new(None);

/// Last live catalog per vendor. One shared slot used to paint Claude with
/// leftover Codex ids (and the reverse) after `/runtime`.
#[derive(Default)]
struct VendorCatalogs {
    codex: Option<crate::acp::model_state::ModelState>,
    claude: Option<crate::acp::model_state::ModelState>,
    lazar: Option<crate::acp::model_state::ModelState>,
    hermes: Option<crate::acp::model_state::ModelState>,
}

static VENDOR_CATALOGS: OnceLock<RwLock<VendorCatalogs>> = OnceLock::new();
/// Grok catalog stashed when leaving Grok so `/runtime grok` can restore it.
static GROK_STASH: OnceLock<RwLock<Option<crate::acp::model_state::ModelState>>> = OnceLock::new();

fn vendor_catalogs() -> &'static RwLock<VendorCatalogs> {
    VENDOR_CATALOGS.get_or_init(|| RwLock::new(VendorCatalogs::default()))
}

fn grok_stash_lock() -> &'static RwLock<Option<crate::acp::model_state::ModelState>> {
    GROK_STASH.get_or_init(|| RwLock::new(None))
}

fn vendor_slot_mut(
    map: &mut VendorCatalogs,
    backend: RuntimeBackend,
) -> Option<&mut Option<crate::acp::model_state::ModelState>> {
    match backend {
        RuntimeBackend::Codex => Some(&mut map.codex),
        RuntimeBackend::Claude => Some(&mut map.claude),
        RuntimeBackend::Lazar => Some(&mut map.lazar),
        RuntimeBackend::Hermes => Some(&mut map.hermes),
        RuntimeBackend::Grok => None,
    }
}

fn vendor_slot<'a>(
    map: &'a VendorCatalogs,
    backend: RuntimeBackend,
) -> Option<&'a crate::acp::model_state::ModelState> {
    match backend {
        RuntimeBackend::Codex => map.codex.as_ref(),
        RuntimeBackend::Claude => map.claude.as_ref(),
        RuntimeBackend::Lazar => map.lazar.as_ref(),
        RuntimeBackend::Hermes => map.hermes.as_ref(),
        RuntimeBackend::Grok => None,
    }
}

/// Remember a vendor catalog for restore / `/model` while that addon is active.
pub fn store_vendor_catalog(
    backend: RuntimeBackend,
    state: crate::acp::model_state::ModelState,
) {
    if let Ok(mut g) = vendor_catalogs().write()
        && let Some(slot) = vendor_slot_mut(&mut g, backend)
    {
        *slot = Some(state);
    }
}

/// Last catalog fetched for `backend`, if any.
pub fn vendor_catalog(backend: RuntimeBackend) -> Option<crate::acp::model_state::ModelState> {
    vendor_catalogs()
        .read()
        .ok()
        .and_then(|g| vendor_slot(&g, backend).cloned())
}

/// Cached Codex catalog (back-compat alias).
pub fn codex_catalog() -> Option<crate::acp::model_state::ModelState> {
    vendor_catalog(RuntimeBackend::Codex)
}

/// Whether a just-finished vendor catalog fetch should replace UI chrome.
/// Late `/runtime` hops must not paint Codex over Claude (or the reverse).
pub fn vendor_refresh_applies(loaded: RuntimeBackend, active: RuntimeBackend) -> bool {
    loaded == active && loaded != RuntimeBackend::Grok
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
    let path = runtime_file_path();
    let exists = path.is_file();
    let file = if exists {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| toml::from_str(&raw).ok())
            .unwrap_or_default()
    } else {
        RuntimeFile::default()
    };
    let active = crate::product_profile::resolve_active_runtime(file.active, exists);
    let mem = RuntimeMem {
        active,
        codex_model: file.codex_model,
        claude_model: file.claude_model,
        lazar_model: file.lazar_model,
        hermes_model: file.hermes_model,
    };
    // Persist product-forced default when no runtime.toml yet, or when lock rewrote active.
    if !exists || mem.active != file.active {
        let _ = save_to_disk(&RuntimeFile {
            active: mem.active,
            codex_model: mem.codex_model.clone(),
            claude_model: mem.claude_model.clone(),
            lazar_model: mem.lazar_model.clone(),
            hermes_model: mem.hermes_model.clone(),
        });
    }
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
        lazar_model: mem.lazar_model.clone(),
        hermes_model: mem.hermes_model.clone(),
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

/// Selected Lazar model id (if any).
pub fn lazar_model() -> Option<String> {
    load_mem().lazar_model
}

/// Selected Hermes model id (if any).
pub fn hermes_model() -> Option<String> {
    load_mem().hermes_model.and_then(|m| {
        let n = agent_tui_hermes_runtime::normalize_model_id(&m);
        if n.is_empty() { None } else { Some(n) }
    })
}

/// Persist and set the active runtime.
pub fn set_active(backend: RuntimeBackend) -> Result<(), String> {
    if !crate::product_profile::runtime_allowed(backend) {
        let p = crate::product_profile::get();
        if p.lock_runtime {
            return Err(format!(
                "This product ({}) is locked to runtime `{}`",
                p.name,
                p.default_runtime.as_str()
            ));
        }
        return Err(format!(
            "Runtime `{}` is not enabled for product `{}`",
            backend.as_str(),
            p.name
        ));
    }
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
    sync_catalog_current(RuntimeBackend::Codex, &model_id);
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
    sync_catalog_current(RuntimeBackend::Claude, &model_id);
    Ok(())
}

/// Persist Lazar model selection (and reset sticky session).
pub fn set_lazar_model(model_id: impl Into<String>) -> Result<(), String> {
    let model_id = model_id.into();
    let mut mem = load_mem();
    let changed = mem.lazar_model.as_deref() != Some(model_id.as_str());
    mem.lazar_model = Some(model_id.clone());
    store_mem(mem)?;
    if changed {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async {
                lazar_pool().reset_session().await;
            });
        }
    }
    sync_catalog_current(RuntimeBackend::Lazar, &model_id);
    Ok(())
}

/// Persist Hermes model selection (and reset sticky session).
pub fn set_hermes_model(model_id: impl Into<String>) -> Result<(), String> {
    // UI may pass display labels ("Hermes — gpt-5.6-sol"); store the wire id.
    let model_id = agent_tui_hermes_runtime::normalize_model_id(&model_id.into());
    if model_id.is_empty() {
        return Err("empty Hermes model id".into());
    }
    let mut mem = load_mem();
    let changed = mem.hermes_model.as_deref() != Some(model_id.as_str());
    mem.hermes_model = Some(model_id.clone());
    store_mem(mem)?;
    if changed {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async {
                hermes_pool().reset_session().await;
            });
        }
    }
    sync_catalog_current(RuntimeBackend::Hermes, &model_id);
    Ok(())
}

fn sync_catalog_current(backend: RuntimeBackend, model_id: &str) {
    if let Ok(mut g) = vendor_catalogs().write()
        && let Some(slot) = vendor_slot_mut(&mut g, backend)
        && let Some(state) = slot
    {
        let id = acp::ModelId::new(Arc::from(model_id));
        if state.available.contains_key(&id) {
            state.set_current(id, None);
        }
    }
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

/// Probe readiness of each backend for the UI (product-filtered).
pub fn status_list() -> Vec<RuntimeStatus> {
    let current = active();
    crate::product_profile::enabled_runtimes()
        .into_iter()
        .map(|backend| {
            let (ready, detail) = match backend {
                RuntimeBackend::Grok => (true, "built-in agent".into()),
                RuntimeBackend::Codex => codex_status(),
                RuntimeBackend::Claude => claude_status(),
                RuntimeBackend::Lazar => lazar_status(),
                RuntimeBackend::Hermes => hermes_status(),
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

fn lazar_bin_path() -> Option<PathBuf> {
    if let Some(bin) = std::env::var_os("LAZAR_BIN").map(PathBuf::from) {
        if bin.is_file() {
            return Some(bin);
        }
    }
    if which_bin("lazar") {
        return Some(PathBuf::from("lazar"));
    }
    let candidate = agent_tui_lazar_runtime::lazar_home().join("bin/lazar");
    candidate.is_file().then_some(candidate)
}

fn lazar_status() -> (bool, String) {
    // Provider/model config is owned by the kernel (LAZAR_MODEL / lazar-env);
    // Agent TUI only displays what the kernel side reports.
    let Some(bin) = lazar_bin_path() else {
        return (
            false,
            "install lazar (on PATH or $LAZAR_HOME/bin/lazar)".into(),
        );
    };
    let model = lazar_model()
        .or_else(agent_tui_lazar_runtime::discover_active_model)
        .unwrap_or_else(|| "kernel default".into());
    (
        true,
        format!("lazar kernel ({}) · model {model}", bin.display()),
    )
}

fn hermes_status() -> (bool, String) {
    let Some(bin) = agent_tui_hermes_runtime::hermes_bin_path() else {
        return (
            false,
            "install Hermes Agent (`hermes` on PATH or ~/.hermes)".into(),
        );
    };
    let model = hermes_model()
        .or_else(agent_tui_hermes_runtime::discover_active_model)
        .unwrap_or_else(|| "config default".into());
    (true, format!("hermes ({}) · model {model}", bin.display()))
}

fn which_bin(name: &str) -> bool {
    which::which(name).is_ok()
}

/// Global Lazar pool (lazy). Spawn-per-turn; kernel-side continuity via `--session`.
pub fn lazar_pool() -> Arc<agent_tui_lazar_runtime::LazarRuntimePool> {
    static POOL: OnceLock<Arc<agent_tui_lazar_runtime::LazarRuntimePool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let lazar_bin = lazar_bin_path().unwrap_or_else(|| PathBuf::from("lazar"));
        Arc::new(agent_tui_lazar_runtime::LazarRuntimePool::new(
            agent_tui_lazar_runtime::PoolConfig {
                lazar_bin,
                // Run from the kernel's home so it resolves skills/hooks/memory/
                // sessions exactly as its own launcher does.
                cwd: Some(agent_tui_lazar_runtime::lazar_home()),
                default_model: agent_tui_lazar_runtime::discover_active_model(),
                ..Default::default()
            },
        ))
    })
    .clone()
}

/// Global Hermes pool (lazy). Spawn-per-turn; sticky `--resume` session.
pub fn hermes_pool() -> Arc<agent_tui_hermes_runtime::HermesRuntimePool> {
    static POOL: OnceLock<Arc<agent_tui_hermes_runtime::HermesRuntimePool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let hermes_bin =
            agent_tui_hermes_runtime::hermes_bin_path().unwrap_or_else(|| PathBuf::from("hermes"));
        Arc::new(agent_tui_hermes_runtime::HermesRuntimePool::new(
            agent_tui_hermes_runtime::PoolConfig {
                hermes_bin,
                cwd: Some(agent_tui_hermes_runtime::hermes_home()),
                default_model: agent_tui_hermes_runtime::discover_active_model(),
                ..Default::default()
            },
        ))
    })
    .clone()
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
        agent_tui_claude_runtime::ClaudeRuntimePool::new(agent_tui_claude_runtime::PoolConfig {
            cwd,
            default_model: claude_model(),
            ..Default::default()
        })
    })
    .clone()
}

/// Stash the Grok catalog if nothing is stored yet (ACP-while-vendor).
pub fn stash_grok_catalog(state: crate::acp::model_state::ModelState) {
    if let Ok(mut g) = grok_stash_lock().write()
        && g.is_none()
    {
        *g = Some(state);
    }
}

/// Overwrite the Grok stash (call when *leaving* Grok so restore has the latest pick).
pub fn replace_stashed_grok_catalog(state: crate::acp::model_state::ModelState) {
    if let Ok(mut g) = grok_stash_lock().write() {
        *g = Some(state);
    }
}

/// Peek the stashed Grok catalog without consuming it.
pub fn stashed_grok_catalog() -> Option<crate::acp::model_state::ModelState> {
    grok_stash_lock().read().ok().and_then(|g| g.clone())
}

/// Take the stashed Grok catalog (if any).
pub fn take_stashed_grok_catalog() -> Option<crate::acp::model_state::ModelState> {
    grok_stash_lock().write().ok().and_then(|mut g| g.take())
}

/// Catalog to paint immediately on `/runtime` so `/model` is not left showing
/// the previous vendor while the live refresh is in flight.
///
/// Missing cache → empty state (Codex `/model` already treats empty as loading).
pub fn chrome_after_runtime_switch(
    backend: RuntimeBackend,
) -> crate::acp::model_state::ModelState {
    match backend {
        RuntimeBackend::Grok => stashed_grok_catalog().unwrap_or_default(),
        RuntimeBackend::Codex => vendor_catalog(RuntimeBackend::Codex)
            .or_else(|| single_entry_catalog(codex_model(), "Codex"))
            .unwrap_or_default(),
        RuntimeBackend::Claude => vendor_catalog(RuntimeBackend::Claude)
            .unwrap_or_else(|| model_state_from_claude_discovered(claude_model().as_deref())),
        RuntimeBackend::Lazar => vendor_catalog(RuntimeBackend::Lazar)
            .unwrap_or_else(|| model_state_from_lazar_active(lazar_model().as_deref())),
        RuntimeBackend::Hermes => vendor_catalog(RuntimeBackend::Hermes)
            .unwrap_or_else(|| model_state_from_hermes_active(hermes_model().as_deref())),
    }
}

fn single_entry_catalog(
    id: Option<String>,
    vendor: &str,
) -> Option<crate::acp::model_state::ModelState> {
    let id = id.filter(|s| !s.is_empty())?;
    let model_id = acp::ModelId::new(Arc::from(id.as_str()));
    let mut meta = serde_json::Map::new();
    meta_with_context_window(&mut meta, Some(200_000));
    let info = acp::ModelInfo::new(model_id.clone(), format!("{vendor} — {id}")).meta(Some(meta));
    let mut available = IndexMap::new();
    available.insert(model_id.clone(), info);
    let mut state = crate::acp::model_state::ModelState::default();
    state.update_catalog(available);
    state.set_current(model_id, None);
    Some(state)
}

/// Insert `totalContextTokens` so the context bar can update on model switch.
fn meta_with_context_window(
    meta: &mut serde_json::Map<String, serde_json::Value>,
    tokens: Option<u64>,
) {
    if let Some(tokens) = tokens.filter(|&t| t > 0) {
        meta.insert(
            "totalContextTokens".into(),
            serde_json::Value::Number(tokens.into()),
        );
    }
}

/// Best-effort context window for Claude model ids (catalog has no wire field).
/// `[1m]` rows → 1_000_000; otherwise Anthropic default 200k.
pub(crate) fn claude_context_window_tokens(model_id: &str) -> u64 {
    let lower = model_id.to_ascii_lowercase();
    if lower.contains("[1m]") {
        1_000_000
    } else {
        200_000
    }
}

/// Best-effort context window for Codex model ids when app-server omits it.
pub(crate) fn codex_context_window_tokens(model_id: &str) -> u64 {
    let lower = model_id.to_ascii_lowercase();
    if lower.contains("gpt-5") || lower.contains("o3") || lower.contains("codex") {
        // Current OpenAI/Codex coding models are typically 200k–400k; 200k is
        // the safe published floor used for the bar until model/list ships a
        // real field.
        200_000
    } else if lower.contains("gpt-4.1") || lower.contains("gpt-4o") {
        128_000
    } else {
        200_000
    }
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
            meta.insert(
                "supportsReasoningEffort".into(),
                serde_json::Value::Bool(true),
            );
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
            meta.insert("reasoningEfforts".into(), serde_json::Value::Array(efforts));
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
        let window = e
            .context_window
            .filter(|&t| t > 0)
            .unwrap_or_else(|| codex_context_window_tokens(&e.id));
        meta_with_context_window(&mut meta, Some(window));
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
    state.update_catalog(available);
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
    store_vendor_catalog(RuntimeBackend::Codex, state.clone());
    Ok(state)
}

/// Load Claude model catalog (Claude Code cache + built-ins + selected).
pub fn refresh_claude_models() -> Result<crate::acp::model_state::ModelState, String> {
    claude_pool().ensure_binary().map_err(|e| e.to_string())?;
    let preferred = claude_model();
    let state = model_state_from_claude_discovered(preferred.as_deref());
    if claude_model().is_none() {
        if let Some(id) = state.current_model_id_str() {
            let _ = set_claude_model(id);
        }
    }
    store_vendor_catalog(RuntimeBackend::Claude, state.clone());
    Ok(state)
}

/// Load the Lazar "catalog": the kernel owns providers and reports one active
/// model (LAZAR_MODEL / memory/model.txt); there is no catalog API.
pub fn refresh_lazar_models() -> Result<crate::acp::model_state::ModelState, String> {
    let state = model_state_from_lazar_active(lazar_model().as_deref());
    store_vendor_catalog(RuntimeBackend::Lazar, state.clone());
    Ok(state)
}

/// Build ModelState from the kernel-reported active model (single entry).
pub fn model_state_from_lazar_active(
    preferred: Option<&str>,
) -> crate::acp::model_state::ModelState {
    let id = preferred
        .map(str::to_string)
        .or_else(agent_tui_lazar_runtime::discover_active_model)
        .unwrap_or_else(|| "lazar-default".to_string());
    let model_id = acp::ModelId::new(Arc::from(id.as_str()));
    // Kernel may target any provider; use a conservative default so the bar
    // does not keep a previous Grok 500k sticky total after `/runtime lazar`.
    let mut meta = serde_json::Map::new();
    meta_with_context_window(&mut meta, Some(200_000));
    let info = acp::ModelInfo::new(model_id.clone(), format!("Lazar — {id}")).meta(Some(meta));
    let mut available = IndexMap::new();
    available.insert(model_id.clone(), info);
    let mut state = crate::acp::model_state::ModelState::default();
    state.update_catalog(available);
    state.set_current(model_id, None);
    state
}

/// Load the Hermes "catalog": config.yaml default + optional selected model.
pub fn refresh_hermes_models() -> Result<crate::acp::model_state::ModelState, String> {
    let state = model_state_from_hermes_active(hermes_model().as_deref());
    store_vendor_catalog(RuntimeBackend::Hermes, state.clone());
    Ok(state)
}

pub fn model_state_from_hermes_active(
    preferred: Option<&str>,
) -> crate::acp::model_state::ModelState {
    let id = preferred
        .map(|p| agent_tui_hermes_runtime::normalize_model_id(p))
        .filter(|s| !s.is_empty())
        .or_else(agent_tui_hermes_runtime::discover_active_model)
        .unwrap_or_else(|| "hermes-default".to_string());
    // Wire id must be the real Hermes/provider model slug — never the display label.
    let model_id = acp::ModelId::new(Arc::from(id.as_str()));
    let mut meta = serde_json::Map::new();
    meta_with_context_window(&mut meta, Some(200_000));
    let info = acp::ModelInfo::new(model_id.clone(), format!("Hermes — {id}")).meta(Some(meta));
    let mut available = IndexMap::new();
    available.insert(model_id.clone(), info);
    let mut state = crate::acp::model_state::ModelState::default();
    state.update_catalog(available);
    state.set_current(model_id, None);
    state
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
        let mut meta = serde_json::Map::new();
        meta_with_context_window(&mut meta, Some(claude_context_window_tokens(&e.id)));
        let mut info = acp::ModelInfo::new(id.clone(), e.display_name).meta(Some(meta));
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
                let mut meta = serde_json::Map::new();
                meta_with_context_window(&mut meta, Some(claude_context_window_tokens(p)));
                available.insert(
                    id.clone(),
                    acp::ModelInfo::new(id.clone(), p.to_string()).meta(Some(meta)),
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
    state.update_catalog(available);
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

#[cfg(test)]
pub(crate) fn catalog_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
pub(crate) fn reset_runtime_catalogs_for_test() {
    if let Ok(mut g) = vendor_catalogs().write() {
        *g = VendorCatalogs::default();
    }
    if let Ok(mut g) = grok_stash_lock().write() {
        *g = None;
    }
}

#[cfg(test)]
mod tests;
