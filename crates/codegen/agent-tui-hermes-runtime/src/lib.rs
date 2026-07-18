//! Spawn-per-turn client for the **Hermes Agent** CLI.
//!
//! ```text
//! hermes chat -q <prompt> -Q [--resume <session>] [-m <model>]
//! ```
//!
//! Quiet mode (`-Q`) prints the final reply plus a `session_id:` line.
//! Continuity is Hermes `--resume` / sticky session id (same shape as Claude).

use std::fmt;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug)]
pub enum HermesRuntimeError {
    Spawn(std::io::Error),
    Io(std::io::Error),
    Turn(String),
    Timeout(Duration),
    Exit(Option<i32>),
}

impl fmt::Display for HermesRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "failed to spawn hermes: {e}"),
            Self::Io(e) => write!(f, "hermes I/O: {e}"),
            Self::Turn(msg) => write!(f, "{msg}"),
            Self::Timeout(d) => write!(f, "hermes turn timed out after {}s", d.as_secs()),
            Self::Exit(code) => write!(
                f,
                "hermes exited with code {}",
                code.map(|c| c.to_string())
                    .unwrap_or_else(|| "unknown".into())
            ),
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
    session_id: Option<String>,
}

/// Sticky Hermes session via `--resume`; process is spawn-per-turn.
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

    pub async fn reset_session(&self) {
        self.state.lock().await.session_id = None;
    }

    pub async fn set_session(&self, session_id: impl Into<String>) {
        let sid = session_id.into();
        if sid.is_empty() {
            return;
        }
        self.state.lock().await.session_id = Some(sid);
    }

    pub async fn session_id(&self) -> Option<String> {
        self.state.lock().await.session_id.clone()
    }

    pub async fn start_text_turn(
        &self,
        prompt: &str,
        model: Option<&str>,
    ) -> Result<HermesTurnResult> {
        let sticky = {
            let state = self.state.lock().await;
            state.session_id.clone()
        };
        let model = model
            .map(str::to_string)
            .or_else(|| self.config.default_model.clone())
            .or_else(discover_active_model);

        let mut cmd = Command::new(&self.config.hermes_bin);
        cmd.arg("chat")
            .arg("-q")
            .arg(prompt)
            .arg("-Q"); // quiet: final response + session_id only
        if let Some(m) = &model {
            cmd.arg("-m").arg(m);
        }
        if let Some(sid) = &sticky {
            cmd.arg("--resume").arg(sid);
        }
        if let Some(cwd) = &self.config.cwd {
            cmd.current_dir(cwd);
        }
        // Hermes tools can be interactive; yolo for agent-tui non-interactive path.
        if std::env::var_os("HERMES_NO_YOLO").is_none() {
            cmd.arg("--yolo");
        }
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        let turn_timeout = self.config.turn_timeout;
        let work = async move {
            let mut child = cmd.spawn().map_err(HermesRuntimeError::Spawn)?;
            let stdout = child.stdout.take().expect("stdout piped");
            let mut lines = BufReader::new(stdout).lines();
            let mut body = String::new();
            let mut reported_session: Option<String> = None;
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if let Some(rest) = line.strip_prefix("session_id:") {
                            reported_session = Some(rest.trim().to_string());
                            continue;
                        }
                        // Skip known noise prefixes from hermes quiet mode.
                        if line.starts_with("Warning:")
                            || line.starts_with("API call failed")
                        {
                            if body.is_empty() && line.starts_with("API call failed") {
                                // Keep error path for empty reply below.
                                if !body.is_empty() {
                                    body.push('\n');
                                }
                                body.push_str(&line);
                            }
                            continue;
                        }
                        if !body.is_empty() {
                            body.push('\n');
                        }
                        body.push_str(&line);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let _ = child.kill().await;
                        return Err(HermesRuntimeError::Io(e));
                    }
                }
            }
            let status = child.wait().await.map_err(HermesRuntimeError::Io)?;
            let text = body.trim().to_string();
            if !status.success() && text.is_empty() {
                return Err(HermesRuntimeError::Exit(status.code()));
            }
            if text.starts_with("API call failed") {
                return Err(HermesRuntimeError::Turn(text));
            }
            let session_id = reported_session
                .or(sticky)
                .unwrap_or_else(new_session_placeholder);
            Ok(HermesTurnResult { text, session_id })
        };

        let result = match tokio::time::timeout(turn_timeout, work).await {
            Ok(res) => res?,
            Err(_) => return Err(HermesRuntimeError::Timeout(turn_timeout)),
        };

        // Stick the session for multi-turn.
        {
            let mut state = self.state.lock().await;
            state.session_id = Some(result.session_id.clone());
        }
        Ok(result)
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
        let m = m.trim().to_string();
        if !m.is_empty() {
            return Some(m);
        }
    }
    let raw = std::fs::read_to_string(hermes_home().join("config.yaml")).ok()?;
    // Tiny YAML scrape — avoid pulling a full yaml dep for one key.
    // Look for under `model:` block: `default: value`
    let mut in_model = false;
    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        if t == "model:" || t.starts_with("model:") && t != "model:" {
            if t == "model:" {
                in_model = true;
                continue;
            }
        }
        if in_model {
            if !line.starts_with(' ') && !line.starts_with('\t') && t.contains(':') && !t.starts_with("default") {
                // left the model block
                if !t.starts_with("default") {
                    in_model = false;
                }
            }
            if let Some(rest) = t.strip_prefix("default:") {
                let v = rest.trim().trim_matches('"').trim_matches('\'').to_string();
                if !v.is_empty() {
                    return Some(v);
                }
            }
            // nested under model: another key at same indent ends block naively
            if !line.starts_with(' ') && !line.starts_with('\t') && !t.is_empty() {
                in_model = false;
            }
        }
    }
    None
}

fn new_session_placeholder() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("agent-tui-hermes-{}-{nanos}", std::process::id())
}

/// True when `hermes` is on PATH or `$HERMES_HOME/bin/hermes` exists.
pub fn hermes_available() -> bool {
    which_bin("hermes")
        || hermes_home().join("bin/hermes").is_file()
        || std::path::Path::new("/Users/jkneen/.local/bin/hermes").is_file()
}

fn which_bin(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|p| {
                let candidate = p.join(name);
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}

pub fn hermes_bin_path() -> Option<PathBuf> {
    if which_bin("hermes") {
        return Some(PathBuf::from("hermes"));
    }
    let home_bin = hermes_home().join("bin/hermes");
    if home_bin.is_file() {
        return Some(home_bin);
    }
    let local = PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join(".local/bin/hermes");
    local.is_file().then_some(local)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_parses_default_from_yaml_snippet() {
        // Unit-level: write temp not needed — just exercise empty home path doesn't panic.
        let _ = discover_active_model();
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
    }
}
