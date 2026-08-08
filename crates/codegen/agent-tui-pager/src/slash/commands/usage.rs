//! `/usage` — session token/cost; consumer accounts can also manage billing.
//!
//! External-auth deployments (`auth_provider_command`) never reach grok.com
//! billing, so the command is hidden and refused via
//! [`AppCtx::usage_command_visible`].

use crate::app::actions::Action;
use crate::slash::command::{AppCtx, ArgItem, CommandExecCtx, CommandResult, SlashCommand};
use agent_client_protocol as acp;

/// Show coding credit usage or manage billing.
///
/// `/usage`        -- show current credit usage
/// `/usage show`   -- same as above
/// `/usage manage` -- open billing management page in browser
pub struct UsageCommand;

/// Detect external-auth installs once at pager startup.
pub(crate) fn detect_external_auth_provider(auth_methods: &[acp::AuthMethod]) -> bool {
    auth_methods.iter().any(auth_method_is_external_provider)
        || auth_provider_env_set()
        || auth_provider_config_set()
}

fn auth_method_is_external_provider(method: &acp::AuthMethod) -> bool {
    method
        .meta()
        .as_ref()
        .and_then(|v| v.get("external_provider"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn auth_provider_env_set() -> bool {
    std::env::var("GROK_AUTH_PROVIDER_COMMAND")
        .ok()
        .is_some_and(|s| !s.trim().is_empty())
}

fn auth_provider_config_set() -> bool {
    let Ok(raw) = agent_tui_shell::config::load_effective_config() else {
        return false;
    };
    let Ok(cfg) = agent_tui_shell::agent::config::Config::new_from_toml_cfg(&raw) else {
        return false;
    };
    cfg.grok_com_config
        .auth_provider_command
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
}

impl SlashCommand for UsageCommand {
    fn name(&self) -> &str {
        "usage"
    }

    /// `/cost` is the minimal-mode name for the same credit-usage summary:
    /// it commits a usage/cost system block rather than opening a
    /// pane, so it's an alias rather than a separate command.
    fn aliases(&self) -> &[&str] {
        &["cost"]
    }

    fn description(&self) -> &str {
        "View credit usage or manage billing"
    }

    fn usage(&self) -> &str {
        "/usage [show|manage]"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn visible(&self, ctx: &AppCtx) -> bool {
        ctx.usage_command_visible
    }

    fn takes_args_now(&self, ctx: &AppCtx) -> bool {
        // Non-consumer: bare `/usage` only — Enter should send, not chain for args.
        ctx.usage_command_visible && ctx.billing_surface_visible
    }

    fn suggest_args(&self, ctx: &AppCtx, _args_query: &str) -> Option<Vec<ArgItem>> {
        if !ctx.usage_command_visible || !ctx.billing_surface_visible {
            return None;
        }
        Some(vec![
            ArgItem {
                display: "show".to_string(),
                match_text: "show".to_string(),
                insert_text: "show".to_string(),
                description: "View credit usage".to_string(),
            },
            ArgItem {
                display: "manage".to_string(),
                match_text: "manage".to_string(),
                insert_text: "manage".to_string(),
                description: "Open billing management page".to_string(),
            },
        ])
    }

    fn run(&self, ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        if !ctx.usage_command_visible {
            return CommandResult::Error("/usage is not available.".into());
        }
        let arg = args.trim();
        match arg {
            "" | "show" => CommandResult::Action(Action::ShowUsage),
            "manage" => {
                CommandResult::Action(Action::OpenUrl("https://grok.com/?_s=usage".to_string()))
            }
            _ => CommandResult::Error(format!(
                "Unknown argument: {arg}. Use /usage show or /usage manage"
            )),
        }
    }
}
