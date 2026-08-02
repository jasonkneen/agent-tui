//! Warm multi-turn Claude Code sessions (sticky `--resume` + idle timeout).

use crate::error::{ClaudeRuntimeError, Result};
use crate::protocol::{ClaudeTurnResult, PrintResultJson};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, info, warn};

/// Configuration for the Claude runtime pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// `claude` binary name or path.
    pub claude_bin: PathBuf,
    /// Working directory for turns.
    pub cwd: Option<PathBuf>,
    /// Drop sticky session after this idle period. Default 10 minutes.
    pub idle_timeout: Duration,
    /// Per-turn wall clock timeout. Default 10 minutes.
    pub turn_timeout: Duration,
    /// Default model alias (`sonnet`, `opus`, full id).
    pub default_model: Option<String>,
    /// Default permission policy for headless turns.
    pub default_permission_mode: PermissionMode,
}

/// Permission contract inherited from the Agent TUI session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionMode {
    /// Do not approve tool actions without the vendor's normal checks.
    #[default]
    Ask,
    /// Let Claude Code apply its automatic permission policy.
    Auto,
    /// Explicit user opt-in to bypass vendor permission prompts.
    AlwaysApprove,
}

impl PermissionMode {
    fn claude_cli_value(self) -> &'static str {
        match self {
            Self::Ask => "manual",
            Self::Auto => "auto",
            Self::AlwaysApprove => "bypassPermissions",
        }
    }
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            claude_bin: PathBuf::from("claude"),
            cwd: None,
            idle_timeout: Duration::from_secs(600),
            turn_timeout: Duration::from_secs(600),
            default_model: None,
            default_permission_mode: PermissionMode::Ask,
        }
    }
}

#[derive(Debug)]
struct SessionState {
    sticky_session_id: Option<String>,
    sticky_model: Option<String>,
    sticky_permission_mode: PermissionMode,
    last_used: Instant,
}

#[derive(Default)]
struct PoolState {
    sessions: HashMap<String, SessionState>,
}

const DEFAULT_STICKY_KEY: &str = "default";

/// Keyed pool of sticky Claude Code sessions, recreated on idle/model/policy change.
pub struct ClaudeRuntimePool {
    config: PoolConfig,
    inner: Mutex<PoolState>,
}

impl ClaudeRuntimePool {
    pub fn new(config: PoolConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            inner: Mutex::new(PoolState::default()),
        })
    }

    /// Ensure the `claude` binary is resolvable.
    pub fn ensure_binary(&self) -> Result<()> {
        if which_bin(&self.config.claude_bin) {
            Ok(())
        } else {
            Err(ClaudeRuntimeError::BinaryNotFound)
        }
    }

    /// Run one text turn. Reuses sticky session when model matches and not idle.
    pub async fn start_text_turn(
        &self,
        prompt: impl Into<String>,
        model: Option<String>,
    ) -> Result<ClaudeTurnResult> {
        self.start_text_turn_keyed(prompt, model, None, self.config.default_permission_mode)
            .await
    }

    /// Run one text turn with continuity isolated by `sticky_key`.
    pub async fn start_text_turn_keyed(
        &self,
        prompt: impl Into<String>,
        model: Option<String>,
        sticky_key: Option<&str>,
        permission_mode: PermissionMode,
    ) -> Result<ClaudeTurnResult> {
        self.ensure_binary()?;
        let prompt = prompt.into();
        let model = model.or_else(|| self.config.default_model.clone());
        let key = sticky_key
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .unwrap_or(DEFAULT_STICKY_KEY)
            .to_string();

        let resume = {
            let mut pool = self.inner.lock().await;
            let state = pool
                .sessions
                .entry(key.clone())
                .or_insert_with(|| SessionState {
                    sticky_session_id: None,
                    sticky_model: None,
                    sticky_permission_mode: permission_mode,
                    last_used: Instant::now(),
                });
            if state.last_used.elapsed() >= self.config.idle_timeout {
                if state.sticky_session_id.is_some() {
                    info!("claude runtime: idle timeout — dropping sticky session");
                }
                state.sticky_session_id = None;
                state.sticky_model = None;
            }
            let reuse = state.sticky_session_id.is_some()
                && state.sticky_model == model
                && state.sticky_permission_mode == permission_mode;
            if reuse {
                state.sticky_session_id.clone()
            } else {
                state.sticky_session_id = None;
                state.sticky_model = model.clone();
                state.sticky_permission_mode = permission_mode;
                None
            }
        };

        debug!(
            resume = resume.is_some(),
            model = ?model,
            "claude runtime: starting turn"
        );

        let result = self
            .run_print_turn(
                &prompt,
                model.as_deref(),
                resume.as_deref(),
                permission_mode,
            )
            .await?;

        {
            let mut pool = self.inner.lock().await;
            let state = pool.sessions.entry(key).or_insert_with(|| SessionState {
                sticky_session_id: None,
                sticky_model: None,
                sticky_permission_mode: permission_mode,
                last_used: Instant::now(),
            });
            state.last_used = Instant::now();
            if let Some(ref sid) = result.session_id {
                state.sticky_session_id = Some(sid.clone());
            }
            state.sticky_model = model;
            state.sticky_permission_mode = permission_mode;
        }

        if result.is_error {
            return Err(ClaudeRuntimeError::Api(if result.text.is_empty() {
                "claude reported an error with empty body".into()
            } else {
                result.text
            }));
        }
        Ok(result)
    }

    /// Drop sticky session (e.g. after model change).
    pub async fn reset_session(&self) {
        self.inner.lock().await.sessions.clear();
    }

    /// Drop continuity for one Agent TUI session.
    pub async fn reset_session_key(&self, key: &str) {
        self.inner.lock().await.sessions.remove(key);
    }

    async fn run_print_turn(
        &self,
        prompt: &str,
        model: Option<&str>,
        resume: Option<&str>,
        permission_mode: PermissionMode,
    ) -> Result<ClaudeTurnResult> {
        let mut cmd = Command::new(&self.config.claude_bin);
        cmd.arg("-p")
            .arg("--output-format")
            .arg("json")
            .arg("--permission-mode")
            .arg(permission_mode.claude_cli_value())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if permission_mode == PermissionMode::AlwaysApprove {
            cmd.arg("--dangerously-skip-permissions");
        }

        if let Some(cwd) = &self.config.cwd {
            cmd.current_dir(cwd);
        }
        if let Some(m) = model {
            cmd.arg("--model").arg(m);
        }
        if let Some(sid) = resume {
            cmd.arg("--resume").arg(sid);
        }

        let mut child = cmd.spawn().map_err(ClaudeRuntimeError::Spawn)?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| ClaudeRuntimeError::Other("claude stdin missing".into()))?;
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(ClaudeRuntimeError::Spawn)?;
        stdin.shutdown().await.map_err(ClaudeRuntimeError::Spawn)?;
        drop(stdin);
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| ClaudeRuntimeError::Other("claude stdout missing".into()))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| ClaudeRuntimeError::Other("claude stderr missing".into()))?;

        let out_fut = async {
            let mut buf = Vec::new();
            stdout.read_to_end(&mut buf).await?;
            Ok::<_, std::io::Error>(buf)
        };
        let err_fut = async {
            let mut buf = Vec::new();
            stderr.read_to_end(&mut buf).await?;
            Ok::<_, std::io::Error>(buf)
        };

        let join = async {
            let (out, err) = tokio::try_join!(out_fut, err_fut)?;
            let status = child.wait().await?;
            Ok::<_, std::io::Error>((out, err, status))
        };

        let (out, err, status) = match timeout(self.config.turn_timeout, join).await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => return Err(ClaudeRuntimeError::Spawn(e)),
            Err(_) => {
                let _ = child.kill().await;
                return Err(ClaudeRuntimeError::Timeout(self.config.turn_timeout));
            }
        };

        let stdout_str = String::from_utf8_lossy(&out);
        let stderr_str = String::from_utf8_lossy(&err);
        if !status.success() {
            // Claude sometimes still emits a valid JSON result with is_error on
            // non-zero exit — prefer parsing that.
            if let Ok(parsed) = parse_print_json(&stdout_str) {
                if !parsed.text.is_empty() || parsed.is_error {
                    return Ok(parsed);
                }
            }
            let code = status.code().unwrap_or(-1);
            let msg = if stderr_str.trim().is_empty() {
                stdout_str.trim().to_string()
            } else {
                stderr_str.trim().to_string()
            };
            return Err(ClaudeRuntimeError::Exit { code, stderr: msg });
        }

        parse_print_json(&stdout_str).or_else(|e| {
            warn!(error = %e, "claude json parse failed");
            // Fall back to raw stdout if non-empty.
            let trimmed = stdout_str.trim();
            if trimmed.is_empty() {
                Err(e)
            } else {
                Ok(ClaudeTurnResult {
                    text: trimmed.to_string(),
                    session_id: None,
                    model: model.map(str::to_string),
                    is_error: false,
                })
            }
        })
    }
}

fn parse_print_json(stdout: &str) -> Result<ClaudeTurnResult> {
    // Prefer the last JSON object line (stream-json hybrids sometimes prefix).
    let candidate = stdout
        .lines()
        .rev()
        .find(|l| {
            let t = l.trim();
            t.starts_with('{') && t.contains("\"type\"")
        })
        .unwrap_or(stdout)
        .trim();

    let parsed: PrintResultJson = serde_json::from_str(candidate).map_err(|e| {
        ClaudeRuntimeError::BadJson(format!("{e}; body={}", truncate(candidate, 240)))
    })?;
    Ok(parsed.into_turn())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    }
}

fn which_bin(bin: &std::path::Path) -> bool {
    which::which(bin).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_success_result() {
        let raw = r#"{"type":"result","subtype":"success","is_error":false,"result":"hello","session_id":"abc"}"#;
        let t = parse_print_json(raw).unwrap();
        assert_eq!(t.text, "hello");
        assert_eq!(t.session_id.as_deref(), Some("abc"));
        assert!(!t.is_error);
    }

    #[test]
    fn parse_error_result() {
        let raw =
            r#"{"type":"result","is_error":true,"result":"boom","terminal_reason":"api_error"}"#;
        let t = parse_print_json(raw).unwrap();
        assert!(t.is_error);
        assert_eq!(t.text, "boom");
    }

    #[test]
    fn permission_modes_are_explicit_and_fail_closed_by_default() {
        assert_eq!(PermissionMode::default(), PermissionMode::Ask);
        assert_eq!(PermissionMode::Ask.claude_cli_value(), "manual");
        assert_eq!(PermissionMode::Auto.claude_cli_value(), "auto");
        assert_eq!(
            PermissionMode::AlwaysApprove.claude_cli_value(),
            "bypassPermissions"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn prompt_uses_stdin_and_sessions_are_keyed() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("fake-claude");
        let args_log = temp.path().join("args.log");
        let stdin_log = temp.path().join("stdin.log");
        std::fs::write(
            &bin,
            r#"#!/bin/sh
printf 'CALL\n' >> "$CLAUDE_TEST_ARGS"
printf '<%s>\n' "$@" >> "$CLAUDE_TEST_ARGS"
input=$(cat)
printf '%s' "$input" >> "$CLAUDE_TEST_STDIN"
printf '\n' >> "$CLAUDE_TEST_STDIN"
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"ok","session_id":"fake-session"}'
"#,
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&bin, permissions).unwrap();

        let pool = ClaudeRuntimePool::new(PoolConfig {
            claude_bin: bin,
            turn_timeout: Duration::from_secs(5),
            ..PoolConfig::default()
        });
        // Set env only on the fake process by wrapping the binary's inherited
        // test environment. The test crate runs this one serially in practice;
        // paths are unique even if another test happens to set the same names.
        unsafe {
            std::env::set_var("CLAUDE_TEST_ARGS", &args_log);
            std::env::set_var("CLAUDE_TEST_STDIN", &stdin_log);
        }

        pool.start_text_turn_keyed("first secret", None, Some("agent-a"), PermissionMode::Ask)
            .await
            .unwrap();
        pool.start_text_turn_keyed("second secret", None, Some("agent-a"), PermissionMode::Ask)
            .await
            .unwrap();
        pool.start_text_turn_keyed("third secret", None, Some("agent-b"), PermissionMode::Ask)
            .await
            .unwrap();

        let args = std::fs::read_to_string(args_log).unwrap();
        let calls: Vec<&str> = args.split("CALL\n").filter(|s| !s.is_empty()).collect();
        assert_eq!(calls.len(), 3);
        assert!(!calls[0].contains("--resume"));
        assert!(calls[1].contains("--resume") && calls[1].contains("fake-session"));
        assert!(!calls[2].contains("--resume"));
        assert!(!args.contains("secret"), "prompt leaked into argv");
        assert!(calls.iter().all(|call| call.contains("<manual>")));

        assert_eq!(
            std::fs::read_to_string(stdin_log).unwrap(),
            "first secret\nsecond secret\nthird secret\n"
        );
        unsafe {
            std::env::remove_var("CLAUDE_TEST_ARGS");
            std::env::remove_var("CLAUDE_TEST_STDIN");
        }
    }
}
