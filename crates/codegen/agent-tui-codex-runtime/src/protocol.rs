//! Wire types for Codex app-server JSON-RPC (subset used by Agent TUI).
//!
//! Protocol notes (openai/codex app-server):
//! - Bidirectional JSON-RPC 2.0 **without** the `"jsonrpc":"2.0"` field on the wire.
//! - stdio transport: one JSON object per line (JSONL).
//! - Notifications have `method` + `params` and no `id`.
//! - Responses echo `id` with `result` or `error`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Outgoing request (client → server).
#[derive(Debug, Clone, Serialize)]
pub struct OutgoingRequest {
    pub id: Value,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// Outgoing notification (client → server), no response expected.
#[derive(Debug, Clone, Serialize)]
pub struct OutgoingNotification {
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// Inbound line from the server (response, notification, or server-initiated request).
#[derive(Debug, Clone, Deserialize)]
pub struct InboundMessage {
    #[serde(default)]
    pub id: Option<Value>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<Value>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<RpcErrorBody>,
}

impl InboundMessage {
    pub fn is_response(&self) -> bool {
        self.id.is_some() && self.method.is_none()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcErrorBody {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

// ── initialize ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub client_info: ClientInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<InitializeCapabilities>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental_api: Option<bool>,
}

// ── thread / turn ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadStartParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
}

/// One row from `model/list`.
#[derive(Debug, Clone)]
pub struct CodexModelEntry {
    pub id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub is_default: bool,
    pub hidden: bool,
    pub default_reasoning_effort: Option<String>,
    /// Effort ids in catalog order (e.g. `low`, `medium`, `high`).
    pub supported_reasoning_efforts: Vec<String>,
    pub input_modalities: Vec<String>,
}

/// Params for `model/list`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelListParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_hidden: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartParams {
    pub thread_id: String,
    pub input: Vec<UserInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UserInput {
    #[serde(rename = "text")]
    Text { text: String },
}

/// Normalized runtime events Agent TUI can consume.
#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    /// Streaming agent text.
    TextDelta { text: String },
    /// Turn began.
    TurnStarted { turn_id: Option<String> },
    /// Turn finished successfully (or interrupted).
    TurnCompleted { status: Option<String> },
    /// Named notification we don't specially map (method string kept).
    Notification { method: String, params: Value },
    /// Server-level error notification.
    Error { message: String },
}

/// Map a server notification into a [`RuntimeEvent`] when possible.
pub fn map_notification(method: &str, params: &Value) -> RuntimeEvent {
    match method {
        "item/agentMessage/delta" | "agentMessage/delta" => {
            let text = params
                .get("delta")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            RuntimeEvent::TextDelta { text }
        }
        "turn/started" => {
            let turn_id = params
                .pointer("/turn/id")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            RuntimeEvent::TurnStarted { turn_id }
        }
        "turn/completed" => {
            let status = params
                .pointer("/turn/status")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            RuntimeEvent::TurnCompleted { status }
        }
        "error" => {
            let message = params
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown app-server error")
                .to_string();
            RuntimeEvent::Error { message }
        }
        other => RuntimeEvent::Notification {
            method: other.to_string(),
            params: params.clone(),
        },
    }
}
