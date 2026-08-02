//! Spawn-per-turn client for the **Hermes Agent** CLI.
//!
//! ```text
//! hermes chat -q <prompt> -Q [--resume <session>] [-m <model>]
//! ```
//!
//! Quiet mode (`-Q`): final reply on **stdout**, `session_id:` on **stderr**
//! (Hermes keeps piped stdout machine-clean for automation wrappers).
//! Continuity is Hermes `--resume` / sticky session id (same shape as Claude).
//!
//! Sticky ids are keyed (e.g. by Agent TUI session id) so multi-agent does not
//! share one Hermes conversation or poison siblings with a bad resume id.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(600);
const DEFAULT_STICKY_KEY: &str = "default";
/// Hermes exposes only `-q <query>` for non-interactive turns. Bound that argv
/// value so Windows and Unix both fail predictably before process creation.
pub const MAX_ARG_PROMPT_BYTES: usize = 10_000;

/// Permission contract inherited from the Agent TUI session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionMode {
    #[default]
    Ask,
    Auto,
    AlwaysApprove,
}

#[derive(Debug)]
pub enum HermesRuntimeError {
    Spawn(std::io::Error),
    Io(std::io::Error),
    Turn(String),
    Timeout(Duration),
    Exit { code: Option<i32>, stderr: String },
    PromptTooLarge { actual: usize, max: usize },
}

impl fmt::Display for HermesRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "failed to spawn hermes: {e}"),
            Self::Io(e) => write!(f, "hermes I/O: {e}"),
            Self::Turn(msg) => write!(f, "{msg}"),
            Self::Timeout(d) => write!(f, "hermes turn timed out after {}s", d.as_secs()),
            Self::Exit { code, stderr } => {
                let code = code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "unknown".into());
                let detail = stderr.trim();
                if detail.is_empty() {
                    write!(f, "hermes exited with code {code}")
                } else {
                    // Prefer a short single-line detail for the TUI.
                    let first = detail.lines().next().unwrap_or(detail);
                    write!(f, "hermes exited with code {code}: {first}")
                }
            }
            Self::PromptTooLarge { actual, max } => {
                write!(f, "hermes prompt is {actual} bytes; maximum is {max}")
            }
        }
    }
}

impl std::error::Error for HermesRuntimeError {}

pub type Result<T> = std::result::Result<T, HermesRuntimeError>;

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub hermes_bin: PathBuf,
    pub cwd: Option<PathBuf>,
    pub turn_timeout: Duration,
    pub default_model: Option<String>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            hermes_bin: PathBuf::from("hermes"),
            cwd: None,
            turn_timeout: DEFAULT_TURN_TIMEOUT,
            default_model: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HermesTurnResult {
    pub text: String,
    pub session_id: String,
}

#[derive(Default)]
struct PoolState {
    /// sticky_key → Hermes session id for `--resume`.
    sessions: HashMap<String, String>,
}

/// Sticky Hermes session via `--resume`; process is spawn-per-turn.
///
/// Keys isolate multi-agent: pass the Agent TUI session id so concurrent agents
/// do not share or clobber one Hermes conversation.
pub struct HermesRuntimePool {
    config: PoolConfig,
    state: Mutex<PoolState>,
}

impl HermesRuntimePool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            state: Mutex::new(PoolState::default()),
        }
    }

    /// Drop all sticky Hermes sessions (e.g. after model change).
    pub async fn reset_session(&self) {
        self.state.lock().await.sessions.clear();
    }

    /// Drop sticky for one key (Agent TUI session id).
    pub async fn reset_session_key(&self, key: &str) {
        self.state.lock().await.sessions.remove(key);
    }

    pub async fn set_session(&self, session_id: impl Into<String>) {
        self.set_session_for(DEFAULT_STICKY_KEY, session_id).await;
    }

    pub async fn set_session_for(&self, key: &str, session_id: impl Into<String>) {
        let sid = session_id.into();
        if sid.is_empty() || is_placeholder_session(&sid) {
            return;
        }
        self.state
            .lock()
            .await
            .sessions
            .insert(key.to_string(), sid);
    }

    pub async fn session_id(&self) -> Option<String> {
        self.session_id_for(DEFAULT_STICKY_KEY).await
    }

    pub async fn session_id_for(&self, key: &str) -> Option<String> {
        self.state.lock().await.sessions.get(key).cloned()
    }

    /// Run one quiet turn. `sticky_key` isolates multi-agent resume state
    /// (use the Agent TUI session id). `None` uses the shared default slot.
    pub async fn start_text_turn(
        &self,
        prompt: &str,
        model: Option<&str>,
    ) -> Result<HermesTurnResult> {
        self.start_text_turn_keyed(prompt, model, None).await
    }

    pub async fn start_text_turn_keyed(
        &self,
        prompt: &str,
        model: Option<&str>,
        sticky_key: Option<&str>,
    ) -> Result<HermesTurnResult> {
        self.start_text_turn_keyed_with_permission(prompt, model, sticky_key, PermissionMode::Ask)
            .await
    }

    pub async fn start_text_turn_keyed_with_permission(
        &self,
        prompt: &str,
        model: Option<&str>,
        sticky_key: Option<&str>,
        permission_mode: PermissionMode,
    ) -> Result<HermesTurnResult> {
        if prompt.len() > MAX_ARG_PROMPT_BYTES {
            return Err(HermesRuntimeError::PromptTooLarge {
                actual: prompt.len(),
                max: MAX_ARG_PROMPT_BYTES,
            });
        }
        let key = sticky_key
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_STICKY_KEY)
            .to_string();

        let sticky = {
            let state = self.state.lock().await;
            state
                .sessions
                .get(&key)
                .cloned()
                .filter(|s| !s.is_empty() && !is_placeholder_session(s))
        };

        let model = model
            .map(str::to_string)
            .or_else(|| self.config.default_model.clone())
            .or_else(discover_active_model)
            .map(|m| normalize_model_id(&m));

        // First attempt (with sticky resume if any). On "Session not found",
        // clear that key and retry once fresh — multi-agent / stale sticky.
        match self
            .run_one_turn(prompt, model.as_deref(), sticky.as_deref(), permission_mode)
            .await
        {
            Ok(result) => {
                if !result.session_id.is_empty() && !is_placeholder_session(&result.session_id) {
                    let mut state = self.state.lock().await;
                    state.sessions.insert(key, result.session_id.clone());
                }
                Ok(result)
            }
            Err(e) if sticky.is_some() && is_session_not_found(&e) => {
                {
                    let mut state = self.state.lock().await;
                    state.sessions.remove(&key);
                }
                let result = self
                    .run_one_turn(prompt, model.as_deref(), None, permission_mode)
                    .await?;
                if !result.session_id.is_empty() && !is_placeholder_session(&result.session_id) {
                    let mut state = self.state.lock().await;
                    state.sessions.insert(key, result.session_id.clone());
                }
                Ok(result)
            }
            Err(e) => Err(e),
        }
    }

    async fn run_one_turn(
        &self,
        prompt: &str,
        model: Option<&str>,
        resume: Option<&str>,
        permission_mode: PermissionMode,
    ) -> Result<HermesTurnResult> {
        let mut cmd = Command::new(&self.config.hermes_bin);
        cmd.arg("chat").arg("-q").arg(prompt).arg("-Q"); // quiet: final response on stdout; session_id on stderr
        if let Some(m) = model {
            if !m.is_empty() {
                cmd.arg("-m").arg(m);
            }
        }
        if let Some(sid) = resume {
            cmd.arg("--resume").arg(sid);
        }
        if let Some(cwd) = &self.config.cwd {
            cmd.current_dir(cwd);
        }
        // Never bypass Hermes approvals unless the user explicitly selected
        // Agent TUI's Always Approve mode for this turn.
        if permission_mode == PermissionMode::AlwaysApprove {
            cmd.arg("--yolo");
            cmd.env("HERMES_ACCEPT_HOOKS", "1");
            cmd.arg("--accept-hooks");
        }
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .kill_on_drop(true);

        let turn_timeout = self.config.turn_timeout;
        let sticky_for_result = resume.map(str::to_string);

        let work = async move {
            let mut child = cmd.spawn().map_err(HermesRuntimeError::Spawn)?;
            let stdout = child.stdout.take().expect("stdout piped");
            let stderr = child.stderr.take().expect("stderr piped");

            // Hermes puts `session_id:` on stderr so piped stdout stays clean.
            let out_fut = async {
                let mut lines = BufReader::new(stdout).lines();
                let mut body = String::new();
                let mut session: Option<String> = None;
                while let Some(line) = lines.next_line().await.map_err(HermesRuntimeError::Io)? {
                    if let Some(sid) = parse_session_id_line(&line) {
                        session = Some(sid);
                        continue;
                    }
                    if should_skip_stdout_line(&line) {
                        if body.is_empty() && line.starts_with("API call failed") {
                            body.push_str(&line);
                        }
                        continue;
                    }
                    if !body.is_empty() {
                        body.push('\n');
                    }
                    body.push_str(&line);
                }
                Ok::<_, HermesRuntimeError>((body, session))
            };
            let err_fut = async {
                let mut buf = Vec::new();
                let mut stderr = stderr;
                stderr
                    .read_to_end(&mut buf)
                    .await
                    .map_err(HermesRuntimeError::Io)?;
                Ok::<_, HermesRuntimeError>(String::from_utf8_lossy(&buf).into_owned())
            };

            let ((body, stdout_session), stderr_text) = match tokio::try_join!(out_fut, err_fut) {
                Ok(v) => v,
                Err(e) => {
                    let _ = child.kill().await;
                    return Err(e);
                }
            };

            let status = child.wait().await.map_err(HermesRuntimeError::Io)?;
            let stderr_session = extract_session_id_from_text(&stderr_text);
            let reported_session = stdout_session.or(stderr_session);
            let text = body.trim().to_string();
            let stderr_trim = stderr_text.trim().to_string();

            if !status.success() {
                // Always treat non-zero as failure. Hermes may print the error
                // on stdout (HTTP 400) or stderr (Session not found).
                let detail = first_useful_error_line(&stderr_trim)
                    .or_else(|| first_useful_error_line(&text))
                    .unwrap_or("")
                    .to_string();
                let combined = if detail.is_empty() {
                    if !stderr_trim.is_empty() {
                        stderr_trim
                    } else {
                        text.clone()
                    }
                } else {
                    detail
                };
                if text.starts_with("API call failed") || text.starts_with("HTTP ") {
                    return Err(HermesRuntimeError::Turn(if text.is_empty() {
                        combined
                    } else {
                        text
                    }));
                }
                return Err(HermesRuntimeError::Exit {
                    code: status.code(),
                    stderr: combined,
                });
            }

            if text.starts_with("API call failed") || text.starts_with("HTTP ") {
                return Err(HermesRuntimeError::Turn(text));
            }

            let session_id = reported_session
                .or(sticky_for_result)
                .filter(|s| !s.is_empty() && !is_placeholder_session(s))
                .unwrap_or_default();

            Ok(HermesTurnResult { text, session_id })
        };

        match tokio::time::timeout(turn_timeout, work).await {
            Ok(res) => res,
            Err(_) => Err(HermesRuntimeError::Timeout(turn_timeout)),
        }
    }
}

/// Hermes home: `$HERMES_HOME` or `~/.hermes`.
pub fn hermes_home() -> PathBuf {
    if let Some(home) = std::env::var_os("HERMES_HOME") {
        return PathBuf::from(home);
    }
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".hermes")
}

/// Active model from `$HERMES_MODEL` or `~/.hermes/config.yaml` `model.default`.
pub fn discover_active_model() -> Option<String> {
    if let Ok(m) = std::env::var("HERMES_MODEL") {
        let m = normalize_model_id(m.trim());
        if !m.is_empty() {
            return Some(m);
        }
    }
    let raw = std::fs::read_to_string(hermes_home().join("config.yaml")).ok()?;
    discover_active_model_from_yaml(&raw)
}

fn discover_active_model_from_yaml(raw: &str) -> Option<String> {
    // Tiny YAML scrape — avoid pulling a full yaml dep for one key.
    let mut in_model = false;
    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        if t == "model:" {
            in_model = true;
            continue;
        }
        if in_model {
            if !line.starts_with(' ')
                && !line.starts_with('\t')
                && t.contains(':')
                && !t.starts_with("default")
            {
                in_model = false;
            }
            if let Some(rest) = t.strip_prefix("default:") {
                let v = normalize_model_id(rest.trim().trim_matches('"').trim_matches('\''));
                if !v.is_empty() {
                    return Some(v);
                }
            }
            if !line.starts_with(' ') && !line.starts_with('\t') && !t.is_empty() {
                in_model = false;
            }
        }
    }
    None
}

/// Strip UI labels accidentally used as model ids (`Hermes — gpt-5.6-sol` → `gpt-5.6-sol`).
pub fn normalize_model_id(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    // Display forms from model_state_from_hermes_active / toasts.
    for prefix in ["Hermes — ", "Hermes - ", "Hermes – ", "Hermes: ", "Hermes "] {
        if let Some(rest) = t.strip_prefix(prefix) {
            let rest = rest.trim();
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    // Bare product label is not a model id — drop so Hermes uses config default.
    if t.eq_ignore_ascii_case("hermes") {
        return String::new();
    }
    t.to_string()
}

fn parse_session_id_line(line: &str) -> Option<String> {
    let t = line.trim();
    let rest = t.strip_prefix("session_id:")?;
    let sid = rest.trim().to_string();
    if sid.is_empty() || is_placeholder_session(&sid) {
        None
    } else {
        Some(sid)
    }
}

fn extract_session_id_from_text(text: &str) -> Option<String> {
    for line in text.lines().rev() {
        if let Some(sid) = parse_session_id_line(line) {
            return Some(sid);
        }
    }
    None
}

fn should_skip_stdout_line(line: &str) -> bool {
    line.starts_with("Warning:") || line.starts_with("API call failed")
}

fn first_useful_error_line(text: &str) -> Option<&str> {
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("session_id:") {
            continue;
        }
        if t.starts_with("Warning:") {
            continue;
        }
        // Resume banner noise: "↻ Resumed session …"
        if t.contains("Resumed session") {
            continue;
        }
        return Some(t);
    }
    None
}

fn is_placeholder_session(sid: &str) -> bool {
    sid.starts_with("agent-tui-hermes-")
}

fn is_session_not_found(err: &HermesRuntimeError) -> bool {
    let msg = err.to_string();
    msg.contains("Session not found")
}

/// True when `hermes` is on PATH or `$HERMES_HOME/bin/hermes` exists.
pub fn hermes_available() -> bool {
    hermes_bin_path().is_some()
}

fn which_bin(name: &str) -> bool {
    which::which(name).is_ok()
}

pub fn hermes_bin_path() -> Option<PathBuf> {
    if which_bin("hermes") {
        return Some(PathBuf::from("hermes"));
    }
    let home_bin = hermes_home().join("bin/hermes");
    if home_bin.is_file() {
        return Some(home_bin);
    }
    let local =
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/bin/hermes");
    local.is_file().then_some(local)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_parses_default_from_yaml_snippet() {
        let yaml = r#"
provider: openai
model:
  default: "openai/gpt-5.4"
  fallback: anthropic/claude-sonnet-4-6
logging:
  level: info
"#;
        assert_eq!(
            discover_active_model_from_yaml(yaml).as_deref(),
            Some("openai/gpt-5.4")
        );
        assert_eq!(discover_active_model_from_yaml("model: nope\n"), None);
    }

    #[test]
    fn parse_session_id_from_stderr_blob() {
        let blob = "\n↻ Resumed session abc\n\nsession_id: 20260720_105700_94fd42\n";
        assert_eq!(
            extract_session_id_from_text(blob).as_deref(),
            Some("20260720_105700_94fd42")
        );
    }

    #[test]
    fn useful_error_skips_session_and_resume_banner() {
        let blob = "↻ Resumed session xyz\nSession not found: agent-tui-hermes-1\nUse a session ID\n\nsession_id: nope\n";
        assert_eq!(
            first_useful_error_line(blob),
            Some("Session not found: agent-tui-hermes-1")
        );
    }

    #[test]
    fn placeholder_session_ids_rejected() {
        assert!(is_placeholder_session("agent-tui-hermes-123-456"));
        assert!(!is_placeholder_session("20260720_105700_94fd42"));
    }

    #[test]
    fn normalize_strips_hermes_display_prefix() {
        assert_eq!(normalize_model_id("Hermes — gpt-5.6-sol"), "gpt-5.6-sol");
        assert_eq!(normalize_model_id("Hermes - gpt-5.6-sol"), "gpt-5.6-sol");
        assert_eq!(normalize_model_id("Hermes"), "");
        assert_eq!(normalize_model_id("gpt-5.6-sol"), "gpt-5.6-sol");
    }

    #[test]
    fn session_not_found_detection() {
        let e = HermesRuntimeError::Exit {
            code: Some(1),
            stderr: "Session not found: agent-tui-hermes-x".into(),
        };
        assert!(is_session_not_found(&e));
        let e2 = HermesRuntimeError::Exit {
            code: Some(1),
            stderr: "HTTP 400: bad".into(),
        };
        assert!(!is_session_not_found(&e2));
    }

    #[tokio::test]
    async fn oversized_argv_prompt_fails_before_spawn() {
        let pool = HermesRuntimePool::new(PoolConfig {
            hermes_bin: PathBuf::from("definitely-not-a-real-hermes"),
            ..Default::default()
        });
        let prompt = "x".repeat(MAX_ARG_PROMPT_BYTES + 1);
        let err = pool
            .start_text_turn_keyed_with_permission(&prompt, None, Some("test"), PermissionMode::Ask)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            HermesRuntimeError::PromptTooLarge {
                actual,
                max: MAX_ARG_PROMPT_BYTES
            } if actual == MAX_ARG_PROMPT_BYTES + 1
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn fake_cli_proves_keyed_resume_and_permission_flags() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("fake-hermes");
        let args_log = temp.path().join("args.log");
        std::fs::write(
            &bin,
            r#"#!/bin/sh
printf 'CALL\n' >> "$HERMES_TEST_ARGS"
printf '<%s>\n' "$@" >> "$HERMES_TEST_ARGS"
printf '%s\n' 'ok'
printf '%s\n' 'session_id: fake-session' >&2
"#,
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&bin, permissions).unwrap();
        unsafe {
            std::env::set_var("HERMES_TEST_ARGS", &args_log);
        }

        let pool = HermesRuntimePool::new(PoolConfig {
            hermes_bin: bin,
            turn_timeout: Duration::from_secs(5),
            ..Default::default()
        });
        pool.start_text_turn_keyed_with_permission(
            "one",
            None,
            Some("agent-a"),
            PermissionMode::Ask,
        )
        .await
        .unwrap();
        pool.start_text_turn_keyed_with_permission(
            "two",
            None,
            Some("agent-a"),
            PermissionMode::Ask,
        )
        .await
        .unwrap();
        pool.start_text_turn_keyed_with_permission(
            "three",
            None,
            Some("agent-b"),
            PermissionMode::AlwaysApprove,
        )
        .await
        .unwrap();

        let args = std::fs::read_to_string(args_log).unwrap();
        let calls: Vec<&str> = args.split("CALL\n").filter(|s| !s.is_empty()).collect();
        assert_eq!(calls.len(), 3);
        assert!(!calls[0].contains("--resume"));
        assert!(calls[1].contains("--resume") && calls[1].contains("fake-session"));
        assert!(!calls[2].contains("--resume"));
        assert!(!calls[0].contains("--yolo") && !calls[0].contains("--accept-hooks"));
        assert!(calls[2].contains("--yolo") && calls[2].contains("--accept-hooks"));
        unsafe {
            std::env::remove_var("HERMES_TEST_ARGS");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn real_turn_streams_text() {
        if std::env::var("HERMES_INTEGRATION").ok().as_deref() != Some("1") {
            return;
        }
        let bin = hermes_bin_path().unwrap_or_else(|| PathBuf::from("hermes"));
        let pool = HermesRuntimePool::new(PoolConfig {
            hermes_bin: bin,
            cwd: Some(hermes_home()),
            ..Default::default()
        });
        let res = pool
            .start_text_turn("Reply with the single word: ok", None)
            .await
            .expect("turn");
        assert!(!res.text.trim().is_empty());
        assert!(
            !res.session_id.is_empty(),
            "quiet mode must report session_id on stderr"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn real_multi_turn_resume() {
        if std::env::var("HERMES_INTEGRATION").ok().as_deref() != Some("1") {
            return;
        }
        let bin = hermes_bin_path().unwrap_or_else(|| PathBuf::from("hermes"));
        let pool = HermesRuntimePool::new(PoolConfig {
            hermes_bin: bin,
            cwd: Some(hermes_home()),
            ..Default::default()
        });
        let t1 = pool
            .start_text_turn_keyed("Reply with exactly: alpha", None, Some("agent-a"))
            .await
            .expect("turn1");
        assert!(!t1.session_id.is_empty());
        let sticky = pool.session_id_for("agent-a").await;
        assert_eq!(sticky.as_deref(), Some(t1.session_id.as_str()));
        // Second agent must not inherit agent-a's sticky.
        assert!(pool.session_id_for("agent-b").await.is_none());
        let t2 = pool
            .start_text_turn_keyed("Reply with exactly: beta", None, Some("agent-a"))
            .await
            .expect("turn2");
        assert!(!t2.text.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn real_stale_sticky_retries_fresh() {
        if std::env::var("HERMES_INTEGRATION").ok().as_deref() != Some("1") {
            return;
        }
        let bin = hermes_bin_path().unwrap_or_else(|| PathBuf::from("hermes"));
        let pool = HermesRuntimePool::new(PoolConfig {
            hermes_bin: bin,
            cwd: Some(hermes_home()),
            ..Default::default()
        });
        pool.set_session_for("agent-x", "agent-tui-hermes-stale-id")
            .await;
        // Placeholder set_session_for rejects agent-tui-hermes-*; force-insert via
        // a real-looking but missing id.
        {
            let mut state = pool.state.lock().await;
            state
                .sessions
                .insert("agent-x".into(), "20200101_000000_deadbeef".into());
        }
        let res = pool
            .start_text_turn_keyed("Reply with exactly: ok", None, Some("agent-x"))
            .await
            .expect("stale sticky should retry without resume");
        assert!(!res.text.is_empty());
        assert!(!res.session_id.is_empty());
    }
}
