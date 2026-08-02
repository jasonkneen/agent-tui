//! Spawn-per-turn client for the **lazar** agent kernel.
//!
//! Lazar is a self-evolving agent kernel driven as a CLI:
//!
//! ```text
//! lazar -p <prompt> --output-format stream-json --model <id> --session <id>
//! ```
//!
//! It emits JSONL events on stdout (`text_delta`, `tool_use`, `tool_result`,
//! `text_done`, `error`, …). The kernel owns providers, model config, auth,
//! tools, and skills; this crate only runs turns and parses the event stream.
//! It never modifies the kernel.
//!
//! Session continuity lives kernel-side: reusing `--session <id>` makes the
//! kernel prepend prior turns from its own `logs/sessions/<id>.jsonl`.

use serde_json::Value;
use std::fmt;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

/// Default per-turn wall clock timeout.
const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(600);

/// Errors from the lazar runtime.
#[derive(Debug)]
pub enum LazarRuntimeError {
    /// Failed to spawn the `lazar` binary.
    Spawn(std::io::Error),
    /// I/O failure while reading the event stream.
    Io(std::io::Error),
    /// The kernel emitted an `error` event.
    Turn(String),
    /// Turn exceeded the wall-clock timeout.
    Timeout(Duration),
    /// Non-zero exit with no reply text (code, stderr diagnostics).
    Exit(Option<i32>, String),
}

impl fmt::Display for LazarRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "failed to spawn lazar: {e}"),
            Self::Io(e) => write!(f, "lazar stream I/O: {e}"),
            Self::Turn(msg) => write!(f, "{msg}"),
            Self::Timeout(d) => write!(f, "lazar turn timed out after {}s", d.as_secs()),
            Self::Exit(code, stderr) => {
                let code_str = code.map(|c| c.to_string()).unwrap_or_else(|| "unknown".into());
                let trimmed = stderr.trim();
                if trimmed.is_empty() {
                    write!(f, "lazar exited with code {code_str}")
                } else {
                    write!(f, "lazar exited with code {code_str}: {trimmed}")
                }
            }
        }
    }
}

impl std::error::Error for LazarRuntimeError {}

pub type Result<T> = std::result::Result<T, LazarRuntimeError>;

/// Configuration for the lazar runtime pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// `lazar` binary name or path.
    pub lazar_bin: PathBuf,
    /// Working directory for the spawned kernel. Should be the kernel home
    /// (`$LAZAR_HOME`, default `~/lazar`) so it resolves skills/hooks/memory/
    /// sessions exactly as its own launcher does.
    pub cwd: Option<PathBuf>,
    /// Per-turn wall clock timeout. Default 10 minutes.
    pub turn_timeout: Duration,
    /// Default model id (kernel `--model`). Re-resolved per turn when `None`.
    pub default_model: Option<String>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            lazar_bin: PathBuf::from("lazar"),
            cwd: None,
            turn_timeout: DEFAULT_TURN_TIMEOUT,
            default_model: None,
        }
    }
}

/// Result of one lazar turn.
#[derive(Debug, Clone)]
pub struct LazarTurnResult {
    /// Concatenated assistant text (`text_delta` events).
    pub text: String,
    /// Kernel session id used for this turn.
    pub session_id: String,
}

#[derive(Default)]
struct PoolState {
    session_id: Option<String>,
}

/// Single-slot pool: sticky kernel session id, reset on demand.
///
/// There is no warm process — the kernel is spawn-per-turn by design (the
/// same contract its Go TUI uses); continuity is the `--session` flag.
pub struct LazarRuntimePool {
    config: PoolConfig,
    state: Mutex<PoolState>,
}

impl LazarRuntimePool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            state: Mutex::new(PoolState::default()),
        }
    }

    /// Drop the sticky session so the next turn starts a fresh kernel session.
    pub async fn reset_session(&self) {
        self.state.lock().await.session_id = None;
    }

    /// Pin a kernel session id for multi-turn continuity (tests / parity evals).
    /// Empty string is ignored. Charset is the kernel's a-zA-Z0-9-_. max 64.
    pub async fn set_session(&self, session_id: impl Into<String>) {
        let sid = session_id.into();
        if sid.is_empty() {
            return;
        }
        self.state.lock().await.session_id = Some(sid);
    }

    /// Current sticky session id, if any.
    pub async fn session_id(&self) -> Option<String> {
        self.state.lock().await.session_id.clone()
    }

    /// Run one text turn; returns the concatenated assistant reply.
    pub async fn start_text_turn(&self, prompt: &str, model: Option<&str>) -> Result<LazarTurnResult> {
        self.start_text_turn_keyed(prompt, model, None).await
    }

    /// Run one text turn with an optional sticky key (e.g. Agent TUI session id).
    pub async fn start_text_turn_keyed(
        &self,
        prompt: &str,
        model: Option<&str>,
        sticky_key: Option<&str>,
    ) -> Result<LazarTurnResult> {
        let session_id = if let Some(key) = sticky_key.filter(|k| !k.is_empty()) {
            let sanitized = sanitize_session_id(key);
            self.state.lock().await.session_id = Some(sanitized.clone());
            sanitized
        } else {
            let mut state = self.state.lock().await;
            state.session_id.get_or_insert_with(new_session_id).clone()
        };
        let model = model
            .map(str::to_string)
            .or_else(|| self.config.default_model.clone())
            .or_else(discover_active_model);

        let mut cmd = Command::new(&self.config.lazar_bin);
        cmd.arg("--output-format").arg("stream-json");
        if let Some(m) = &model {
            cmd.arg("--model").arg(m);
        }
        cmd.arg("--session").arg(&session_id);
        cmd.arg("-p").arg(prompt);
        if let Some(cwd) = &self.config.cwd {
            cmd.current_dir(cwd);
        }
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        let turn_timeout = self.config.turn_timeout;
        let work = async move {
            let mut child = cmd.spawn().map_err(LazarRuntimeError::Spawn)?;
            let stdout = child.stdout.take().expect("stdout piped");
            let stderr = child.stderr.take().expect("stderr piped");
            let mut lines = BufReader::new(stdout).lines();
            let mut stderr_reader = BufReader::new(stderr).lines();

            let mut reply = String::new();
            let mut turn_error: Option<String> = None;
            let mut stderr_buf = String::new();

            // Background task to capture last ~8KB of stderr diagnostics
            let stderr_handle = tokio::spawn(async move {
                let mut buf = String::new();
                while let Ok(Some(line)) = stderr_reader.next_line().await {
                    if !buf.is_empty() {
                        buf.push('\n');
                    }
                    buf.push_str(&line);
                    if buf.len() > 8192 {
                        let mut offset = buf.len() - 8192;
                        while !buf.is_char_boundary(offset) {
                            offset += 1;
                        }
                        buf = buf[offset..].to_string();
                    }
                }
                buf
            });

            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let Ok(v) = serde_json::from_str::<Value>(&line) else {
                            continue;
                        };
                        match v["type"].as_str() {
                            Some("text_delta") => {
                                if let Some(t) = v["text"].as_str() {
                                    reply.push_str(t);
                                }
                            }
                            Some("error") => {
                                turn_error = Some(
                                    v["message"]
                                        .as_str()
                                        .unwrap_or("unknown kernel error")
                                        .to_string(),
                                );
                                break;
                            }
                            // tool_use / tool_result / text_done / invoke_* — ignored
                            // by the text-flattened contract.
                            _ => {}
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let _ = child.kill().await;
                        return Err(LazarRuntimeError::Io(e));
                    }
                }
            }
            let status = child.wait().await.map_err(LazarRuntimeError::Io)?;
            if let Ok(err_text) = stderr_handle.await {
                stderr_buf = err_text;
            }

            if let Some(msg) = turn_error {
                return Err(LazarRuntimeError::Turn(msg));
            }
            if !status.success() && reply.is_empty() {
                return Err(LazarRuntimeError::Exit(status.code(), stderr_buf));
            }
            Ok(LazarTurnResult {
                text: reply,
                session_id,
            })
        };

        match tokio::time::timeout(turn_timeout, work).await {
            Ok(res) => res,
            Err(_) => Err(LazarRuntimeError::Timeout(turn_timeout)),
        }
    }
}

/// Lazar kernel home: `$LAZAR_HOME` or `~/lazar`.
pub fn lazar_home() -> PathBuf {
    if let Some(home) = std::env::var_os("LAZAR_HOME") {
        return PathBuf::from(home);
    }
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("lazar")
}

/// Kernel-reported active model: `$LAZAR_MODEL`, then
/// `$LAZAR_HOME/memory/model.txt`. `None` when neither is set (the kernel's
/// built-in default applies). `#` annotations are stripped (Go TUI parity).
pub fn discover_active_model() -> Option<String> {
    if let Ok(raw) = std::env::var("LAZAR_MODEL") {
        let m = clean_model_id(&raw);
        if !m.is_empty() {
            return Some(m);
        }
    }
    let raw = std::fs::read_to_string(lazar_home().join("memory").join("model.txt")).ok()?;
    let m = clean_model_id(raw.lines().next().unwrap_or(""));
    (!m.is_empty()).then_some(m)
}

/// Strip `#` price/ctx annotations and surrounding whitespace from a model id.
fn clean_model_id(s: &str) -> String {
    s.split('#').next().unwrap_or("").trim().to_string()
}

/// Session id charset per kernel constraint: a-z A-Z 0-9 - _ . (max 64).
fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("agent-tui-{}-{nanos}", std::process::id())
}

/// Sanitize an arbitrary string so it satisfies Lazar kernel session ID rules:
/// a-z A-Z 0-9 - _ . (max 64, no leading '.', no '..').
pub fn sanitize_session_id(id: &str) -> String {
    let mut safe: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .collect();
    safe = safe.replace("..", "_");
    while safe.starts_with('.') || safe.starts_with('_') {
        safe.remove(0);
    }
    if safe.is_empty() {
        return new_session_id();
    }
    if safe.len() > 64 {
        safe.truncate(64);
    }
    safe
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_model_id_strips_annotations() {
        // Mirrors the Go TUI's parseModelLine/canonicalModelID contract.
        assert_eq!(
            clean_model_id("anthropic/claude-haiku-4.5     # $1.00  / $5.00   200K ctx, vision"),
            "anthropic/claude-haiku-4.5"
        );
        assert_eq!(
            clean_model_id("google/gemini-2.5-pro#cheap"),
            "google/gemini-2.5-pro"
        );
        assert_eq!(clean_model_id("  openai/gpt-5  "), "openai/gpt-5");
        assert_eq!(clean_model_id(""), "");
    }

    #[test]
    fn session_id_charset() {
        let id = new_session_id();
        assert!(
            id.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        );
        assert!(id.len() <= 64);
    }

    #[test]
    fn sanitize_session_id_enforces_kernel_rules() {
        assert_eq!(sanitize_session_id("../etc/passwd"), "etc_passwd");
        assert_eq!(sanitize_session_id(".dotfile"), "dotfile");
        assert_eq!(
            sanitize_session_id("long-session-id-".repeat(10).as_str()).len(),
            64
        );
        assert_eq!(sanitize_session_id("sess@123!#$"), "sess_123___");
    }

    /// Real-CLI integration: requires `lazar` on PATH (or `$LAZAR_HOME/bin`)
    /// plus provider env. Run with:
    /// `LAZAR_INTEGRATION=1 cargo test -p agent-tui-lazar-runtime -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn real_turn_streams_text() {
        if std::env::var("LAZAR_INTEGRATION").ok().as_deref() != Some("1") {
            return;
        }
        let bin = std::env::var("LAZAR_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("lazar"));
        let pool = LazarRuntimePool::new(PoolConfig {
            lazar_bin: bin,
            cwd: Some(lazar_home()),
            ..Default::default()
        });
        let res = pool
            .start_text_turn("Reply with the single word: ok", None)
            .await
            .expect("turn");
        assert!(!res.text.trim().is_empty());
    }
}
