//! One-shot turn via `LazarRuntimePool` — used by `scripts/parity-eval.sh`.
//!
//! Usage:
//!   cargo run -p agent-tui-lazar-runtime --example parity_turn -- \
//!     --prompt "..." [--model ID] [--session ID] [--cwd PATH] [--bin PATH]
//!
//! Prints one JSON object on stdout:
//!   {"ok":true,"text":"...","session_id":"...","ms":1234}
//! or
//!   {"ok":false,"error":"...","ms":1234}

use agent_tui_lazar_runtime::{LazarRuntimePool, PoolConfig};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut prompt: Option<String> = None;
    let mut model: Option<String> = None;
    let mut session: Option<String> = None;
    let mut cwd: Option<PathBuf> = None;
    let mut bin = PathBuf::from("lazar");
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--prompt" => {
                i += 1;
                prompt = args.get(i).cloned();
            }
            "--model" => {
                i += 1;
                model = args.get(i).cloned();
            }
            "--session" => {
                i += 1;
                session = args.get(i).cloned();
            }
            "--cwd" => {
                i += 1;
                cwd = args.get(i).map(PathBuf::from);
            }
            "--bin" => {
                i += 1;
                if let Some(p) = args.get(i) {
                    bin = PathBuf::from(p);
                }
            }
            other => {
                eprintln!("unknown arg: {other}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }
    let Some(prompt) = prompt else {
        eprintln!("usage: parity_turn --prompt TEXT [--model ID] [--session ID] [--cwd PATH] [--bin PATH]");
        return ExitCode::from(2);
    };

    let pool = LazarRuntimePool::new(PoolConfig {
        lazar_bin: bin,
        cwd: cwd.or_else(|| Some(agent_tui_lazar_runtime::lazar_home())),
        default_model: model.clone(),
        turn_timeout: std::time::Duration::from_secs(
            env::var("EVAL_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(120),
        ),
    });

    if let Some(sid) = session {
        pool.set_session(sid).await;
    }

    let t0 = Instant::now();
    match pool.start_text_turn(&prompt, model.as_deref()).await {
        Ok(res) => {
            let ms = t0.elapsed().as_millis();
            println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "text": res.text,
                    "session_id": res.session_id,
                    "ms": ms,
                })
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            let ms = t0.elapsed().as_millis();
            println!(
                "{}",
                serde_json::json!({
                    "ok": false,
                    "error": e.to_string(),
                    "ms": ms,
                })
            );
            ExitCode::from(1)
        }
    }
}
