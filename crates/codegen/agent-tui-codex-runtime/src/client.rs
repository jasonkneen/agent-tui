//! Codex app-server JSON-RPC client over stdio.

use crate::error::{CodexRuntimeError, Result};
use crate::protocol::{
    ClientInfo, InboundMessage, InitializeCapabilities, InitializeParams, OutgoingNotification,
    OutgoingRequest, RuntimeEvent, ThreadStartParams, TurnStartParams, UserInput, map_notification,
};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, oneshot};
use tokio::time::timeout;
use tracing::{debug, warn};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// How to launch / attach to app-server.
#[derive(Debug, Clone)]
pub enum TransportConfig {
    /// Spawn `codex app-server` (stdio://) as a child process.
    SpawnStdio {
        /// Binary name or absolute path. Default: `"codex"`.
        codex_bin: PathBuf,
        /// Extra args after `app-server` (e.g. feature flags).
        extra_args: Vec<String>,
    },
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self::SpawnStdio {
            codex_bin: PathBuf::from("codex"),
            extra_args: Vec::new(),
        }
    }
}

/// Client identity presented during `initialize`.
#[derive(Debug, Clone)]
pub struct ClientIdentity {
    pub name: String,
    pub version: String,
    pub title: Option<String>,
}

impl Default for ClientIdentity {
    fn default() -> Self {
        Self {
            name: "agent_tui".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            title: Some("Agent TUI".into()),
        }
    }
}

struct Pending {
    tx: oneshot::Sender<std::result::Result<Value, CodexRuntimeError>>,
}

/// Live connection to a Codex app-server process.
pub struct CodexAppServerClient {
    identity: ClientIdentity,
    child: Mutex<Option<Child>>,
    stdin: Arc<Mutex<ChildStdin>>,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<String, Pending>>>,
    /// Broadcast of all server notifications (and unsolicited traffic).
    notify_tx: tokio::sync::broadcast::Sender<RuntimeEvent>,
    /// Reader task handle — aborted on drop/shutdown.
    reader_abort: tokio::task::AbortHandle,
    stderr_abort: tokio::task::AbortHandle,
    reader_finished: Arc<AtomicBool>,
    initialized: Mutex<bool>,
    last_used: Mutex<std::time::Instant>,
}

impl CodexAppServerClient {
    /// Spawn `codex app-server` and complete the `initialize` handshake.
    pub async fn connect(
        transport: TransportConfig,
        identity: ClientIdentity,
    ) -> Result<Arc<Self>> {
        let TransportConfig::SpawnStdio {
            codex_bin,
            extra_args,
        } = transport;

        let mut cmd = Command::new(&codex_bin);
        cmd.arg("app-server")
            .arg("--stdio")
            .args(&extra_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CodexRuntimeError::CodexNotFound
            } else {
                CodexRuntimeError::Spawn(e)
            }
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| CodexRuntimeError::Other("child stdin missing".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CodexRuntimeError::Other("child stdout missing".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CodexRuntimeError::Other("child stderr missing".into()))?;
        let stdin = Arc::new(Mutex::new(stdin));

        let pending: Arc<Mutex<HashMap<String, Pending>>> = Arc::new(Mutex::new(HashMap::new()));
        let (notify_tx, _) = tokio::sync::broadcast::channel(256);
        let pending_r = pending.clone();
        let notify_r = notify_tx.clone();
        let stdin_r = stdin.clone();
        let reader_finished = Arc::new(AtomicBool::new(false));
        let reader_finished_r = reader_finished.clone();

        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<InboundMessage>(&line) {
                            Ok(msg) => dispatch_inbound(msg, &pending_r, &notify_r, &stdin_r).await,
                            Err(e) => {
                                warn!(
                                    error = %e,
                                    line = %truncate_for_log(&line, 512),
                                    "codex app-server: bad JSON line"
                                );
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        warn!(%error, "codex app-server: stdout reader failed");
                        break;
                    }
                }
            }
            reader_finished_r.store(true, Ordering::Release);
            // Fail any waiters.
            let mut map = pending_r.lock().await;
            for (_, p) in map.drain() {
                let _ = p.tx.send(Err(CodexRuntimeError::ConnectionClosed(
                    "reader exited".into(),
                )));
            }
        });

        // app-server can be verbose on stderr. Drain it continuously so a
        // full pipe cannot deadlock the long-lived child.
        let stderr_reader = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    debug!(
                        line = %truncate_for_log(&line, 512),
                        "codex app-server stderr"
                    );
                }
            }
        });

        let client = Arc::new(Self {
            identity,
            child: Mutex::new(Some(child)),
            stdin,
            next_id: AtomicU64::new(1),
            pending,
            notify_tx,
            reader_abort: reader.abort_handle(),
            stderr_abort: stderr_reader.abort_handle(),
            reader_finished,
            initialized: Mutex::new(false),
            last_used: Mutex::new(std::time::Instant::now()),
        });

        client.initialize().await?;
        Ok(client)
    }

    pub async fn touch(&self) {
        *self.last_used.lock().await = std::time::Instant::now();
    }

    pub async fn idle_for(&self) -> Duration {
        self.last_used.lock().await.elapsed()
    }

    /// True only while both the reader and child process are still alive.
    pub async fn is_healthy(&self) -> bool {
        if self.reader_finished.load(Ordering::Acquire) {
            return false;
        }
        let mut child = self.child.lock().await;
        match child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Subscribe to server notifications / mapped runtime events.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<RuntimeEvent> {
        self.notify_tx.subscribe()
    }

    async fn initialize(&self) -> Result<()> {
        let mut guard = self.initialized.lock().await;
        if *guard {
            return Err(CodexRuntimeError::AlreadyInitialized);
        }

        let params = InitializeParams {
            client_info: ClientInfo {
                name: self.identity.name.clone(),
                version: self.identity.version.clone(),
                title: self.identity.title.clone(),
            },
            capabilities: Some(InitializeCapabilities {
                experimental_api: Some(false),
            }),
        };

        let _result = self
            .request(
                "initialize",
                Some(serde_json::to_value(params)?),
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;

        // Required follow-up notification.
        self.notify("initialized", Some(json!({}))).await?;
        *guard = true;
        self.touch().await;
        debug!("codex app-server: initialized");
        Ok(())
    }

    /// Start a new conversation thread. Returns `thread.id`.
    pub async fn thread_start(&self, params: ThreadStartParams) -> Result<String> {
        if !*self.initialized.lock().await {
            return Err(CodexRuntimeError::NotInitialized);
        }
        self.touch().await;
        let result = self
            .request(
                "thread/start",
                Some(serde_json::to_value(params)?),
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;
        let id = result
            .pointer("/thread/id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CodexRuntimeError::Other(format!("thread/start missing thread.id: {result}"))
            })?
            .to_string();
        Ok(id)
    }

    /// List available models (`model/list`), following pagination.
    pub async fn model_list(
        &self,
        include_hidden: bool,
    ) -> Result<Vec<crate::protocol::CodexModelEntry>> {
        if !*self.initialized.lock().await {
            return Err(CodexRuntimeError::NotInitialized);
        }
        self.touch().await;
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        let mut seen_cursors = HashSet::new();
        loop {
            if let Some(current) = cursor.as_ref()
                && !seen_cursors.insert(current.clone())
            {
                return Err(CodexRuntimeError::Other(format!(
                    "model/list repeated pagination cursor `{current}`"
                )));
            }
            let params = crate::protocol::ModelListParams {
                limit: Some(100),
                cursor: cursor.clone(),
                include_hidden: Some(include_hidden),
            };
            let result = self
                .request(
                    "model/list",
                    Some(serde_json::to_value(params)?),
                    DEFAULT_REQUEST_TIMEOUT,
                )
                .await?;
            let data = result
                .get("data")
                .or_else(|| result.get("models"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for item in data {
                if let Some(entry) = parse_model_entry(&item) {
                    out.push(entry);
                }
            }
            cursor = result
                .get("nextCursor")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    /// Begin a turn and return the initial `turn` object (events stream via [`Self::subscribe`]).
    pub async fn turn_start(&self, params: TurnStartParams) -> Result<Value> {
        if !*self.initialized.lock().await {
            return Err(CodexRuntimeError::NotInitialized);
        }
        self.touch().await;
        self.request(
            "turn/start",
            Some(serde_json::to_value(params)?),
            DEFAULT_REQUEST_TIMEOUT,
        )
        .await
    }

    /// Convenience: start a text turn on `thread_id`.
    pub async fn turn_start_text(
        &self,
        thread_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<Value> {
        self.turn_start(TurnStartParams {
            thread_id: thread_id.into(),
            input: vec![UserInput::Text { text: text.into() }],
            cwd: None,
        })
        .await
    }

    /// Request cancellation of the in-flight turn for a thread.
    pub async fn turn_interrupt(&self, thread_id: impl Into<String>) -> Result<Value> {
        self.touch().await;
        self.request(
            "turn/interrupt",
            Some(json!({ "threadId": thread_id.into() })),
            Duration::from_secs(30),
        )
        .await
    }

    /// Low-level JSON-RPC request with timeout.
    pub async fn request(
        &self,
        method: &str,
        params: Option<Value>,
        req_timeout: Duration,
    ) -> Result<Value> {
        let id_num = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id = Value::from(id_num);
        let id_key = id_num.to_string();

        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending.lock().await;
            map.insert(id_key.clone(), Pending { tx });
        }

        let msg = OutgoingRequest {
            id: id.clone(),
            method: method.to_string(),
            params,
        };
        if let Err(error) = self.write_line(&msg).await {
            self.pending.lock().await.remove(&id_key);
            return Err(error);
        }

        match timeout(req_timeout, rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => Err(CodexRuntimeError::ConnectionClosed(id_key)),
            Err(_) => {
                self.pending.lock().await.remove(&id_key);
                Err(CodexRuntimeError::Timeout(req_timeout))
            }
        }
    }

    async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        let msg = OutgoingNotification {
            method: method.to_string(),
            params,
        };
        self.write_line(&msg).await
    }

    async fn write_line<T: serde::Serialize>(&self, msg: &T) -> Result<()> {
        let mut line = serde_json::to_vec(msg)?;
        line.push(b'\n');
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(&line).await?;
        stdin.flush().await?;
        Ok(())
    }

    /// Gracefully stop the child process.
    pub async fn shutdown(&self) {
        self.reader_abort.abort();
        self.stderr_abort.abort();
        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        let mut map = self.pending.lock().await;
        for (_, p) in map.drain() {
            let _ =
                p.tx.send(Err(CodexRuntimeError::ConnectionClosed("shutdown".into())));
        }
    }
}

impl Drop for CodexAppServerClient {
    fn drop(&mut self) {
        self.reader_abort.abort();
        self.stderr_abort.abort();
    }
}

async fn dispatch_inbound(
    msg: InboundMessage,
    pending: &Arc<Mutex<HashMap<String, Pending>>>,
    notify_tx: &tokio::sync::broadcast::Sender<RuntimeEvent>,
    stdin: &Arc<Mutex<ChildStdin>>,
) {
    if msg.is_response() {
        let id_key = match &msg.id {
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => return,
        };
        let waiter = pending.lock().await.remove(&id_key);
        if let Some(Pending { tx }) = waiter {
            if let Some(err) = msg.error {
                let _ = tx.send(Err(CodexRuntimeError::Rpc {
                    code: err.code,
                    message: err.message,
                }));
            } else {
                let _ = tx.send(Ok(msg.result.unwrap_or(Value::Null)));
            }
        } else {
            debug!(id = %id_key, "codex app-server: response with no waiter");
        }
        return;
    }

    if let Some(method) = msg.method.as_deref() {
        // Server-initiated request (has id + method): auto-reject unknown for now.
        if let Some(id) = msg.id {
            // A valid JSON-RPC error response makes approvals/user-input fail
            // promptly. Dropping the request leaves app-server blocked forever.
            let response = json!({
                "id": id,
                "error": {
                    "code": -32000,
                    "message": format!(
                        "Agent TUI cannot satisfy server request `{method}` in this runtime mode"
                    )
                }
            });
            if let Err(error) = write_value_line(stdin, &response).await {
                warn!(%error, method, "codex app-server: failed to reject server request");
            }
            return;
        }
        let params = msg.params.unwrap_or(Value::Null);
        let event = map_notification(method, &params);
        let _ = notify_tx.send(event);
    }
}

async fn write_value_line(stdin: &Arc<Mutex<ChildStdin>>, value: &Value) -> Result<()> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    let mut writer = stdin.lock().await;
    writer.write_all(&line).await?;
    writer.flush().await?;
    Ok(())
}

fn truncate_for_log(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }
    let mut end = max;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...[truncated]", &value[..end])
}

fn parse_model_entry(item: &Value) -> Option<crate::protocol::CodexModelEntry> {
    let id = item
        .get("id")
        .or_else(|| item.get("model"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let display_name = item
        .get("displayName")
        .or_else(|| item.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or(&id)
        .to_string();
    let description = item
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let is_default = item
        .get("isDefault")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let hidden = item
        .get("hidden")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let default_reasoning_effort = item
        .get("defaultReasoningEffort")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let supported_reasoning_efforts = item
        .get("supportedReasoningEfforts")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    e.get("reasoningEffort")
                        .or_else(|| e.get("id"))
                        .and_then(|v| v.as_str())
                        .or_else(|| e.as_str())
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let input_modalities = item
        .get("inputModalities")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec!["text".into(), "image".into()]);
    let context_window = item
        .get("contextWindow")
        .or_else(|| item.get("context_window"))
        .or_else(|| item.get("totalContextTokens"))
        .and_then(|v| v.as_u64())
        .or_else(|| {
            item.get("meta")
                .and_then(|m| m.get("totalContextTokens").or_else(|| m.get("contextWindow")))
                .and_then(|v| v.as_u64())
        })
        .filter(|&t| t > 0);
    Some(crate::protocol::CodexModelEntry {
        id,
        display_name,
        description,
        is_default,
        hidden,
        default_reasoning_effort,
        supported_reasoning_efforts,
        input_modalities,
        context_window,
    })
}

/// Collect text deltas from a subscription until `TurnCompleted` or channel lag/close.
pub async fn collect_turn_text(
    rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>,
    max_wait: Duration,
) -> Result<String> {
    let mut out = String::new();
    let deadline = tokio::time::Instant::now() + max_wait;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return Err(CodexRuntimeError::Timeout(max_wait));
        }
        match timeout(left, rx.recv()).await {
            Ok(Ok(RuntimeEvent::TextDelta { text, .. })) => out.push_str(&text),
            Ok(Ok(RuntimeEvent::TurnCompleted { .. })) => return Ok(out),
            Ok(Ok(RuntimeEvent::Error { message, .. })) => {
                return Err(CodexRuntimeError::Other(message));
            }
            Ok(Ok(_)) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(dropped))) => {
                return Err(CodexRuntimeError::Other(format!(
                    "codex notification stream lagged; dropped {dropped} events"
                )));
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                return Err(CodexRuntimeError::ConnectionClosed("notify".into()));
            }
            Err(_) => return Err(CodexRuntimeError::Timeout(max_wait)),
        }
    }
}

/// Collect only notifications belonging to one exact thread/turn pair.
pub async fn collect_turn_text_for(
    rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>,
    thread_id: &str,
    turn_id: &str,
    max_wait: Duration,
) -> Result<String> {
    let mut out = String::new();
    let deadline = tokio::time::Instant::now() + max_wait;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return Err(CodexRuntimeError::Timeout(max_wait));
        }
        match timeout(left, rx.recv()).await {
            Ok(Ok(RuntimeEvent::TextDelta {
                thread_id: event_thread,
                turn_id: event_turn,
                text,
            })) if event_thread.as_deref() == Some(thread_id)
                && event_turn.as_deref() == Some(turn_id) =>
            {
                out.push_str(&text);
            }
            Ok(Ok(RuntimeEvent::TurnCompleted {
                thread_id: event_thread,
                turn_id: event_turn,
                ..
            })) if event_thread.as_deref() == Some(thread_id)
                && event_turn.as_deref() == Some(turn_id) =>
            {
                return Ok(out);
            }
            Ok(Ok(RuntimeEvent::Error {
                thread_id: event_thread,
                turn_id: event_turn,
                message,
            })) if (event_thread.is_none() && event_turn.is_none())
                || (event_thread.as_deref() == Some(thread_id)
                    && event_turn.as_deref() == Some(turn_id)) =>
            {
                return Err(CodexRuntimeError::Other(message));
            }
            Ok(Ok(_)) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(dropped))) => {
                return Err(CodexRuntimeError::Other(format!(
                    "codex notification stream lagged; dropped {dropped} events"
                )));
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                return Err(CodexRuntimeError::ConnectionClosed("notify".into()));
            }
            Err(_) => return Err(CodexRuntimeError::Timeout(max_wait)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn fake_app_server(temp: &tempfile::TempDir) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let bin = temp.path().join("fake-codex");
        std::fs::write(
            &bin,
            r#"#!/usr/bin/env python3
import json
import pathlib
import sys

mode = sys.argv[-2] if len(sys.argv) >= 2 else "normal"
record = pathlib.Path(sys.argv[-1]) if mode == "request" else None

for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        print(json.dumps({"id": msg["id"], "result": {}}), flush=True)
    elif method == "initialized":
        if mode == "exit":
            break
        if mode == "request":
            print(json.dumps({
                "id": 99,
                "method": "item/commandExecution/requestApproval",
                "params": {"threadId": "thread-a", "turnId": "turn-a"}
            }), flush=True)
    elif method == "model/list":
        print(json.dumps({
            "id": msg["id"],
            "result": {"data": [], "nextCursor": "same-cursor"}
        }), flush=True)
    elif msg.get("id") == 99 and "error" in msg:
        if record is not None:
            record.write_text(json.dumps(msg))
"#,
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&bin, permissions).unwrap();
        bin
    }

    #[cfg(unix)]
    async fn connect_fake(
        bin: PathBuf,
        mode: &str,
        extra: Option<&std::path::Path>,
    ) -> Arc<CodexAppServerClient> {
        let mut extra_args = vec![mode.to_string()];
        if let Some(path) = extra {
            extra_args.push(path.display().to_string());
        } else {
            extra_args.push("unused".into());
        }
        CodexAppServerClient::connect(
            TransportConfig::SpawnStdio {
                codex_bin: bin,
                extra_args,
            },
            ClientIdentity::default(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn correlated_collector_ignores_other_turns() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(16);
        tx.send(RuntimeEvent::TextDelta {
            thread_id: Some("other-thread".into()),
            turn_id: Some("other-turn".into()),
            text: "wrong".into(),
        })
        .unwrap();
        tx.send(RuntimeEvent::TextDelta {
            thread_id: Some("thread-a".into()),
            turn_id: Some("turn-a".into()),
            text: "right".into(),
        })
        .unwrap();
        tx.send(RuntimeEvent::TurnCompleted {
            thread_id: Some("other-thread".into()),
            turn_id: Some("other-turn".into()),
            status: Some("completed".into()),
        })
        .unwrap();
        tx.send(RuntimeEvent::TurnCompleted {
            thread_id: Some("thread-a".into()),
            turn_id: Some("turn-a".into()),
            status: Some("completed".into()),
        })
        .unwrap();

        let text = collect_turn_text_for(&mut rx, "thread-a", "turn-a", Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(text, "right");
    }

    #[tokio::test]
    async fn collector_fails_on_broadcast_lag() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(1);
        tx.send(RuntimeEvent::Notification {
            method: "one".into(),
            params: Value::Null,
        })
        .unwrap();
        tx.send(RuntimeEvent::Notification {
            method: "two".into(),
            params: Value::Null,
        })
        .unwrap();
        let err = collect_turn_text_for(&mut rx, "thread-a", "turn-a", Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("lagged"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn repeated_model_cursor_is_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let client = connect_fake(fake_app_server(&temp), "repeat", None).await;
        let err = client.model_list(false).await.unwrap_err();
        assert!(err.to_string().contains("repeated pagination cursor"));
        client.shutdown().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn server_requests_receive_explicit_error_response() {
        let temp = tempfile::tempdir().unwrap();
        let record = temp.path().join("response.json");
        let client = connect_fake(fake_app_server(&temp), "request", Some(&record)).await;

        for _ in 0..50 {
            if record.is_file() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let response: Value = serde_json::from_str(
            &std::fs::read_to_string(&record).expect("server request response"),
        )
        .unwrap();
        assert_eq!(response["id"], 99);
        assert_eq!(response["error"]["code"], -32000);
        client.shutdown().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dead_child_is_unhealthy_and_write_failure_leaks_no_pending_request() {
        let temp = tempfile::tempdir().unwrap();
        let client = connect_fake(fake_app_server(&temp), "exit", None).await;
        for _ in 0..50 {
            if !client.is_healthy().await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!client.is_healthy().await);
        let _ = client
            .request("model/list", None, Duration::from_millis(100))
            .await;
        assert!(client.pending.lock().await.is_empty());
        client.shutdown().await;
    }
}
