//! Shared hook source path discovery.

use std::path::{Path, PathBuf};

use agent_tui_hooks::discovery::HookSource;

/// Owned paths for hook sources. Callers borrow via `as_sources()`.
pub(crate) struct HookSourcePaths {
    pub global: Vec<PathBuf>,
    pub project: Vec<PathBuf>,
}

impl HookSourcePaths {
    /// Borrow as `HookSource` refs. Project sources are excluded when untrusted.
    pub(crate) fn as_sources(
        &self,
        include_project: bool,
    ) -> (Vec<HookSource<'_>>, Vec<HookSource<'_>>) {
        let global = self.global.iter().map(|p| path_to_source(p)).collect();
        let project = if include_project {
            self.project.iter().map(|p| path_to_source(p)).collect()
        } else {
            vec![]
        };
        (global, project)
    }
}

fn path_to_source(p: &Path) -> HookSource<'_> {
    if p.is_dir() {
        HookSource::Directory(p)
    } else {
        HookSource::SettingsFile(p)
    }
}

fn include_claude_hooks(compat: &agent_tui_tools::types::compat::CompatConfig) -> bool {
    compat.claude.hooks
        && !crate::claude_import::is_claude_import_marked_with_log("discover_hook_source_paths")
}

fn include_cursor_hooks(compat: &agent_tui_tools::types::compat::CompatConfig) -> bool {
    compat.cursor.hooks
}

/// Global + project hook source paths. Registry file is never a discovery
/// source; compatible vendor globals are appended when their gates are on.
pub(crate) fn discover_hook_source_paths(
    git_root: Option<&Path>,
    compat: &agent_tui_tools::types::compat::CompatConfig,
) -> HookSourcePaths {
    // Compat gate: skip .claude hook sources when disabled.
    let skip_claude_compat = !compat.claude.hooks;
    // Phase 2 cutoff: if the user has imported, skip .claude/settings.json
    // sources. Native .grok/hooks/ directories are still scanned (they hold
    // any hooks that were imported by /import-claude).
    let skip_claude = skip_claude_compat
        || crate::claude_import::is_claude_import_marked_with_log("discover_hook_source_paths");

    // Compat gate: skip Cursor hook sources when disabled.
    let skip_cursor = !compat.cursor.hooks;

    let home = dirs::home_dir();
    // user_grok_home() is None when no home resolves, so inspect lists the same
    // sources a live session loads, instead of a cwd-relative .grok.
    let grok = agent_tui_config::user_grok_home();
    let mut global = Vec::new();

    if !skip_claude && let Some(ref h) = home {
        global.push(h.join(".claude").join("settings.json"));
        global.push(h.join(".claude").join("settings.local.json"));
    }
    if let Some(ref grok) = grok {
        global.push(grok.join("hooks"));
    }

    let custom_paths: Vec<PathBuf> = grok
        .as_ref()
        .and_then(|g| std::fs::read_to_string(g.join("hooks-paths")).ok())
        .map(|content| {
            content
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| PathBuf::from(l.trim()))
                .collect()
        })
        .unwrap_or_default();
    global.extend(custom_paths);

    if let Some(ref h) = home
        && !skip_cursor
    {
        global.push(h.join(".cursor").join("hooks.json"));
    }

    let mut project = Vec::new();

    if let Some(root) = git_root {
        if !skip_claude {
            project.push(root.join(".claude").join("settings.json"));
            project.push(root.join(".claude").join("settings.local.json"));
        }
        project.push(root.join(".grok").join("hooks"));
        if !skip_cursor {
            project.push(root.join(".cursor").join("hooks.json"));
        }
    }

    HookSourcePaths { global, project }
}

/// Single load entry point: build compat-aware sources, gate project sources on
/// trust, then load. Every session-startup and mid-session reload site routes
/// through here so the source policy stays in one place.
pub(crate) fn discover_hooks(
    git_root: Option<&Path>,
    compat: &agent_tui_tools::types::compat::CompatConfig,
    trusted: bool,
) -> (agent_tui_hooks::discovery::HookRegistry, Vec<HookError>) {
    // Read fresh each call (not cached): a mid-session `/hooks` reload must see an
    // updated `config.toml` / `managed_config.toml`. This is lighter than
    // `ConfigLayers::load` (only the small per-layer files, no campaigns, version
    // overrides, or MDM).
    let config_layers = agent_tui_config::hook_config_layers();
    assemble_hooks(&config_layers, git_root, compat, trusted)
}

/// Pure, injectable core: combine config-layer hooks with file-source hooks and
/// dedup once. Config-layer specs are placed first so that, under the first-wins
/// dedup in [`agent_tui_hooks::discovery::registry_from_specs_deduped`], a config
/// hook wins over a byte-identical file hook. `config_layers` is a parameter (not
/// read here) so tests can drive it with hand-built layers.
pub(crate) fn assemble_hooks(
    config_layers: &[agent_tui_config::HookConfigLayer],
    git_root: Option<&Path>,
    compat: &agent_tui_tools::types::compat::CompatConfig,
    trusted: bool,
) -> (agent_tui_hooks::discovery::HookRegistry, Vec<HookError>) {
    let (mut specs, mut errors) =
        agent_tui_hooks::config::parse_hooks_from_config_layers(config_layers);

    let source_paths = discover_hook_source_paths(git_root, compat);
    let (global_sources, project_sources) = source_paths.as_sources(trusted);
    agent_tui_hooks::discovery::load_hooks_from_sources(&global_sources, &project_sources)
}
