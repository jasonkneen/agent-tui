//! Claude Code / Anthropic local credential detection.

use super::{CredentialOrigin, DetectedCredential, LocalCliId};
use agent_tui_sampler::AuthScheme;
use agent_tui_sampling_types::ApiBackend;
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use std::path::{Path, PathBuf};

const ANTHROPIC_MESSAGES_BASE: &str = "https://api.anthropic.com/v1";
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// Ordered env vars. First set, non-blank wins.
const ENV_CANDIDATES: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
];

/// Public entry: probe Claude Code–compatible stores.
pub fn detect_claude() -> Option<DetectedCredential> {
    ClaudeCodeSource::default().detect()
}

#[derive(Debug, Clone, Default)]
pub struct ClaudeCodeSource {
    /// Override home for tests (`None` → real `$HOME` / `CLAUDE_CONFIG_DIR`).
    pub home_override: Option<PathBuf>,
    /// When true, skip macOS keychain (unit tests).
    pub skip_keychain: bool,
}

impl ClaudeCodeSource {
    pub fn detect(&self) -> Option<DetectedCredential> {
        if let Some(c) = self.from_env() {
            return Some(c);
        }
        let claude_dir = self.claude_config_dir();
        if let Some(c) = self.from_credentials_json(&claude_dir.join("credentials.json")) {
            return Some(c);
        }
        if let Some(c) = self.from_dot_credentials_json(&claude_dir.join(".credentials.json")) {
            return Some(c);
        }
        if !self.skip_keychain {
            if let Some(c) = self.from_macos_keychain() {
                return Some(c);
            }
        }
        // OpenCode's Anthropic OAuth file (same vendor tokens, different CLI).
        if let Some(home) = self.user_home() {
            if let Some(c) = self.from_opencode_anthropic(&home.join(".opencode/anthropic-oauth.json"))
            {
                return Some(c);
            }
        }
        None
    }

    fn user_home(&self) -> Option<PathBuf> {
        self.home_override
            .clone()
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
    }

    fn claude_config_dir(&self) -> PathBuf {
        if let Some(ref h) = self.home_override {
            return h.join(".claude");
        }
        if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
            return PathBuf::from(dir);
        }
        self.user_home()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude")
    }

    fn from_env(&self) -> Option<DetectedCredential> {
        for name in ENV_CANDIDATES {
            if let Ok(val) = std::env::var(name) {
                let token = val.trim();
                if token.is_empty() {
                    continue;
                }
                return Some(make_credential(
                    token.to_string(),
                    CredentialOrigin::Env {
                        name: (*name).to_string(),
                    },
                    None,
                    None,
                ));
            }
        }
        None
    }

    /// `~/.claude/credentials.json` — flat oauth_* keys (seen on some installs).
    fn from_credentials_json(&self, path: &Path) -> Option<DetectedCredential> {
        #[derive(Deserialize)]
        struct Flat {
            #[serde(default)]
            oauth_token: Option<String>,
            #[serde(default)]
            access_token: Option<String>,
            #[serde(default)]
            oauth_refresh_token: Option<String>,
            #[serde(default)]
            oauth_expires_at: Option<i64>,
            #[serde(default)]
            expires_at: Option<i64>,
        }
        let raw = std::fs::read_to_string(path).ok()?;
        let parsed: Flat = serde_json::from_str(&raw).ok()?;
        let token = parsed
            .oauth_token
            .or(parsed.access_token)
            .filter(|t| !t.trim().is_empty())?;
        let exp = parse_expires(parsed.oauth_expires_at.or(parsed.expires_at));
        let _ = parsed.oauth_refresh_token; // held by Claude Code; we re-read file later
        Some(make_credential(
            token,
            CredentialOrigin::File {
                path: path.to_path_buf(),
            },
            exp,
            None,
        ))
    }

    /// Official Linux layout: `{ "claudeAiOauth": { accessToken, … } }`.
    fn from_dot_credentials_json(&self, path: &Path) -> Option<DetectedCredential> {
        #[derive(Deserialize)]
        struct Root {
            #[serde(rename = "claudeAiOauth")]
            claude_ai_oauth: Option<OauthBlock>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct OauthBlock {
            access_token: Option<String>,
            #[serde(default)]
            refresh_token: Option<String>,
            #[serde(default)]
            expires_at: Option<i64>,
            #[serde(default)]
            subscription_type: Option<String>,
        }
        let raw = std::fs::read_to_string(path).ok()?;
        let root: Root = serde_json::from_str(&raw).ok()?;
        let block = root.claude_ai_oauth?;
        let token = block.access_token.filter(|t| !t.trim().is_empty())?;
        let exp = parse_expires(block.expires_at);
        let _ = block.refresh_token;
        Some(make_credential(
            token,
            CredentialOrigin::File {
                path: path.to_path_buf(),
            },
            exp,
            block.subscription_type,
        ))
    }

    /// OpenCode stores Anthropic OAuth as `{ type, access, refresh, expires }`.
    fn from_opencode_anthropic(&self, path: &Path) -> Option<DetectedCredential> {
        #[derive(Deserialize)]
        struct Oc {
            #[serde(default)]
            access: Option<String>,
            #[serde(default)]
            expires: Option<i64>,
        }
        let raw = std::fs::read_to_string(path).ok()?;
        let parsed: Oc = serde_json::from_str(&raw).ok()?;
        let token = parsed.access.filter(|t| !t.trim().is_empty())?;
        Some(make_credential(
            token,
            CredentialOrigin::File {
                path: path.to_path_buf(),
            },
            parse_expires(parsed.expires),
            Some("opencode".into()),
        ))
    }

    #[cfg(target_os = "macos")]
    fn from_macos_keychain(&self) -> Option<DetectedCredential> {
        // Avoid pulling security-framework; `security -w` prints the password only.
        let output = std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                KEYCHAIN_SERVICE,
                "-w",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let secret = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if secret.is_empty() {
            return None;
        }
        // Keychain payload is often JSON (same shapes as .credentials.json).
        if let Ok(root) = serde_json::from_str::<serde_json::Value>(&secret) {
            if let Some(c) = credential_from_json_value(
                &root,
                CredentialOrigin::Keychain {
                    service: KEYCHAIN_SERVICE.into(),
                },
            ) {
                return Some(c);
            }
        }
        // Bare token string.
        Some(make_credential(
            secret,
            CredentialOrigin::Keychain {
                service: KEYCHAIN_SERVICE.into(),
            },
            None,
            None,
        ))
    }

    #[cfg(not(target_os = "macos"))]
    fn from_macos_keychain(&self) -> Option<DetectedCredential> {
        None
    }
}

fn credential_from_json_value(
    root: &serde_json::Value,
    origin: CredentialOrigin,
) -> Option<DetectedCredential> {
    // claudeAiOauth nested
    if let Some(block) = root.get("claudeAiOauth") {
        let token = block
            .get("accessToken")
            .or_else(|| block.get("access_token"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())?;
        let exp = block
            .get("expiresAt")
            .or_else(|| block.get("expires_at"))
            .and_then(|v| v.as_i64());
        let hint = block
            .get("subscriptionType")
            .or_else(|| block.get("subscription_type"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        return Some(make_credential(
            token.to_string(),
            origin,
            parse_expires(exp),
            hint,
        ));
    }
    // flat oauth_token
    let token = root
        .get("oauth_token")
        .or_else(|| root.get("access_token"))
        .or_else(|| root.get("accessToken"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    let exp = root
        .get("oauth_expires_at")
        .or_else(|| root.get("expires_at"))
        .or_else(|| root.get("expiresAt"))
        .and_then(|v| v.as_i64());
    Some(make_credential(
        token.to_string(),
        origin,
        parse_expires(exp),
        None,
    ))
}

fn parse_expires(raw: Option<i64>) -> Option<DateTime<Utc>> {
    let v = raw?;
    // Claude uses ms epoch in some builds, seconds in others.
    let secs = if v > 10_000_000_000 { v / 1000 } else { v };
    Utc.timestamp_opt(secs, 0).single()
}

fn auth_scheme_for_token(token: &str) -> AuthScheme {
    // OAuth / subscription tokens are typically `sk-ant-oat…` → Bearer.
    // Console API keys are `sk-ant-api…` → x-api-key.
    if token.starts_with("sk-ant-oat") || token.starts_with("sk-ant-ort") {
        AuthScheme::Bearer
    } else if token.starts_with("sk-ant-") {
        AuthScheme::XApiKey
    } else {
        // Unknown shape: Claude Code OAuth is usually Bearer.
        AuthScheme::Bearer
    }
}

fn make_credential(
    token: String,
    origin: CredentialOrigin,
    expires_at: Option<DateTime<Utc>>,
    account_hint: Option<String>,
) -> DetectedCredential {
    let auth_scheme = auth_scheme_for_token(&token);
    DetectedCredential {
        cli: LocalCliId::ClaudeCode,
        origin,
        token,
        auth_scheme,
        expires_at,
        base_url: ANTHROPIC_MESSAGES_BASE.to_string(),
        api_backend: ApiBackend::Messages,
        account_hint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn auth_scheme_api_key_vs_oauth() {
        assert!(matches!(
            auth_scheme_for_token("sk-ant-api03-abcdef"),
            AuthScheme::XApiKey
        ));
        assert!(matches!(
            auth_scheme_for_token("sk-ant-oat01-abcdef"),
            AuthScheme::Bearer
        ));
    }

    #[test]
    fn parse_expires_ms_and_secs() {
        let secs = parse_expires(Some(1_700_000_000)).unwrap();
        let ms = parse_expires(Some(1_700_000_000_000)).unwrap();
        assert_eq!(secs.timestamp(), ms.timestamp());
    }

    #[test]
    fn detects_flat_credentials_json() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        let path = claude.join("credentials.json");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            r#"{{"oauth_token":"sk-ant-oat01-testtoken","oauth_expires_at":9999999999}}"#
        )
        .unwrap();

        let src = ClaudeCodeSource {
            home_override: Some(dir.path().to_path_buf()),
            skip_keychain: true,
        };
        let cred = src.detect().expect("detect");
        assert_eq!(cred.token, "sk-ant-oat01-testtoken");
        assert!(matches!(cred.auth_scheme, AuthScheme::Bearer));
        assert!(matches!(cred.api_backend, ApiBackend::Messages));
        assert!(matches!(cred.origin, CredentialOrigin::File { .. }));
    }

    #[test]
    fn detects_claude_ai_oauth_layout() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        let path = claude.join(".credentials.json");
        std::fs::write(
            &path,
            r#"{
              "claudeAiOauth": {
                "accessToken": "sk-ant-oat01-nested",
                "refreshToken": "sk-ant-ort01-nested",
                "expiresAt": 9999999999000,
                "subscriptionType": "max"
              }
            }"#,
        )
        .unwrap();

        let src = ClaudeCodeSource {
            home_override: Some(dir.path().to_path_buf()),
            skip_keychain: true,
        };
        let cred = src.detect().expect("detect");
        assert_eq!(cred.token, "sk-ant-oat01-nested");
        assert_eq!(cred.account_hint.as_deref(), Some("max"));
    }

    #[test]
    fn env_wins_over_file() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("credentials.json"),
            r#"{"oauth_token":"sk-ant-oat01-file"}"#,
        )
        .unwrap();

        // SAFETY: test-only, serial env mutation is acceptable for this unit.
        let prev = std::env::var("ANTHROPIC_API_KEY").ok();
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-api03-from-env") };
        let src = ClaudeCodeSource {
            home_override: Some(dir.path().to_path_buf()),
            skip_keychain: true,
        };
        let cred = src.detect().expect("detect");
        assert_eq!(cred.token, "sk-ant-api03-from-env");
        assert!(matches!(cred.auth_scheme, AuthScheme::XApiKey));
        match prev {
            Some(v) => unsafe { std::env::set_var("ANTHROPIC_API_KEY", v) },
            None => unsafe { std::env::remove_var("ANTHROPIC_API_KEY") },
        }
    }
}
