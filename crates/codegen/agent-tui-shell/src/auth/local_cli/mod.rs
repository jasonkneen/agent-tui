//! Detect credentials from **already-authenticated local CLIs**.
//!
//! Strategy: prefer reusing Claude Code / Codex / OpenCode logins that already
//! live on disk or in the OS keychain over implementing OAuth ourselves.
//!
//! ## Claude first
//!
//! Probe order (first hit wins):
//! 1. Env: `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`
//! 2. `~/.claude/credentials.json` (oauth_token form)
//! 3. `~/.claude/.credentials.json` (`claudeAiOauth` form)
//! 4. macOS Keychain service `Claude Code-credentials`
//! 5. `~/.opencode/anthropic-oauth.json` (same vendor, different CLI)
//!
//! Codex / OpenRouter / etc. plug in as sibling `LocalCliSource` impls later.
//!
//! Tokens are **never** written into Agent TUI's `auth.json` by detectors —
//! callers re-read on demand so refresh stays with the source CLI.

mod claude;

pub use claude::{ClaudeCodeSource, detect_claude};

use agent_tui_sampler::AuthScheme;
use agent_tui_sampling_types::ApiBackend;
use chrono::{DateTime, Utc};

/// Stable id for a local CLI credential source (not an ACP auth method id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalCliId {
    /// Anthropic Claude Code CLI (+ compatible stores).
    ClaudeCode,
    /// OpenAI Codex CLI (ChatGPT or API key) — planned.
    Codex,
    /// OpenCode multi-provider store — planned.
    OpenCode,
}

impl LocalCliId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
        }
    }
}

/// Where a detected credential came from (for UI / diagnostics only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialOrigin {
    Env { name: String },
    File { path: std::path::PathBuf },
    Keychain { service: String },
}

impl std::fmt::Display for CredentialOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Env { name } => write!(f, "env:{name}"),
            Self::File { path } => write!(f, "file:{}", path.display()),
            Self::Keychain { service } => write!(f, "keychain:{service}"),
        }
    }
}

/// A usable credential harvested from a local CLI install.
#[derive(Clone)]
pub struct DetectedCredential {
    pub cli: LocalCliId,
    pub origin: CredentialOrigin,
    /// Secret material — treat as sensitive; Debug redacts.
    pub token: String,
    pub auth_scheme: AuthScheme,
    pub expires_at: Option<DateTime<Utc>>,
    /// Suggested inference base URL for this vendor.
    pub base_url: String,
    /// Suggested sampler protocol shape.
    pub api_backend: ApiBackend,
    /// Optional human label (email / plan) when known without extra I/O.
    pub account_hint: Option<String>,
}

impl DetectedCredential {
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .is_some_and(|exp| exp <= Utc::now() + chrono::Duration::seconds(60))
    }

    /// True when this credential should be preferred for Anthropic Messages models.
    pub fn is_anthropic(&self) -> bool {
        matches!(self.cli, LocalCliId::ClaudeCode)
            || self.base_url.contains("anthropic.com")
    }
}

impl std::fmt::Debug for DetectedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DetectedCredential")
            .field("cli", &self.cli.as_str())
            .field("origin", &self.origin.to_string())
            .field("token", &crate::auth::token_suffix(&self.token))
            .field("auth_scheme", &self.auth_scheme)
            .field("expires_at", &self.expires_at)
            .field("base_url", &self.base_url)
            .field("api_backend", &self.api_backend)
            .field("account_hint", &self.account_hint)
            .field("expired", &self.is_expired())
            .finish()
    }
}

/// Probe all known local CLIs. Claude first; others append when implemented.
pub fn detect_all_local_cli_credentials() -> Vec<DetectedCredential> {
    let mut out = Vec::new();
    if let Some(c) = detect_claude() {
        out.push(c);
    }
    // Future: detect_codex(), detect_opencode()
    out
}

/// First non-expired Claude credential, if any.
pub fn detect_preferred_claude() -> Option<DetectedCredential> {
    detect_claude().filter(|c| !c.is_expired())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_cli_ids_are_stable_wire_strings() {
        assert_eq!(LocalCliId::ClaudeCode.as_str(), "claude-code");
        assert_eq!(LocalCliId::Codex.as_str(), "codex");
    }
}
