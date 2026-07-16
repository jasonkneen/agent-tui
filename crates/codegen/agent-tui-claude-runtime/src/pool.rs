//! Warm multi-turn Claude Code sessions (sticky `--resume` + idle timeout).

use crate::error::{ClaudeRuntimeError, Result};
use crate::protocol::{ClaudeTurnResult, PrintResultJson};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
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
    /// Claude Code permission mode for headless agent use.
    pub permission_mode: String,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            claude_bin: PathBuf::from("claude"),
            cwd: None,
            idle_timeout: Duration::from_secs(600),
            turn_timeout: Duration::from_secs(600),
            default_model: None,
            // Agent TUI owns the UX; accept edits without interactive prompts.
            permission_mode: "acceptEdits".into(),
        }
    }
}

struct PoolState {
    sticky_session_id: Option<String>,
    sticky_model: Option<String>,
    last_used: Instant,
}

/// Single-slot pool: sticky Claude Code session id, recreated on idle/model change.
pub struct ClaudeRuntimePool {
    config: PoolConfig,
    inner: Mutex<PoolState>,
}

impl ClaudeRuntimePool {
    pub fn new(config: PoolConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            inner: Mutex::new(PoolState {
                sticky_session_id: None,
                sticky_model: None,
                last_used: Instant::now(),
            }),
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
        self.ensure_binary()?;
        let prompt = prompt.into();
        let model = model.or_else(|| self.config.default_model.clone());

        let resume = {
            let mut state = self.inner.lock().await;
            if state.last_used.elapsed() >= self.config.idle_timeout {
                if state.sticky_session_id.is_some() {
                    info!("claude runtime: idle timeout — dropping sticky session");
                }
                state.sticky_session_id = None;
                state.sticky_model = None;
            }
            let reuse = state.sticky_session_id.is_some() && state.sticky_model == model;
            if reuse {
                state.sticky_session_id.clone()
            } else {
                state.sticky_session_id = None;
                state.sticky_model = model.clone();
                None
            }
        };

        debug!(
            resume = resume.is_some(),
            model = ?model,
            "claude runtime: starting turn"
        );

        let result = self
            .run_print_turn(&prompt, model.as_deref(), resume.as_deref())
            .await?;

        {
            let mut state = self.inner.lock().await;
            state.last_used = Instant::now();
            if let Some(ref sid) = result.session_id {
                state.sticky_session_id = Some(sid.clone());
            }
            state.sticky_model = model;
        }

        if result.is_error {
            return Err(ClaudeRuntimeError::Api(
                if result.text.is_empty() {
                    "claude reported an error with empty body".into()
                } else {
                    result.text
                },
            ));
        }
        Ok(result)
    }

    /// Drop sticky session (e.g. after model change).
    pub async fn reset_session(&self) {
        let mut state = self.inner.lock().await;
        state.sticky_session_id = None;
        state.sticky_model = None;
    }

    async fn run_print_turn(
        &self,
        prompt: &str,
        model: Option<&str>,
        resume: Option<&str>,
    ) -> Result<ClaudeTurnResult> {
        let mut cmd = Command::new(&self.config.claude_bin);
        cmd.arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("json")
            .arg("--permission-mode")
            .arg(&self.config.permission_mode)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

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
        let mut stdout = child.stdout.take().ok_or_else(|| {
            ClaudeRuntimeError::Other("claude stdout missing".into())
        })?;
        let mut stderr = child.stderr.take().ok_or_else(|| {
            ClaudeRuntimeError::Other("claude stderr missing".into())
        })?;

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
            return Err(ClaudeRuntimeError::Exit {
                code,
                stderr: msg,
            });
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
    if bin.is_absolute() {
        return bin.is_file();
    }
    let name = bin.to_string_lossy();
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let p = dir.join(name.as_ref());
                p.is_file()
            })
        })
        .unwrap_or(false)
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
        let raw = r#"{"type":"result","is_error":true,"result":"boom","terminal_reason":"api_error"}"#;
        let t = parse_print_json(raw).unwrap();
        assert!(t.is_error);
        assert_eq!(t.text, "boom");
    }
}
