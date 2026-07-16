//! GrokBuildEnvironment configuration for the shell crate family.
//!
//! The environment presets (per-environment endpoint URLs, the staging
//! trust check) live in the [`agent_tui_env`] leaf crate so
//! sibling crates (telemetry, tools, workspace) can share them without
//! depending on this crate. This module re-exports them and hosts the
//! shell-specific gateway-bridge env vars.
//!
//! # Gateway-bridge mode (env-only)
//! - `GROK_GATEWAY_URL` — when set to a valid URL, `MvpAgent` spawns a
//!   per-session gateway bridge actor and routes prompts through
//!   it. Unset → falls back to [`GrokBuildEnvironment::gateway_ws_url`] for
//!   sessions created in gateway mode; otherwise local-mode (unchanged).
pub use agent_tui_env::{
    GrokBuildEnvironment, PROD_ASSET_SERVER_URL, PROD_CLI_CHAT_PROXY_BASE_URL, PROD_GATEWAY_WS_URL,
    PROD_RELAY_WS_URL, PROD_WS_ORIGIN,
};
/// Env var that opts a process into gateway-bridge mode. When set to
/// a parseable URL, `session/new` / `session/load` spawns a per-session
/// `gateway_bridge` actor in the shell; unset → local-mode (unchanged).
pub const GROK_GATEWAY_URL_ENV: &str = "GROK_GATEWAY_URL";
/// Client kill switch for the gateway-bridge custom-method passthrough.
/// Set to `1` / `true` to force every `custom_method` call back onto
/// agent-local dispatch regardless of the routing table or negotiated
/// capability — an instant revert without a redeploy if the channel
/// misbehaves. Unset/`0`/`false` → normal routing.
pub const GROK_DISABLE_CUSTOM_BRIDGE_ENV: &str = "GROK_DISABLE_CUSTOM_BRIDGE";
fn custom_bridge_disabled_value(value: Option<&str>) -> bool {
    value
        .map(|value| {
            let value = value.trim();
            value == "1" || value.eq_ignore_ascii_case("true")
        })
        .unwrap_or(false)
}
/// `true` when the custom-method bridge passthrough is force-disabled via
/// [`GROK_DISABLE_CUSTOM_BRIDGE_ENV`]. Accepts `1`/`true` (case-insensitive).
pub fn custom_bridge_disabled() -> bool {
    let value = std::env::var(GROK_DISABLE_CUSTOM_BRIDGE_ENV).ok();
    custom_bridge_disabled_value(value.as_deref())
}
/// Parse `GROK_GATEWAY_URL` into a [`url::Url`]. Unset, empty, or
/// malformed → `None` (malformed is warned and falls back to local
/// mode so the shell doesn't refuse to start).
///
/// The malformed-URL warning intentionally does **not** log the raw
/// env-var value — a mistyped credential URL of the form
/// `wss://user:pass@host` would leak `pass` if the parse failed. The
/// operator can inspect their own env var directly.
///
/// Hard-off (always `None`) without the `chat` feature so release
/// builds can't activate the bridge via env.
fn parse_gateway_url_value(raw: Option<&str>) -> Option<url::Url> {
    let raw = raw?;
    if raw.is_empty() {
        return None;
    }
    if true {
        tracing::warn!(
            env = GROK_GATEWAY_URL_ENV,
            "GROK_GATEWAY_URL is set but this build lacks the `chat` feature; staying in local mode"
        );
        return None;
    }
    match url::Url::parse(&raw) {
        Ok(url) => Some(url),
        Err(err) => {
            tracing::warn!(
                env = GROK_GATEWAY_URL_ENV, error = % err,
                "GROK_GATEWAY_URL is not a valid URL; falling back to local mode (raw value omitted to avoid leaking userinfo)"
            );
            None
        }
    }
}
pub fn parse_gateway_url() -> Option<url::Url> {
    let raw = std::env::var(GROK_GATEWAY_URL_ENV).ok();
    parse_gateway_url_value(raw.as_deref())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_gateway_url_returns_none_when_unset() {
        assert!(parse_gateway_url_value(None).is_none());
    }
    #[test]
    fn parse_gateway_url_returns_none_when_empty() {
        assert!(parse_gateway_url_value(Some("")).is_none());
    }
    #[test]
    fn parse_gateway_url_returns_none_for_malformed_url() {
        assert!(
            parse_gateway_url_value(Some("not a url")).is_none(),
            "malformed URL falls back to None"
        );
    }
    #[test]
    fn custom_bridge_disabled_defaults_false_when_unset() {
        assert!(!custom_bridge_disabled_value(None));
    }
    #[test]
    fn custom_bridge_disabled_true_for_one_and_true() {
        for v in ["1", "true", "TRUE", " true "] {
            assert!(
                custom_bridge_disabled_value(Some(v)),
                "{v:?} must disable the custom bridge"
            );
        }
    }
    #[test]
    fn custom_bridge_disabled_false_for_zero_and_garbage() {
        for v in ["0", "false", "", "no"] {
            assert!(
                !custom_bridge_disabled_value(Some(v)),
                "{v:?} must leave the custom bridge enabled"
            );
        }
    }
}
