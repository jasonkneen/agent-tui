//! Product profile — brand + default **addon** + optional lock.
//!
//! **ONE CORE + ADDONS** (`docs/CORE_AND_ADDONS.md`):
//! - Core = this TUI (always)
//! - Addons = grok / codex / claude / lazar (runtime brains)
//! - Product = skin + which addons are allowed + which is default
//!
//! Config (first wins):
//!
//! 1. `AGENT_TUI_PRODUCT=lazar|agent-tui` (named preset)
//! 2. `AGENT_TUI_PRODUCT_FILE=/path/to/product.toml`
//! 3. `$AGENT_TUI_HOME/product.toml` (default `~/.agent-tui/product.toml`)
//! 4. Built-in Agent TUI defaults (all addons, default grok)
//!
//! ```toml
//! # ~/.agent-tui/product.toml — Lazar single-product
//! id = "lazar"
//! name = "Lazar"
//! title_token = "lazar"
//! default_runtime = "lazar"
//! lock_runtime = true
//! addons = ["lazar"]              # preferred key
//! # enabled_runtimes = ["lazar"]  # legacy alias of addons
//! ```

use crate::runtime_backend::RuntimeBackend;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Brand + runtime policy for this process.
#[derive(Debug, Clone)]
pub struct ProductProfile {
    /// Short id (`agent-tui`, `lazar`).
    pub id: String,
    /// Human product name shown in UI.
    pub name: String,
    /// Token used in window titles (`agent-tui`, `lazar`).
    pub title_token: String,
    /// Runtime selected when no `runtime.toml` exists, or when locked.
    pub default_runtime: RuntimeBackend,
    /// When true, `/runtime` cannot switch away from `default_runtime`.
    pub lock_runtime: bool,
    /// If `Some`, only these runtimes appear in `/runtime` and may be selected.
    pub enabled_runtimes: Option<Vec<RuntimeBackend>>,
}

impl Default for ProductProfile {
    fn default() -> Self {
        agent_tui_defaults()
    }
}

/// All compiled-in runtime addons (multi-provider platform).
fn all_addons() -> Vec<RuntimeBackend> {
    RuntimeBackend::all().to_vec()
}

/// Multi-addon platform: every provider available, switch with `/runtime`.
fn agent_tui_defaults() -> ProductProfile {
    ProductProfile {
        id: "agent-tui".into(),
        name: agent_tui_version::PRODUCT_DISPLAY_NAME.into(),
        title_token: agent_tui_version::PRODUCT_TITLE_TOKEN.into(),
        default_runtime: RuntimeBackend::Grok,
        lock_runtime: false,
        // Explicit full allow-list so new addons show up when `RuntimeBackend::all` grows.
        enabled_runtimes: Some(all_addons()),
    }
}

/// Alias for the kitchen-sink product (same as platform).
fn all_providers_defaults() -> ProductProfile {
    let mut p = agent_tui_defaults();
    p.id = "all".into();
    p.name = "Agent TUI (all providers)".into();
    p.title_token = "agent-tui".into();
    p
}

/// Single-addon product skin (brand + lock).
fn single_addon_product(
    id: &str,
    name: &str,
    title_token: &str,
    runtime: RuntimeBackend,
) -> ProductProfile {
    ProductProfile {
        id: id.into(),
        name: name.into(),
        title_token: title_token.into(),
        default_runtime: runtime,
        lock_runtime: true,
        enabled_runtimes: Some(vec![runtime]),
    }
}

fn grok_defaults() -> ProductProfile {
    single_addon_product("grok", "Grok", "grok", RuntimeBackend::Grok)
}

fn lazar_defaults() -> ProductProfile {
    single_addon_product("lazar", "Lazar", "lazar", RuntimeBackend::Lazar)
}

fn codex_defaults() -> ProductProfile {
    single_addon_product("codex", "Codex", "codex", RuntimeBackend::Codex)
}

fn claude_defaults() -> ProductProfile {
    single_addon_product("claude", "Claude", "claude", RuntimeBackend::Claude)
}

fn hermes_defaults() -> ProductProfile {
    single_addon_product("hermes", "Hermes", "hermes", RuntimeBackend::Hermes)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ProductFile {
    id: Option<String>,
    name: Option<String>,
    title_token: Option<String>,
    default_runtime: Option<String>,
    lock_runtime: Option<bool>,
    /// Preferred: allow-listed addon slugs (`lazar`, `codex`, …).
    addons: Option<Vec<String>>,
    /// Legacy alias of `addons`.
    enabled_runtimes: Option<Vec<String>>,
}

static PROFILE: OnceLock<ProductProfile> = OnceLock::new();

/// Resolved product profile for this process (loaded once).
pub fn get() -> &'static ProductProfile {
    PROFILE.get_or_init(load_profile)
}

/// Human display name (welcome badge, toasts).
pub fn display_name() -> &'static str {
    get().name.as_str()
}

/// Window-title product token.
pub fn title_token() -> &'static str {
    get().title_token.as_str()
}

/// Whether `/runtime` switches are blocked.
pub fn lock_runtime() -> bool {
    get().lock_runtime
}

/// Default runtime for this product.
pub fn default_runtime() -> RuntimeBackend {
    get().default_runtime
}

/// Runtimes exposed in the UI (filtered by product).
pub fn enabled_runtimes() -> Vec<RuntimeBackend> {
    let p = get();
    match &p.enabled_runtimes {
        Some(list) if !list.is_empty() => list.clone(),
        _ => RuntimeBackend::all().to_vec(),
    }
}

/// True if this backend may be selected under the current product.
pub fn runtime_allowed(backend: RuntimeBackend) -> bool {
    let p = get();
    if p.lock_runtime {
        return backend == p.default_runtime;
    }
    match &p.enabled_runtimes {
        Some(list) if !list.is_empty() => list.contains(&backend),
        _ => true,
    }
}

fn load_profile() -> ProductProfile {
    // 1) Named preset or path via env
    if let Ok(raw) = std::env::var("AGENT_TUI_PRODUCT") {
        let v = raw.trim();
        if !v.is_empty() {
            if let Some(p) = preset(v) {
                return p;
            }
            // Treat as path to a product.toml
            if let Some(p) = load_from_path(Path::new(v)) {
                return p;
            }
        }
    }
    if let Ok(path) = std::env::var("AGENT_TUI_PRODUCT_FILE") {
        let path = path.trim();
        if !path.is_empty() {
            if let Some(p) = load_from_path(Path::new(path)) {
                return p;
            }
        }
    }
    // 2) Config home product.toml
    let home_file = product_file_path();
    if let Some(p) = load_from_path(&home_file) {
        return p;
    }
    agent_tui_defaults()
}

/// Path of the product.toml under config home.
pub fn product_file_path() -> PathBuf {
    agent_tui_config::grok_home().join("product.toml")
}

fn preset(name: &str) -> Option<ProductProfile> {
    match name.trim().to_ascii_lowercase().as_str() {
        // Multi-addon platform — every runtime (grok, codex, claude, lazar, hermes, …).
        "agent-tui" | "agent_tui" | "default" | "platform" => Some(agent_tui_defaults()),
        "all" | "multi" | "everything" | "kitchen-sink" | "kitchen_sink" => {
            Some(all_providers_defaults())
        }
        // Single-product skins (locked to one addon).
        "grok" | "xai" => Some(grok_defaults()),
        "lazar" => Some(lazar_defaults()),
        "codex" | "chatgpt" | "openai" => Some(codex_defaults()),
        "claude" | "anthropic" => Some(claude_defaults()),
        "hermes" => Some(hermes_defaults()),
        _ => None,
    }
}

fn load_from_path(path: &Path) -> Option<ProductProfile> {
    let raw = std::fs::read_to_string(path).ok()?;
    let file: ProductFile = toml::from_str(&raw).ok()?;
    Some(merge_file(file))
}

fn merge_file(file: ProductFile) -> ProductProfile {
    // Start from preset if id matches a known product; else agent-tui base.
    let mut base = file
        .id
        .as_deref()
        .and_then(preset)
        .unwrap_or_else(agent_tui_defaults);

    if let Some(id) = file.id {
        base.id = id;
    }
    if let Some(name) = file.name {
        base.name = name;
    }
    if let Some(token) = file.title_token {
        base.title_token = token;
    }
    if let Some(rt) = file.default_runtime.as_deref().and_then(RuntimeBackend::parse) {
        base.default_runtime = rt;
    }
    if let Some(lock) = file.lock_runtime {
        base.lock_runtime = lock;
    }
    // Prefer `addons`; fall back to legacy `enabled_runtimes`.
    let list = file.addons.or(file.enabled_runtimes);
    if let Some(list) = list {
        let parsed: Vec<RuntimeBackend> = list
            .iter()
            .filter_map(|s| RuntimeBackend::parse(s))
            .collect();
        base.enabled_runtimes = if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        };
    }
    // lock_runtime implies enabled list is at least the default.
    if base.lock_runtime {
        base.enabled_runtimes = Some(vec![base.default_runtime]);
    }
    base
}

/// Resolve which runtime should be active given disk state + product policy.
pub fn resolve_active_runtime(
    disk_active: RuntimeBackend,
    runtime_file_exists: bool,
) -> RuntimeBackend {
    let p = get();
    if p.lock_runtime {
        return p.default_runtime;
    }
    if !runtime_file_exists {
        return p.default_runtime;
    }
    if !runtime_allowed(disk_active) {
        return p.default_runtime;
    }
    disk_active
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_lazar_locks_to_lazar() {
        let p = lazar_defaults();
        assert_eq!(p.default_runtime, RuntimeBackend::Lazar);
        assert!(p.lock_runtime);
        assert_eq!(p.enabled_runtimes.as_deref(), Some(&[RuntimeBackend::Lazar][..]));
        assert_eq!(p.name, "Lazar");
        assert_eq!(p.title_token, "lazar");
    }

    #[test]
    fn preset_grok_locks_to_grok() {
        let p = grok_defaults();
        assert_eq!(p.default_runtime, RuntimeBackend::Grok);
        assert!(p.lock_runtime);
        assert_eq!(p.enabled_runtimes.as_deref(), Some(&[RuntimeBackend::Grok][..]));
        assert_eq!(p.name, "Grok");
        assert_eq!(p.title_token, "grok");
    }

    #[test]
    fn preset_names_resolve() {
        assert_eq!(preset("grok").unwrap().id, "grok");
        assert_eq!(preset("xai").unwrap().id, "grok");
        assert_eq!(preset("lazar").unwrap().id, "lazar");
        assert_eq!(preset("codex").unwrap().id, "codex");
        assert_eq!(preset("chatgpt").unwrap().id, "codex");
        assert_eq!(preset("claude").unwrap().id, "claude");
        assert_eq!(preset("anthropic").unwrap().id, "claude");
        assert_eq!(preset("hermes").unwrap().id, "hermes");
        assert_eq!(preset("platform").unwrap().id, "agent-tui");
        assert!(!preset("platform").unwrap().lock_runtime);
        assert!(preset("codex").unwrap().lock_runtime);
        assert!(preset("claude").unwrap().lock_runtime);
        let all = preset("all").unwrap();
        assert_eq!(all.id, "all");
        assert!(!all.lock_runtime);
        assert!(all.enabled_runtimes.as_ref().unwrap().len() >= 5);
        assert!(all.enabled_runtimes.as_ref().unwrap().contains(&RuntimeBackend::Hermes));
        assert_eq!(preset("multi").unwrap().id, "all");
    }

    #[test]
    #[test]
    fn merge_file_overrides_name_and_default() {
        let file = ProductFile {
            id: Some("lazar".into()),
            name: Some("Lazar NERV".into()),
            title_token: None,
            default_runtime: Some("lazar".into()),
            lock_runtime: Some(false),
            addons: Some(vec!["lazar".into(), "codex".into()]),
            enabled_runtimes: None,
        };
        let p = merge_file(file);
        assert_eq!(p.name, "Lazar NERV");
        assert_eq!(p.default_runtime, RuntimeBackend::Lazar);
        assert!(!p.lock_runtime);
        assert!(p.enabled_runtimes.as_ref().unwrap().contains(&RuntimeBackend::Codex));
    }

    #[test]
    fn lock_forces_enabled_to_default_only() {
        let file = ProductFile {
            id: Some("lazar".into()),
            name: None,
            title_token: None,
            default_runtime: Some("lazar".into()),
            lock_runtime: Some(true),
            addons: Some(vec!["lazar".into(), "grok".into()]),
            enabled_runtimes: None,
        };
        let p = merge_file(file);
        assert_eq!(p.enabled_runtimes.as_deref(), Some(&[RuntimeBackend::Lazar][..]));
    }

    #[test]
    fn addons_key_preferred_over_legacy_enabled_runtimes() {
        let file = ProductFile {
            id: None,
            name: None,
            title_token: None,
            default_runtime: Some("lazar".into()),
            lock_runtime: Some(false),
            addons: Some(vec!["lazar".into()]),
            enabled_runtimes: Some(vec!["grok".into(), "codex".into()]),
        };
        let p = merge_file(file);
        assert_eq!(p.enabled_runtimes.as_deref(), Some(&[RuntimeBackend::Lazar][..]));
    }
}
