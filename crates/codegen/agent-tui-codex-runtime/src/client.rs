//! Codex app-server JSON-RPC client over stdio.

use crate::error::{CodexRuntimeError, Result};
use crate::protocol::{
    ClientInfo, InboundMessage, InitializeCapabilities, InitializeParams, OutgoingNotification,
    OutgoingRequest, RuntimeEvent, ThreadStartParams, TurnStartParams, UserInput, map_notification,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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
    stdin: Mutex<ChildStdin>,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<String, Pending>>>,
    /// Broadcast of all server notifications (and unsolicited traffic).
    notify_tx: tokio::sync::broadcast::Sender<RuntimeEvent>,
    /// Reader task handle — aborted on drop/shutdown.
    reader_abort: tokio::task::AbortHandle,
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

        let pending: Arc<Mutex<HashMap<String, Pending>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (notify_tx, _) = tokio::sync::broadcast::channel(256);
        let pending_r = pending.clone();
        let notify_r = notify_tx.clone();

        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<InboundMessage>(&line) {
                    Ok(msg) => dispatch_inbound(msg, &pending_r, &notify_r).await,
                    Err(e) => {
                        warn!(error = %e, line = %line, "codex app-server: bad JSON line");
                    }
                }
            }
            // Fail any waiters.
            let mut map = pending_r.lock().await;
            for (_, p) in map.drain() {
                let _ = p.tx.send(Err(CodexRuntimeError::ConnectionClosed(
                    "reader exited".into(),
                )));
            }
        });

        let client = Arc::new(Self {
            identity,
            child: Mutex::new(Some(child)),
            stdin: Mutex::new(stdin),
            next_id: AtomicU64::new(1),
            pending,
            notify_tx,
            reader_abort: reader.abort_handle(),
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
                CodexRuntimeError::Other(format!(
                    "thread/start missing thread.id: {result}"
                ))
            })?
            .to_string();
        Ok(id)
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
    pub async fn turn_start_text(&self, thread_id: impl Into<String>, text: impl Into<String>) -> Result<Value> {
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
        self.write_line(&msg).await?;

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
        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        let mut map = self.pending.lock().await;
        for (_, p) in map.drain() {
            let _ = p.tx.send(Err(CodexRuntimeError::ConnectionClosed(
                "shutdown".into(),
            )));
        }
    }
}

impl Drop for CodexAppServerClient {
    fn drop(&mut self) {
        self.reader_abort.abort();
    }
}

async fn dispatch_inbound(
    msg: InboundMessage,
    pending: &Arc<Mutex<HashMap<String, Pending>>>,
    notify_tx: &tokio::sync::broadcast::Sender<RuntimeEvent>,
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
        if msg.id.is_some() {
            debug!(method, "codex app-server: ignoring server request");
            return;
        }
        let params = msg.params.unwrap_or(Value::Null);
        let event = map_notification(method, &params);
        let _ = notify_tx.send(event);
    }
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
            Ok(Ok(RuntimeEvent::TextDelta { text })) => out.push_str(&text),
            Ok(Ok(RuntimeEvent::TurnCompleted { .. })) => return Ok(out),
            Ok(Ok(RuntimeEvent::Error { message })) => {
                return Err(CodexRuntimeError::Other(message));
            }
            Ok(Ok(_)) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                return Err(CodexRuntimeError::ConnectionClosed("notify".into()));
            }
            Err(_) => return Err(CodexRuntimeError::Timeout(max_wait)),
        }
    }
}

