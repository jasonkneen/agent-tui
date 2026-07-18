//! `/runtime` (aliases `/provider`, `/rt`) — switch **runtime addon**.
//!
//! ONE CORE + ADDONS: the TUI is fixed; this command only picks which addon
//! serves turns (`docs/CORE_AND_ADDONS.md`). Product profiles may lock or
//! filter the list.
//!
//! - `grok` — built-in xAI agent (default platform addon)
//! - `codex` — local Codex app-server (warm JSON-RPC; uses `codex login`)
//! - `claude` — Claude Code CLI harness (`claude -p`; uses Claude Code login)
//! - `lazar` — local lazar kernel (`lazar -p` stream-json; kernel owns providers/models)

use crate::app::actions::Action;
use crate::runtime_backend::{self, RuntimeBackend};
use crate::slash::command::{AppCtx, ArgItem, CommandExecCtx, CommandResult, SlashCommand};

pub struct RuntimeCommand;

impl SlashCommand for RuntimeCommand {
    fn name(&self) -> &str {
        "runtime"
    }

    fn aliases(&self) -> &[&str] {
        &["provider", "rt"]
    }

    fn description(&self) -> &str {
        if crate::product_profile::lock_runtime() {
            "Show runtime addon (locked by product profile)"
        } else {
            "Switch runtime addon (Grok / Codex / Claude / Lazar / Hermes)"
        }
    }

    fn usage(&self) -> &str {
        if crate::product_profile::lock_runtime() {
            "/runtime"
        } else {
            "/runtime [grok|codex|claude|lazar|hermes]"
        }
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn args_required(&self) -> bool {
        false
    }

    fn arg_placeholder(&self) -> Option<&str> {
        if crate::product_profile::lock_runtime() {
            None
        } else {
            Some("[grok|codex|claude|lazar|hermes]")
        }
    }

    fn suggest_args(&self, _ctx: &AppCtx, _args_query: &str) -> Option<Vec<ArgItem>> {
        if crate::product_profile::lock_runtime() {
            return None;
        }
        Some(
            runtime_backend::status_list()
                .into_iter()
                .map(|s| {
                    let mark = if s.active { "● " } else { "○ " };
                    let ready = if s.ready { "ready" } else { "not ready" };
                    ArgItem {
                        display: s.backend.as_str().to_string(),
                        match_text: s.backend.as_str().to_string(),
                        insert_text: s.backend.as_str().to_string(),
                        description: format!(
                            "{mark}{} — {ready}: {}",
                            s.backend.display_name(),
                            s.detail
                        ),
                    }
                })
                .collect(),
        )
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            return CommandResult::Message(format_status());
        }
        if crate::product_profile::lock_runtime() {
            let p = crate::product_profile::get();
            return CommandResult::Error(format!(
                "Product `{}` is locked to runtime `{}`. Edit product.toml or unset AGENT_TUI_PRODUCT to switch.",
                p.name,
                p.default_runtime.as_str()
            ));
        }
        let Some(backend) = RuntimeBackend::parse(trimmed) else {
            let allowed: Vec<&str> = crate::product_profile::enabled_runtimes()
                .iter()
                .map(|b| b.as_str())
                .collect();
            return CommandResult::Error(format!(
                "Unknown runtime `{trimmed}`. Use: {}",
                allowed.join(" | ")
            ));
        };
        if !crate::product_profile::runtime_allowed(backend) {
            return CommandResult::Error(format!(
                "Runtime `{}` is not enabled for product `{}`",
                backend.as_str(),
                crate::product_profile::display_name()
            ));
        };

        // Soft readiness checks with clear next steps.
        match backend {
            RuntimeBackend::Codex => {
                let (ready, detail) = {
                    let list = runtime_backend::status_list();
                    list.into_iter()
                        .find(|s| s.backend == RuntimeBackend::Codex)
                        .map(|s| (s.ready, s.detail))
                        .unwrap_or((false, "unknown".into()))
                };
                if !ready {
                    return CommandResult::Error(format!(
                        "Codex not ready ({detail}). Install the Codex CLI and run `codex login`."
                    ));
                }
            }
            RuntimeBackend::Claude => {
                let (ready, detail) = {
                    let list = runtime_backend::status_list();
                    list.into_iter()
                        .find(|s| s.backend == RuntimeBackend::Claude)
                        .map(|s| (s.ready, s.detail))
                        .unwrap_or((false, "unknown".into()))
                };
                if !ready {
                    return CommandResult::Error(format!(
                        "Claude not ready ({detail}). Install Claude Code and log in."
                    ));
                }
            }
            RuntimeBackend::Lazar => {
                let (ready, detail) = {
                    let list = runtime_backend::status_list();
                    list.into_iter()
                        .find(|s| s.backend == RuntimeBackend::Lazar)
                        .map(|s| (s.ready, s.detail))
                        .unwrap_or((false, "unknown".into()))
                };
                if !ready {
                    return CommandResult::Error(format!(
                        "Lazar not ready ({detail}). Install lazar (on PATH or $LAZAR_HOME/bin/lazar)."
                    ));
                }
            }
            RuntimeBackend::Hermes => {
                let (ready, detail) = {
                    let list = runtime_backend::status_list();
                    list.into_iter()
                        .find(|s| s.backend == RuntimeBackend::Hermes)
                        .map(|s| (s.ready, s.detail))
                        .unwrap_or((false, "unknown".into()))
                };
                if !ready {
                    return CommandResult::Error(format!(
                        "Hermes not ready ({detail}). Install Hermes Agent (`hermes` on PATH)."
                    ));
                }
            }
            RuntimeBackend::Grok => {}
        }

        CommandResult::Action(Action::SetRuntime(backend))
    }
}

fn format_status() -> String {
    let product = crate::product_profile::get();
    let mut lines = vec![
        format!(
            "Product: {} ({})",
            product.name, product.id
        ),
        "Runtime addons (ONE CORE + ADDONS — local harnesses, no extra OAuth):".to_string(),
        String::new(),
    ];
    for s in runtime_backend::status_list() {
        let star = if s.active { "← active" } else { "" };
        let ready = if s.ready { "ready" } else { "not ready" };
        let shape = crate::runtime_addon::get(s.backend)
            .map(|a| a.turn_shape)
            .unwrap_or("");
        lines.push(format!(
            "  {}  {} ({ready}) — {} · {shape} {star}",
            s.backend.as_str(),
            s.backend.display_name(),
            s.detail
        ));
    }
    lines.push(String::new());
    if product.lock_runtime {
        lines.push(format!(
            "Runtime locked to `{}` by product profile (product.toml / AGENT_TUI_PRODUCT).",
            product.default_runtime.as_str()
        ));
    } else {
        let switches: Vec<&str> = crate::product_profile::enabled_runtimes()
            .iter()
            .map(|b| b.as_str())
            .collect();
        lines.push(format!(
            "Switch: {}",
            switches
                .iter()
                .map(|s| format!("/runtime {s}"))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
        lines.push("After switch, /model lists that runtime's models.".into());
    }
    if product.default_runtime == RuntimeBackend::Lazar
        || lines.iter().any(|l| l.contains("lazar"))
    {
        lines.push("Lazar spawns `lazar -p` per turn; the kernel owns providers and models.".into());
    }
    lines.join("\n")
}
