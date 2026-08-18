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
/// Lazar currently accepts its prompt only as an argv value. Keep it below
/// the Windows command-line ceiling and reject larger direct API calls rather
/// than letting spawn fail with an opaque OS error.
pub const MAX_ARG_PROMPT_BYTES: usize = 10_000;

/// Permission contract inherited from the Agent TUI session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionMode {
    #[default]
    Ask,
    Auto,
    AlwaysApprove,
}

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
    /// Prompt cannot be represented safely in the vendor CLI argv contract.
    PromptTooLarge { actual: usize, max: usize },
}

impl fmt::Display for LazarRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "failed to spawn lazar: {e}"),
            Self::Io(e) => write!(f, "lazar stream I/O: {e}"),
            Self::Turn(msg) => write!(f, "{msg}"),
            Self::Timeout(d) => write!(f, "lazar turn timed out after {}s", d.as_secs()),
            Self::Exit(code, stderr) => {
                let code_str = code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "unknown".into());
                let trimmed = stderr.trim();
                if trimmed.is_empty() {
                    write!(f, "lazar exited with code {code_str}")
                } else {
                    write!(f, "lazar exited with code {code_str}: {trimmed}")
                }
            }
            Self::PromptTooLarge { actual, max } => {
                write!(f, "lazar prompt is {actual} bytes; maximum is {max}")
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
    pub async fn start_text_turn(
        &self,
        prompt: &str,
        model: Option<&str>,
    ) -> Result<LazarTurnResult> {
        self.start_text_turn_keyed(prompt, model, None).await
    }

    /// Run one text turn with an optional sticky key (e.g. Agent TUI session id).
    pub async fn start_text_turn_keyed(
        &self,
        prompt: &str,
        model: Option<&str>,
        sticky_key: Option<&str>,
    ) -> Result<LazarTurnResult> {
        self.start_text_turn_keyed_with_permission(prompt, model, sticky_key, PermissionMode::Ask)
            .await
    }

    pub async fn start_text_turn_keyed_with_permission(
        &self,
        prompt: &str,
        model: Option<&str>,
        sticky_key: Option<&str>,
        permission_mode: PermissionMode,
    ) -> Result<LazarTurnResult> {
        self.start_text_turn_with_delta(prompt, model, sticky_key, permission_mode, |_| {})
            .await
    }

    /// Same as [`Self::start_text_turn_keyed_with_permission`], calling
    /// `on_delta` for each non-empty `text_delta` as it arrives.
    pub async fn start_text_turn_with_delta(
        &self,
        prompt: &str,
        model: Option<&str>,
        sticky_key: Option<&str>,
        permission_mode: PermissionMode,
        mut on_delta: impl FnMut(&str) + Send,
    ) -> Result<LazarTurnResult> {
        if prompt.len() > MAX_ARG_PROMPT_BYTES {
            return Err(LazarRuntimeError::PromptTooLarge {
                actual: prompt.len(),
                max: MAX_ARG_PROMPT_BYTES,
            });
        }
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
        if permission_mode == PermissionMode::AlwaysApprove {
            cmd.arg("--no-sandbox");
        }
        if let Some(cwd) = &self.config.cwd {
            cmd.current_dir(cwd);
        }
        // Match Go lazartui: remap ANTHROPIC_* / LAZAR_BACKEND to the selected
        // model. Parent may still hold MiniMax residue from launch while
        // --model is kimi-k3 (MiniMax then 401s on X-Api-Key or keeps serving
        // MiniMax under the wrong name).
        if let Some(ref m) = model {
            if let Some(hint) = provider_key_missing_hint(m) {
                return Err(LazarRuntimeError::Turn(hint));
            }
            apply_provider_env_for_model(&mut cmd, m);
        }
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .kill_on_drop(true);

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
                        match classify_stream_json_line(&line) {
                            StreamJsonAction::TextDelta(t) if !t.is_empty() => {
                                reply.push_str(&t);
                                on_delta(&t);
                            }
                            StreamJsonAction::TextDelta(_) => {}
                            StreamJsonAction::Error(msg) => {
                                turn_error = Some(msg);
                                break;
                            }
                            StreamJsonAction::Ignore => {}
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

/// One line of kernel `stream-json` stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamJsonAction {
    Ignore,
    TextDelta(String),
    Error(String),
}

/// Classify one JSONL line from `lazar --output-format stream-json`.
pub fn classify_stream_json_line(line: &str) -> StreamJsonAction {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return StreamJsonAction::Ignore;
    };
    match v["type"].as_str() {
        Some("text_delta") => {
            StreamJsonAction::TextDelta(v["text"].as_str().unwrap_or("").to_string())
        }
        Some("error") => StreamJsonAction::Error(
            v["message"]
                .as_str()
                .unwrap_or("unknown kernel error")
                .to_string(),
        ),
        _ => StreamJsonAction::Ignore,
    }
}

/// Strip `#` price/ctx annotations and surrounding whitespace from a model id.
fn clean_model_id(s: &str) -> String {
    s.split('#').next().unwrap_or("").trim().to_string()
}

/// Map a model id to a `LAZAR_BACKEND` preset (Go TUI `backendForModel` parity).
///
/// Switching model must also switch BASE_URL + API key — MiniMax silently
/// accepts foreign names (or 401s on missing X-Api-Key) while the parent still
/// holds MiniMax credentials from launch.
pub fn backend_for_model(model: &str) -> Option<&'static str> {
    let m = model.trim().to_ascii_lowercase();
    if m.is_empty() {
        return None;
    }
    if m.contains("minimax") {
        return Some("minimax");
    }
    // provider/model form is OpenRouter's namespace
    if m.starts_with("anthropic/")
        || m.contains("openrouter")
        || m.contains("moonshotai/")
        || m.contains('/')
    {
        return Some("openrouter");
    }
    if m.starts_with("kimi") || m.starts_with("moonshot") || m.contains("for-coding") {
        return Some("kimi");
    }
    if m.starts_with("claude") {
        return Some("anthropic");
    }
    None
}

fn first_non_empty(vals: &[&str]) -> Option<String> {
    vals.iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Provider env for a backend: BASE_URL + API_KEY + AUTH_TOKEN + LAZAR_BACKEND.
/// Keys come from the process environment (caller should have sourced
/// `lazar-env.sh` so Keychain keys are already exported).
pub fn provider_env_for_backend(backend: &str) -> Vec<(String, String)> {
    let backend = backend.trim().to_ascii_lowercase();
    if backend.is_empty() {
        return Vec::new();
    }
    let (backend, base, key) = match backend.as_str() {
        "kimi" | "moonshot" | "kimi-code" | "allegro" => {
            let mut key = first_non_empty(&[
                &std::env::var("KIMI_API_KEY").unwrap_or_default(),
                &std::env::var("KIMI_CODING_API_KEY").unwrap_or_default(),
                &std::env::var("MOONSHOT_API_KEY").unwrap_or_default(),
            ])
            .unwrap_or_default();
            if let Ok(k) = std::env::var("KIMI_API_KEY") {
                if k.starts_with("sk-kimi-") {
                    key = k;
                }
            } else if let Ok(k) = std::env::var("KIMI_CODING_API_KEY") {
                if k.starts_with("sk-kimi-") {
                    key = k;
                }
            }
            let endpoint = std::env::var("LAZAR_KIMI_ENDPOINT").unwrap_or_default();
            let base = if key.starts_with("sk-kimi-") || endpoint == "coding" {
                "https://api.kimi.com/coding"
            } else {
                "https://api.moonshot.ai/anthropic"
            };
            ("kimi", base.to_string(), key)
        }
        "openrouter" | "or" => (
            "openrouter",
            "https://openrouter.ai/api".to_string(),
            std::env::var("OPENROUTER_API_KEY").unwrap_or_default(),
        ),
        "minimax" => (
            "minimax",
            "https://api.minimax.io/anthropic".to_string(),
            std::env::var("MINIMAX_API_KEY").unwrap_or_default(),
        ),
        "anthropic" | "claude" => {
            let mut key = first_non_empty(&[
                &std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
                &std::env::var("ANTHROPIC_AUTH_TOKEN").unwrap_or_default(),
            ])
            .unwrap_or_default();
            // Prefer real Anthropic key if MiniMax key was stuffed into ANTHROPIC_*.
            if let Ok(mm) = std::env::var("MINIMAX_API_KEY") {
                if !mm.is_empty() && key == mm {
                    key.clear();
                }
            }
            (
                "anthropic",
                "https://api.anthropic.com".to_string(),
                key,
            )
        }
        _ => return Vec::new(),
    };

    let mut out = vec![
        ("LAZAR_BACKEND".into(), backend.into()),
        ("ANTHROPIC_BASE_URL".into(), base),
        ("LAZAR_USE_PROXY".into(), "0".into()),
    ];
    if !key.trim().is_empty() {
        out.push(("ANTHROPIC_API_KEY".into(), key.clone()));
        out.push(("ANTHROPIC_AUTH_TOKEN".into(), key));
    }
    out
}

/// Apply model-matched provider credentials onto a spawn command.
fn apply_provider_env_for_model(cmd: &mut Command, model: &str) {
    let Some(backend) = backend_for_model(model) else {
        // Still keep LAZAR_MODEL in sync for children that read it.
        cmd.env("LAZAR_MODEL", model);
        cmd.env("ANTHROPIC_MODEL", model);
        return;
    };
    for (k, v) in provider_env_for_backend(backend) {
        cmd.env(k, v);
    }
    cmd.env("LAZAR_MODEL", model);
    cmd.env("ANTHROPIC_MODEL", model);
}

/// User-visible hint when the model maps to a backend but that provider's key
/// is not in the process env (common: MiniMax residue + kimi model → MiniMax
/// `X-Api-Key` 401).
fn provider_key_missing_hint(model: &str) -> Option<String> {
    let backend = backend_for_model(model)?;
    let env = provider_env_for_backend(backend);
    let has_key = env.iter().any(|(k, v)| {
        (k == "ANTHROPIC_API_KEY" || k == "ANTHROPIC_AUTH_TOKEN") && !v.trim().is_empty()
    });
    if has_key {
        return None;
    }
    let (var, how) = match backend {
        "kimi" => (
            "KIMI_API_KEY (or KIMI_CODING_API_KEY / MOONSHOT_API_KEY)",
            "LAZAR_BACKEND=kimi source ~/lazar/workspace/lazar-env.sh",
        ),
        "minimax" => (
            "MINIMAX_API_KEY",
            "LAZAR_BACKEND=minimax source ~/lazar/workspace/lazar-env.sh",
        ),
        "openrouter" => (
            "OPENROUTER_API_KEY",
            "LAZAR_BACKEND=openrouter source ~/lazar/workspace/lazar-env.sh",
        ),
        "anthropic" => (
            "ANTHROPIC_API_KEY",
            "LAZAR_BACKEND=anthropic source ~/lazar/workspace/lazar-env.sh",
        ),
        _ => return None,
    };
    Some(format!(
        "model `{model}` needs backend `{backend}`, but {var} is not set in this process. \
         Do not use a MiniMax key for kimi-k3 (or vice versa). \
         Fix: `{how}` then restart Agent TUI. \
         Keys can also live in macOS Keychain (service=lazar)."
    ))
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
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
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

    /// Serialize env-mutating tests (process env is global).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    fn backend_for_model_matches_go_tui() {
        assert_eq!(backend_for_model("kimi-k3[1m]"), Some("kimi"));
        assert_eq!(backend_for_model("kimi-k3"), Some("kimi"));
        assert_eq!(backend_for_model("MiniMax-M3"), Some("minimax"));
        assert_eq!(backend_for_model("MiniMax-M2.7"), Some("minimax"));
        assert_eq!(backend_for_model("minimax-m2.7"), Some("minimax"));
        assert_eq!(
            backend_for_model("anthropic/claude-haiku-4.5"),
            Some("openrouter")
        );
        assert_eq!(backend_for_model("claude-sonnet-4-6"), Some("anthropic"));
        assert_eq!(backend_for_model(""), None);
    }

    #[test]
    fn provider_env_kimi_uses_kimi_key_not_minimax_residue() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // Isolate from ambient shell credentials.
        let _g1 = EnvGuard::set("MINIMAX_API_KEY", Some("mm-secret"));
        let _g2 = EnvGuard::set("KIMI_API_KEY", Some("kimi-secret"));
        let _g3 = EnvGuard::set("KIMI_CODING_API_KEY", None);
        let _g4 = EnvGuard::set("MOONSHOT_API_KEY", None);
        let _g5 = EnvGuard::set("ANTHROPIC_API_KEY", Some("mm-secret"));
        let _g6 = EnvGuard::set("ANTHROPIC_BASE_URL", Some("https://api.minimax.io/anthropic"));
        let _g7 = EnvGuard::set("LAZAR_KIMI_ENDPOINT", None);

        let env = provider_env_for_backend("kimi");
        let map: std::collections::HashMap<_, _> = env.into_iter().collect();
        assert_eq!(map.get("LAZAR_BACKEND").map(String::as_str), Some("kimi"));
        assert_eq!(
            map.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://api.moonshot.ai/anthropic")
        );
        assert_eq!(
            map.get("ANTHROPIC_API_KEY").map(String::as_str),
            Some("kimi-secret")
        );
        assert_ne!(
            map.get("ANTHROPIC_API_KEY").map(String::as_str),
            Some("mm-secret")
        );
    }

    #[test]
    fn provider_env_sk_kimi_uses_coding_endpoint() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let _g1 = EnvGuard::set("KIMI_API_KEY", Some("sk-kimi-coding-key"));
        let _g2 = EnvGuard::set("MINIMAX_API_KEY", Some("mm-secret"));
        let _g3 = EnvGuard::set("LAZAR_KIMI_ENDPOINT", None);

        let env = provider_env_for_backend("kimi");
        let map: std::collections::HashMap<_, _> = env.into_iter().collect();
        assert_eq!(
            map.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://api.kimi.com/coding")
        );
        assert_eq!(
            map.get("ANTHROPIC_API_KEY").map(String::as_str),
            Some("sk-kimi-coding-key")
        );
    }

    /// RAII env var restore for unit tests (serial — not parallel-safe across
    /// tests that touch the same keys; these tests only touch provider keys).
    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }
    impl EnvGuard {
        fn set(key: &'static str, value: Option<&str>) -> Self {
            let prev = std::env::var(key).ok();
            match value {
                Some(v) => unsafe { std::env::set_var(key, v) },
                None => unsafe { std::env::remove_var(key) },
            }
            Self { key, prev }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[test]
    fn classify_text_delta_and_error() {
        assert_eq!(
            classify_stream_json_line(r#"{"type":"text_delta","text":"Hi"}"#),
            StreamJsonAction::TextDelta("Hi".into())
        );
        assert_eq!(
            classify_stream_json_line(r#"{"type":"error","message":"nope"}"#),
            StreamJsonAction::Error("nope".into())
        );
        assert_eq!(
            classify_stream_json_line(r#"{"type":"tool_use","name":"bash"}"#),
            StreamJsonAction::Ignore
        );
        assert_eq!(
            classify_stream_json_line("not-json"),
            StreamJsonAction::Ignore
        );
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

    #[tokio::test]
    async fn oversized_argv_prompt_fails_before_spawn() {
        let pool = LazarRuntimePool::new(PoolConfig {
            lazar_bin: PathBuf::from("definitely-not-a-real-lazar"),
            ..Default::default()
        });
        let prompt = "x".repeat(MAX_ARG_PROMPT_BYTES + 1);
        let err = pool
            .start_text_turn_keyed_with_permission(&prompt, None, Some("test"), PermissionMode::Ask)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            LazarRuntimeError::PromptTooLarge {
                actual,
                max: MAX_ARG_PROMPT_BYTES
            } if actual == MAX_ARG_PROMPT_BYTES + 1
        ));
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
