//! Mock app-server over a fake `codex` binary to exercise the client without
//! a real Codex install / network.

#![cfg(unix)]

use agent_tui_codex_runtime::{
    ClientIdentity, CodexAppServerClient, RuntimeEvent, TransportConfig,
};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::timeout;

/// Write a fake `codex` script that implements a minimal app-server on stdio.
fn write_fake_codex(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("codex");
    // Responds to initialize + thread/start + turn/start; emits a couple of notifications.
    let script = r#"#!/usr/bin/env python3
import sys, json

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method = msg.get("method")
    mid = msg.get("id")
    if method == "initialize":
        send({"id": mid, "result": {"userAgent": "fake-codex", "platformOs": "test"}})
    elif method == "initialized":
        pass
    elif method == "thread/start":
        send({"id": mid, "result": {"thread": {"id": "thr_test_1"}}})
        send({"method": "thread/started", "params": {"thread": {"id": "thr_test_1"}}})
    elif method == "turn/start":
        send({"id": mid, "result": {"turn": {"id": "turn_1", "status": "inProgress"}}})
        send({"method": "turn/started", "params": {"threadId": "thr_test_1", "turn": {"id": "turn_1"}}})
        send({"method": "item/agentMessage/delta", "params": {
            "threadId": "thr_test_1", "turnId": "turn_1", "itemId": "i1", "delta": "hello "
        }})
        send({"method": "item/agentMessage/delta", "params": {
            "threadId": "thr_test_1", "turnId": "turn_1", "itemId": "i1", "delta": "world"
        }})
        send({"method": "turn/completed", "params": {
            "threadId": "thr_test_1", "turn": {"id": "turn_1", "status": "completed"}
        }})
    else:
        send({"id": mid, "error": {"code": -32601, "message": f"unknown method {method}"}})
"#;
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[tokio::test]
async fn mock_initialize_thread_and_stream_deltas() {
    let dir = tempfile::tempdir().unwrap();
    let codex = write_fake_codex(dir.path());

    let client = CodexAppServerClient::connect(
        TransportConfig::SpawnStdio {
            codex_bin: codex,
            extra_args: vec![],
        },
        ClientIdentity {
            name: "test".into(),
            version: "0.0.0".into(),
            title: Some("test".into()),
        },
    )
    .await
    .expect("connect");

    let mut rx = client.subscribe();

    let thread_id = client
        .thread_start(Default::default())
        .await
        .expect("thread/start");
    assert_eq!(thread_id, "thr_test_1");

    // Drain thread/started
    let _ = timeout(Duration::from_secs(2), rx.recv()).await;

    client
        .turn_start_text(&thread_id, "hi")
        .await
        .expect("turn/start");

    let mut text = String::new();
    let completed = timeout(Duration::from_secs(3), async {
        loop {
            match rx.recv().await {
                Ok(RuntimeEvent::TextDelta { text: t, .. }) => text.push_str(&t),
                Ok(RuntimeEvent::TurnCompleted { status, .. }) => {
                    assert_eq!(status.as_deref(), Some("completed"));
                    break;
                }
                Ok(_) => {}
                Err(e) => panic!("recv: {e}"),
            }
        }
    })
    .await;
    assert!(completed.is_ok(), "timed out waiting for turn");
    assert_eq!(text, "hello world");

    client.shutdown().await;
}

#[tokio::test]
async fn missing_codex_binary_errors_cleanly() {
    let result = CodexAppServerClient::connect(
        TransportConfig::SpawnStdio {
            codex_bin: PathBuf::from("/nonexistent/codex-binary-xyz"),
            extra_args: vec![],
        },
        ClientIdentity::default(),
    )
    .await;
    assert!(result.is_err(), "should fail when binary missing");
    let msg = result.err().unwrap().to_string();
    assert!(
        msg.contains("not found") || msg.contains("spawn") || msg.contains("No such"),
        "msg={msg}"
    );
}
