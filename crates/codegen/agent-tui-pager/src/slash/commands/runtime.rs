//! `/runtime` (aliases `/provider`, `/rt`) — switch agent harness.
//!
//! - `grok` — built-in xAI agent (default)
//! - `codex` — local Codex app-server (warm JSON-RPC; uses `codex login`)
//! - `claude` — Claude Code Agent SDK harness (`claude -p`; uses Claude Code login)

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
        "Switch agent runtime (Grok / Codex / Claude)"
    }

    fn usage(&self) -> &str {
        "/runtime [grok|codex|claude]"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn args_required(&self) -> bool {
        false
    }

    fn arg_placeholder(&self) -> Option<&str> {
        Some("[grok|codex|claude]")
    }

    fn suggest_args(&self, _ctx: &AppCtx, _args_query: &str) -> Option<Vec<ArgItem>> {
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
        let Some(backend) = RuntimeBackend::parse(trimmed) else {
            return CommandResult::Error(format!(
                "Unknown runtime `{trimmed}`. Use: grok | codex | claude"
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
            RuntimeBackend::Grok => {}
        }

        CommandResult::Action(Action::SetRuntime(backend))
    }
}

fn format_status() -> String {
    let mut lines = vec![
        "Agent runtimes (local CLIs — no extra OAuth in Agent TUI):".to_string(),
        String::new(),
    ];
    for s in runtime_backend::status_list() {
        let star = if s.active { "← active" } else { "" };
        let ready = if s.ready { "ready" } else { "not ready" };
        lines.push(format!(
            "  {}  {} ({ready}) — {} {star}",
            s.backend.as_str(),
            s.backend.display_name(),
            s.detail
        ));
    }
    lines.push(String::new());
    lines.push("Switch: /runtime grok | /runtime codex | /runtime claude".into());
    lines.push("Codex turns use `codex app-server` (warm). Claude uses `claude -p` + resume.".into());
    lines.push("After switch, /model lists that runtime's models.".into());
    lines.join("\n")
}
