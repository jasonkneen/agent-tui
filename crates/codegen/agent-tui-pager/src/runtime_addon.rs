//! Runtime **addons** — the brains that plug into ONE CORE.
//!
//! Architecture law: [docs/CORE_AND_ADDONS.md](../../../../../docs/CORE_AND_ADDONS.md).
//!
//! - **Core** = pager, sessions, slash, tools UI (this crate + shell).
//! - **Addon** = something that can serve turns + expose a model catalog.
//! - **Product profile** = which addons are enabled + brand + default.
//!
//! Today addons are compiled in (`RuntimeBackend`). This module is the
//! registry surface: metadata, product-filtered list, and the names used by
//! `/runtime`. Turn dispatch stays in `runtime_backend` until a full `dyn`
//! registry lands.

use crate::product_profile;
use crate::runtime_backend::RuntimeBackend;

/// Static metadata for one compiled-in runtime addon.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeAddon {
    pub id: RuntimeBackend,
    /// Slash / config id (`lazar`, `codex`, …).
    pub slug: &'static str,
    /// Short UI label.
    pub label: &'static str,
    /// One-line how turns work.
    pub turn_shape: &'static str,
}

/// Full catalog of addons linked into this build (before product filtering).
pub fn catalog() -> &'static [RuntimeAddon] {
    &CATALOG
}

const CATALOG: [RuntimeAddon; 5] = [
    RuntimeAddon {
        id: RuntimeBackend::Grok,
        slug: "grok",
        label: "Grok (xAI)",
        turn_shape: "built-in sampler HTTP SSE",
    },
    RuntimeAddon {
        id: RuntimeBackend::Codex,
        slug: "codex",
        label: "Codex (app-server)",
        turn_shape: "warm codex app-server JSON-RPC",
    },
    RuntimeAddon {
        id: RuntimeBackend::Claude,
        slug: "claude",
        label: "Claude (CLI harness)",
        turn_shape: "claude -p + sticky resume",
    },
    RuntimeAddon {
        id: RuntimeBackend::Lazar,
        slug: "lazar",
        label: "Lazar (kernel)",
        turn_shape: "spawn-per-turn lazar -p stream-json",
    },
    RuntimeAddon {
        id: RuntimeBackend::Hermes,
        slug: "hermes",
        label: "Hermes (agent)",
        turn_shape: "hermes chat -q -Q + sticky --resume",
    },
];

/// Addons visible under the current product profile.
pub fn enabled() -> Vec<RuntimeAddon> {
    let allowed = product_profile::enabled_runtimes();
    catalog()
        .iter()
        .copied()
        .filter(|a| allowed.contains(&a.id))
        .collect()
}

/// Look up addon meta by backend id.
pub fn get(id: RuntimeBackend) -> Option<RuntimeAddon> {
    catalog().iter().copied().find(|a| a.id == id)
}

/// Active addon (product-resolved runtime).
pub fn active() -> RuntimeAddon {
    let id = crate::runtime_backend::active();
    get(id).unwrap_or(CATALOG[0])
}

/// Human one-liner for status / docs.
pub fn describe(id: RuntimeBackend) -> String {
    match get(id) {
        Some(a) => format!("{} — {}", a.label, a.turn_shape),
        None => id.display_name().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_exactly_matches_compiled_runtime_registry() {
        let ids: Vec<_> = catalog().iter().map(|addon| addon.id).collect();
        assert_eq!(ids, RuntimeBackend::all());
        for addon in catalog() {
            assert_eq!(addon.slug, addon.id.as_str());
            assert_eq!(get(addon.id).map(|found| found.slug), Some(addon.slug));
            assert!(!addon.label.trim().is_empty());
            assert!(!addon.turn_shape.trim().is_empty());
        }
    }
}
