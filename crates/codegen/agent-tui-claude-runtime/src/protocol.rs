//! Wire shapes for `claude -p --output-format json` and model catalog.

use serde::Deserialize;
use std::path::PathBuf;

/// Built-in Claude Code model catalog (kept current with Claude Code 2.x).
///
/// Prefer [`discover_models`] at runtime — it merges Claude Code's local
/// `additionalModelOptionsCache` so newly launched models (Fable, 4.8, …)
/// appear without a rebuild.
pub const KNOWN_MODELS: &[ClaudeModelEntry] = &[
    // --- Aliases (Claude Code resolves to current default of that family) ---
    ClaudeModelEntry {
        id: "sonnet",
        display_name: "Sonnet (alias)",
        description: Some("Latest Sonnet alias"),
        is_default: false,
    },
    ClaudeModelEntry {
        id: "opus",
        display_name: "Opus (alias)",
        description: Some("Latest Opus alias"),
        is_default: false,
    },
    ClaudeModelEntry {
        id: "haiku",
        display_name: "Haiku (alias)",
        description: Some("Fast Haiku alias"),
        is_default: false,
    },
    ClaudeModelEntry {
        id: "fable",
        display_name: "Fable (alias)",
        description: Some("Latest Fable alias"),
        is_default: false,
    },
    // --- Claude 5 / Mythos-class ---
    ClaudeModelEntry {
        id: "claude-fable-5[1m]",
        display_name: "Fable 5 (1M)",
        description: Some("Most capable for hard / long-running tasks"),
        is_default: true,
    },
    ClaudeModelEntry {
        id: "claude-fable-5",
        display_name: "Fable 5",
        description: Some("Claude 5 Fable (Mythos-class)"),
        is_default: false,
    },
    // --- Opus 4.x ---
    ClaudeModelEntry {
        id: "claude-opus-4-8",
        display_name: "Opus 4.8",
        description: Some("Latest Opus 4.8"),
        is_default: false,
    },
    ClaudeModelEntry {
        id: "claude-opus-4-8[1m]",
        display_name: "Opus 4.8 (1M)",
        description: Some("Opus 4.8 with 1M context"),
        is_default: false,
    },
    ClaudeModelEntry {
        id: "claude-opus-4-7",
        display_name: "Opus 4.7",
        description: None,
        is_default: false,
    },
    ClaudeModelEntry {
        id: "claude-opus-4-7[1m]",
        display_name: "Opus 4.7 (1M)",
        description: None,
        is_default: false,
    },
    ClaudeModelEntry {
        id: "claude-opus-4-7-fast",
        display_name: "Opus 4.7 Fast",
        description: None,
        is_default: false,
    },
    ClaudeModelEntry {
        id: "claude-opus-4-6",
        display_name: "Opus 4.6",
        description: None,
        is_default: false,
    },
    ClaudeModelEntry {
        id: "claude-opus-4-6[1m]",
        display_name: "Opus 4.6 (1M)",
        description: None,
        is_default: false,
    },
    // --- Sonnet 5 / 4.x ---
    ClaudeModelEntry {
        id: "claude-sonnet-5",
        display_name: "Sonnet 5",
        description: Some("Latest Sonnet 5"),
        is_default: false,
    },
    ClaudeModelEntry {
        id: "claude-sonnet-5[1m]",
        display_name: "Sonnet 5 (1M)",
        description: None,
        is_default: false,
    },
    ClaudeModelEntry {
        id: "claude-sonnet-4-6",
        display_name: "Sonnet 4.6",
        description: None,
        is_default: false,
    },
    ClaudeModelEntry {
        id: "claude-sonnet-4-6[1m]",
        display_name: "Sonnet 4.6 (1M)",
        description: None,
        is_default: false,
    },
    ClaudeModelEntry {
        id: "claude-sonnet-4-5-20250929",
        display_name: "Sonnet 4.5",
        description: None,
        is_default: false,
    },
    // --- Haiku ---
    ClaudeModelEntry {
        id: "claude-haiku-4-5-20251001",
        display_name: "Haiku 4.5",
        description: Some("Fast / cheap"),
        is_default: false,
    },
];

#[derive(Debug, Clone)]
pub struct ClaudeModelEntry {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: Option<&'static str>,
    pub is_default: bool,
}

/// Owned model entry (from disk discovery or static catalog).
#[derive(Debug, Clone)]
pub struct DiscoveredModel {
    pub id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub is_default: bool,
}

/// Merge built-in catalog + Claude Code's `additionalModelOptionsCache`.
///
/// Order: discovered (Claude Code cache) first so newest models surface at the
/// top of `/model`, then remaining built-ins not already present.
pub fn discover_models() -> Vec<DiscoveredModel> {
    let mut out: Vec<DiscoveredModel> = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();

    for m in load_from_claude_json_cache() {
        if seen.insert(m.id.clone()) {
            out.push(m);
        }
    }

    for e in KNOWN_MODELS {
        if seen.insert(e.id.to_string()) {
            out.push(DiscoveredModel {
                id: e.id.to_string(),
                display_name: e.display_name.to_string(),
                description: e.description.map(str::to_string),
                is_default: e.is_default,
            });
        }
    }

    // If nothing marked default, prefer Fable 5 / Opus 4.8 / first entry.
    if !out.iter().any(|m| m.is_default) {
        let prefer = ["claude-fable-5[1m]", "claude-fable-5", "claude-opus-4-8", "sonnet"];
        if let Some(m) = out.iter_mut().find(|m| prefer.contains(&m.id.as_str())) {
            m.is_default = true;
        } else if let Some(m) = out.first_mut() {
            m.is_default = true;
        }
    }

    out
}

fn load_from_claude_json_cache() -> Vec<DiscoveredModel> {
    let mut out = Vec::new();
    for path in claude_json_paths() {
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        // additionalModelOptionsCache: [{value, label, description}, ...]
        if let Some(arr) = val.get("additionalModelOptionsCache").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(m) = parse_option_row(item) {
                    out.push(m);
                }
            }
        }
        // openaiAdditionalModelOptionsCache (extra discovered endpoints) — skip;
        // those are not Claude Code first-party models.
        // Also scan nested clientDataCache / growthbook-ish dumps for model values.
        collect_modelish_strings(&val, &mut out);
    }
    out
}

fn parse_option_row(item: &serde_json::Value) -> Option<DiscoveredModel> {
    let id = item
        .get("value")
        .or_else(|| item.get("id"))
        .or_else(|| item.get("model"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    // Keep Claude / Anthropic-shaped ids (and short aliases).
    if !is_claude_model_id(&id) {
        return None;
    }
    let display_name = item
        .get("label")
        .or_else(|| item.get("displayName"))
        .or_else(|| item.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or(&id)
        .to_string();
    let description = item
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Some(DiscoveredModel {
        id,
        display_name,
        description,
        is_default: item
            .get("isDefault")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

fn collect_modelish_strings(val: &serde_json::Value, out: &mut Vec<DiscoveredModel>) {
    // Pull known full ids from free-form JSON so caches with only strings still help.
    let blob = val.to_string();
    for id in [
        "claude-fable-5[1m]",
        "claude-fable-5",
        "claude-opus-4-8[1m]",
        "claude-opus-4-8",
        "claude-opus-4-7[1m]",
        "claude-opus-4-7",
        "claude-sonnet-5[1m]",
        "claude-sonnet-5",
        "claude-sonnet-4-6[1m]",
        "claude-sonnet-4-6",
    ] {
        if blob.contains(id)
            && !out.iter().any(|m| m.id == id)
        {
            out.push(DiscoveredModel {
                id: id.to_string(),
                display_name: pretty_name(id),
                description: None,
                is_default: false,
            });
        }
    }
}

fn is_claude_model_id(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    lower.starts_with("claude-")
        || matches!(
            lower.as_str(),
            "sonnet" | "opus" | "haiku" | "fable" | "mythos"
        )
}

fn pretty_name(id: &str) -> String {
    match id {
        "claude-fable-5[1m]" => "Fable 5 (1M)".into(),
        "claude-fable-5" => "Fable 5".into(),
        "claude-opus-4-8" => "Opus 4.8".into(),
        "claude-opus-4-8[1m]" => "Opus 4.8 (1M)".into(),
        "claude-opus-4-7" => "Opus 4.7".into(),
        "claude-opus-4-7[1m]" => "Opus 4.7 (1M)".into(),
        "claude-sonnet-5" => "Sonnet 5".into(),
        "claude-sonnet-5[1m]" => "Sonnet 5 (1M)".into(),
        "claude-sonnet-4-6" => "Sonnet 4.6".into(),
        "claude-sonnet-4-6[1m]" => "Sonnet 4.6 (1M)".into(),
        other => other.to_string(),
    }
}

fn claude_json_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        paths.push(home.join(".claude.json"));
        paths.push(home.join(".claude/.claude.json"));
    }
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let dir = PathBuf::from(dir);
        paths.push(dir.join(".claude.json"));
        paths.push(dir.join("claude.json"));
    }
    paths
}

/// Parsed result of a successful `claude -p --output-format json` turn.
#[derive(Debug, Clone)]
pub struct ClaudeTurnResult {
    pub text: String,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub is_error: bool,
}

/// Subset of the JSON result object from Claude Code print mode.
#[derive(Debug, Deserialize)]
pub(crate) struct PrintResultJson {
    #[serde(default)]
    #[allow(dead_code)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub is_error: Option<bool>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    /// Some versions put the text under `message`.
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    /// Terminal failure reason when not a clean success.
    #[serde(default)]
    pub terminal_reason: Option<String>,
}

impl PrintResultJson {
    pub fn into_turn(self) -> ClaudeTurnResult {
        let is_error = self.is_error.unwrap_or(false)
            || self
                .terminal_reason
                .as_deref()
                .is_some_and(|r| r != "completed" && r != "success")
            || self.subtype.as_deref() == Some("error");
        let text = self
            .result
            .or(self.message)
            .or(self.error)
            .unwrap_or_default();
        ClaudeTurnResult {
            text,
            session_id: self.session_id,
            model: None,
            is_error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_models_include_fable_and_opus_48() {
        let ids: Vec<_> = KNOWN_MODELS.iter().map(|m| m.id).collect();
        assert!(ids.contains(&"claude-fable-5[1m]"));
        assert!(ids.contains(&"claude-fable-5"));
        assert!(ids.contains(&"claude-opus-4-8"));
        assert!(ids.contains(&"claude-opus-4-8[1m]"));
        assert!(ids.contains(&"claude-sonnet-5"));
        assert!(ids.contains(&"fable"));
    }

    #[test]
    fn discover_merges_known() {
        let models = discover_models();
        assert!(models.iter().any(|m| m.id.contains("fable") || m.id.contains("opus-4-8")));
        assert!(models.iter().any(|m| m.is_default));
    }
}
