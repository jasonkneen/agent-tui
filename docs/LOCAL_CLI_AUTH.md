# External agent runtimes (Claude · Codex · Lazar)

**Architecture:** [ONE CORE + ADDONS](CORE_AND_ADDONS.md) — one linked binary,
symlink product skins, runtime **addons** (including Hermes). This doc is the
vendor-harness detail; product profiles only brand and filter addons.

**Goal:** use vendors the way their products do — **not** by reimplementing OAuth
or raw chat HTTP when a local, already-authenticated harness exists.

## TUI usage (shipped)

| Command | What it does |
|---------|----------------|
| `/runtime` | Show Grok / Codex / Claude / Lazar / Hermes readiness |
| `/runtime grok` | Built-in xAI agent (default on platform) |
| `/runtime codex` | Route turns through warm `codex app-server` (`~/.codex` auth) |
| `/runtime claude` | Route turns through Claude Code CLI harness (`claude -p`) |
| `/runtime lazar` | Route turns through the local **lazar kernel** (`lazar -p` stream-json) |
| `/runtime hermes` | Route turns through **Hermes** Agent (`hermes chat -q -Q`) |
| `/provider`, `/rt` | Aliases for `/runtime` |

After `/runtime codex|claude|lazar|hermes`, Agent TUI loads that vendor’s model
catalog and **`/model` switches it**. Selection is stored in `runtime.toml`
(`codex_model` / `claude_model` / `lazar_model` / `hermes_model`).

| Vendor | Catalog source |
|--------|----------------|
| **Codex** | `model/list` over warm app-server |
| **Claude** | Claude Code aliases / discovered models via CLI harness |
| **Lazar** | Single kernel-reported active model (`LAZAR_MODEL` / `memory/model.txt`); providers stay kernel-side |
| **Hermes** | Config default (`~/.hermes/config.yaml` / `HERMES_MODEL`) |
| **Grok** | Built-in sampler catalog |

| Vendor | Runtime | Transport | Auth |
|--------|---------|-----------|------|
| **Claude** | Claude Code harness (`claude -p --output-format json` + sticky `--resume`; full Agent SDK sidecar optional later) | Subprocess stream | Reuse Claude Code login (keychain / `~/.claude`) — **no Agent TUI OAuth** |
| **Codex** | `codex app-server` (+ optional `daemon`) | JSON-RPC over **stdio / unix socket / ws**; turn events stream as notifications | Reuse `~/.codex/auth.json` ChatGPT login — **no Agent TUI OAuth** |
| **Grok / xAI** | Existing `agent-tui-sampler` | HTTP SSE to cli-chat-proxy | Existing OIDC / API key |
| **Lazar** | `lazar -p --output-format stream-json` (spawn-per-turn; `agent-tui-lazar-runtime`) | JSONL events on stdout | Kernel provider config (`lazar-env.sh` / `LAZAR_MODEL`) — **no Agent TUI provider code** |
| **Hermes** | `hermes chat -q -Q` (spawn-per-turn; `agent-tui-hermes-runtime`) | Quiet stdout + sticky `--resume` | Hermes config / credentials (`~/.hermes`) |

Credential **detection** (`auth::local_cli`) only answers “is this CLI logged in?”
and seeds discovery. **Inference** goes through a **runtime bridge** (warm pool for
Codex/Claude; spawn-per-turn for Lazar), not a one-shot `reqwest` to a vendor chat API.

---

## Why a warm connection

Cold-start cost is dominated by:

1. Process spawn (Claude Agent SDK / `codex app-server`)
2. Auth materialization + initialize handshake
3. TLS / first model connect / tool index load

So Agent TUI keeps **one connection per vendor runtime** (per workspace or
global — config later), with:

- **Idle timeout** — e.g. 5–15 min without a turn → graceful shutdown  
- **Health probe** — ping / noop before reuse; respawn if dead  
- **Eager warm** — optional on TUI start if that CLI is detected + enabled  

That is the “always on (with timeout) SSE connection for speed.”

For Codex, streaming is **JSON-RPC notifications** on a persistent socket
(`item/agentMessage/delta`, `turn/completed`, …), not classic HTTP SSE — same
latency properties (one open channel, incremental events).

For Claude, streaming is the CLI harness message stream (or an Agent SDK client
when the sidecar lands).

For **Lazar**, there is no warm process: each turn is a fresh `lazar -p` spawn
(same contract as the Go `lazartui`). Continuity is the kernel `--session` flag
and `logs/sessions/<id>.jsonl`.

---

## Architecture

```
┌──────────────── agent-tui (Rust) ─────────────────┐
│  pager / shell / ACP                               │
│         │                                          │
│         ▼                                          │
│  RuntimeRouter  (model → backend)                  │
│    ├─ Grok    → Sampler (existing HTTP SSE)        │
│    ├─ Claude  → ClaudeAgentBridge                  │
│    ├─ Codex   → CodexAppServerBridge               │
│    └─ Lazar   → LazarRuntimePool (spawn-per-turn)  │
│         │                                          │
│  ConnectionPool (per LocalRuntimeId; Grok/Claude/  │
│  Codex warm; Lazar reuses --session only)          │
│    spawn | reuse | idle-evict | reconnect          │
└─────────┬──────────────┬──────────────┬────────────┘
          │              │              │
          ▼              ▼              ▼
   Claude Agent SDK  codex app-server  lazar -p
   (Node/TS or CLI   (JSON-RPC: stdio  (stream-json;
    long session)     unix:// | ws://)  kernel providers)
          │                  │
          └──── auth from ───┘
           local CLI stores only
```

### Layers (keep tight)

| Layer | Responsibility |
|-------|----------------|
| **L0 Detect** | `local_cli` / binary presence — “CLI present + authed?” (Lazar: `lazar` on PATH or `$LAZAR_HOME/bin/lazar`) |
| **L1 Runtime** | Spawn/attach Claude harness, `codex app-server`, or `lazar -p` |
| **L2 Pool** | Warm connection + idle for Codex/Claude; sticky `--session` for Lazar |
| **L3 Bridge** | Map Agent TUI turns ↔ vendor protocol events → pager stream |
| **L4 Route** | `/runtime` + model id pick runtime; Grok stays default |

Do **not** fold Claude/Codex/Lazar into `SamplerConfig` HTTP. They are **different
backends** behind a shared “stream of agent events” path.

---

## Claude path — Agent SDK

### What we use

- Package: `@anthropic-ai/claude-agent-sdk` (or Python `claude-agent-sdk`)  
- Long-lived: `ClaudeSDKClient` / multi-turn session (not one-shot `query()` per keystroke if avoidable)  
- Auth: SDK uses Claude Code’s existing login (same keychain / credentials) when available  

### Bridge sketch (Rust side)

Preferred integration (pick one; order of preference):

1. **Sidecar Node process** owned by Agent TUI  
   - `agent-tui-claude-bridge` small TS service using the official SDK  
   - Speak a **thin JSON-RPC / NDJSON** over stdio to Rust (our protocol, stable)  
   - SDK holds the warm Claude session + streams events to the bridge  

2. **Direct subprocess to `claude` print/stream mode** only if SDK is overkill  
   - Worse for multi-turn / tools; prefer SDK  

Rust never reimplements Claude OAuth. Detect → ensure bridge up → stream.

### Idle timeout

```
on first Claude model use → ensure_claude_runtime()
on each turn              → reset idle timer
on idle > CLAUDE_RUNTIME_IDLE_SECS (default 600)
                          → shutdown SDK client / sidecar
on next use               → cold start again
```

---

## Codex path — app-server

### What we use (already on your machine)

```sh
codex app-server                    # default stdio://
codex app-server --listen unix://   # durable local socket
codex app-server daemon start       # managed daemon
# schema:
codex app-server generate-json-schema --out ./schemas
```

Protocol (official): **JSON-RPC 2.0** (no `"jsonrpc":"2.0"` on wire),
bidirectional, MCP-like.

Core lifecycle:

1. `initialize` + `initialized` (once per connection)  
2. `thread/start` | `thread/resume`  
3. `turn/start` → stream notifications (`item/*`, `turn/completed`, …)  
4. Optional: `turn/steer`, `turn/interrupt`  
5. Keep socket open across turns for speed  

Auth: app-server uses Codex CLI’s existing ChatGPT / API credentials from
`~/.codex` — **no second OAuth**.

### Bridge sketch (Rust)

```
CodexAppServerBridge
  ensure_running():
    try connect unix:// or daemon socket
    else spawn: codex app-server --listen unix://PATH
    initialize once
  start_turn(prompt) → stream events into Agent TUI event bus
  idle_timeout → drop connection (daemon may stay up; we detach)
```

Prefer **unix socket + daemon** for always-on:

```sh
codex app-server daemon start
# Agent TUI attaches; idle timeout only drops our client, not necessarily the daemon
```

Config:

```toml
[runtimes.codex]
enabled = true
# attach | spawn | daemon
mode = "daemon"
listen = "unix://"          # or ws://127.0.0.1:4500 for debug
idle_timeout_secs = 900
```

---

## Shared Rust trait (single abstraction)

```rust
/// Long-lived vendor agent runtime (not HTTP chat completions).
#[async_trait]
trait AgentRuntime: Send + Sync {
    fn id(&self) -> LocalRuntimeId; // Claude | Codex | …
    async fn ensure_ready(&self) -> Result<()>;  // spawn/attach + health
    async fn start_turn(&self, req: RuntimeTurnRequest)
        -> Result<BoxStream<'_, RuntimeEvent>>;
    async fn interrupt(&self) -> Result<()>;
    fn touch(&self);                 // reset idle timer
    async fn shutdown(&self) -> Result<()>;
}
```

`RuntimeEvent` is a **small** enum Agent TUI already almost has (text delta,
tool start/end, done, error) — map vendor events in the bridge, not in the pager.

---

## How this relates to credential detect

| Module | Role |
|--------|------|
| `auth::local_cli` (exists) | “Is Claude Code / Codex logged in?” for UI + gate enablement |
| `runtimes::claude` (todo) | Own Agent SDK sidecar + pool |
| `runtimes::codex` (todo) | Own app-server client + pool |

Detect alone is **not** enough for speed. Runtime pool is the product.

---

## Phased delivery

| Phase | Deliverable |
|-------|-------------|
| **A** | Claude detect (done) + UI “Claude Code ready” |
| **B** | **Codex app-server client** — `agent-tui-codex-runtime` crate (**done**): stdio JSON-RPC, initialize, thread/start, turn/start, stream notifications, warm pool + idle timeout |
| **C** | **Claude CLI harness routing** — `claude -p` + sticky resume (**done** for turn path; full Agent SDK sidecar optional) |
| **D** | `RuntimeRouter` + `/runtime` + model picker: Grok \| Claude \| Codex \| **Lazar** (**done**) |
| **E** | Shared pool metrics, reconnect, optional eager warm; codex `daemon` / `unix://` attach |
| **F** | **Lazar kernel client** — `agent-tui-lazar-runtime` (**done**): spawn-per-turn stream-json, sticky `--session`, parity-eval script |

### Phase B crate

```
crates/codegen/agent-tui-codex-runtime/
  src/client.rs   # spawn codex app-server --stdio, JSON-RPC
  src/pool.rs     # warm single-slot pool, idle recycle
  src/protocol.rs # wire types + RuntimeEvent mapping
  tests/mock_stdio.rs
```

```rust
use agent_tui_codex_runtime::{CodexRuntimePool, PoolConfig};

let pool = CodexRuntimePool::new(PoolConfig::default());
let client = pool.ensure_ready().await?;
let mut events = client.subscribe();
let (thread_id, turn) = pool.start_text_turn("hello", None).await?;
```

Requires `codex` on `PATH` (uses local `~/.codex` auth automatically).

Order rationale: Codex app-server is pure JSON-RPC over a socket (fits Rust
well). Claude Agent SDK is Node/Python-native — wrap as a small sidecar so the
TUI stays Rust.

---

## Product profiles (brand + default runtime)

Agent TUI is a **core shell**. A **product profile** sets who it is (name, title)
and which runtime is default / locked — without forking the binary.

| Source (first wins) | Example |
|---------------------|---------|
| `AGENT_TUI_PRODUCT=lazar` | Named preset: brand Lazar, lock to `lazar` runtime |
| `AGENT_TUI_PRODUCT=agent-tui` | Default multi-vendor Agent TUI |
| `AGENT_TUI_PRODUCT_FILE=/path/product.toml` | Custom file |
| `~/.agent-tui/product.toml` | Persistent profile |
| (none) | Built-in Agent TUI defaults (`default_runtime = grok`) |

Example Lazar product (also `docs/product.lazar.example.toml`):

```toml
id = "lazar"
name = "Lazar"
title_token = "lazar"
default_runtime = "lazar"
lock_runtime = true
```

| Mode | Profile knobs | UX |
|------|---------------|-----|
| **Platform** | default Agent TUI, `lock_runtime = false` | `/runtime` switches; `/model` per vendor |
| **Lazar only** | `AGENT_TUI_PRODUCT=lazar` or example toml | Title/welcome say **Lazar**; runtime locked; `/model` = kernel |
| **Lazar primary** | `default_runtime = lazar`, `lock_runtime = false`, `enabled_runtimes = [...]` | Brand Lazar, still can `/runtime codex` etc. |

Launcher helper:

```sh
source ~/lazar/workspace/lazar-env.sh
# one-shot:
AGENT_TUI_PRODUCT=lazar ./target/debug/agent-tui
# or:
./scripts/lazartui-agent.sh
```

Code: `crates/codegen/agent-tui-pager/src/product_profile.rs`.

---

## Lazar path — kernel spawn-per-turn (shipped)

### What we use

- Binary: `lazar` on `PATH`, or `$LAZAR_HOME/bin/lazar` (default home `~/lazar`)
- Per turn:

```text
lazar --output-format stream-json [--model <id>] --session <id> -p <prompt>
```

- Cwd: `$LAZAR_HOME` so skills/hooks/memory/sessions resolve like the Go TUI
- Env: **caller must source** `~/lazar/workspace/lazar-env.sh` (or equivalent)
  before launching Agent TUI — keys + `LAZAR_MODEL` / `ANTHROPIC_*` live there
- **Model ↔ provider:** Agent TUI remaps credentials per turn from the selected
  model (Go TUI parity). `kimi-k3` uses `KIMI_*` + Kimi/Moonshot base; `MiniMax-M*`
  uses `MINIMAX_API_KEY` + MiniMax base. You do **not** put a MiniMax key on a
  Kimi model — that produces MiniMax `401 X-Api-Key` / auth errors.
  Prefer launching with the matching preset:
  `LAZAR_BACKEND=kimi source ~/lazar/workspace/lazar-env.sh`
- Crate: `crates/codegen/agent-tui-lazar-runtime` (`LazarRuntimePool`)

### Run (local)

```sh
cd /path/to/grok-build   # or agent-tui checkout
source ~/lazar/workspace/lazar-env.sh
export LAZAR_NO_SANDBOX=1   # optional: write outside ~/lazar
cargo run -p agent-tui-bin
# inside TUI:
#   /runtime lazar
#   then chat
```

Selection persists as `active = "lazar"` in `~/.agent-tui/runtime.toml`.

### Parity eval (CLI path vs pool)

Same prompts through a Go-TUI-equivalent CLI spawn and `LazarRuntimePool`:

```sh
source ~/lazar/workspace/lazar-env.sh
export LAZAR_NO_SANDBOX=1
bash crates/codegen/agent-tui-lazar-runtime/scripts/parity-eval.sh
# cases: echo | tool | file | session  (EVAL_CASES=… to subset)
```

### What stays outside Agent TUI (kernel / launchd)

| Concern | Owner |
|---------|--------|
| Heartbeat ticks (~30m) | launchd `lazar.heartbeat` → `skills/_meta/heartbeat/tick.sh` |
| Repo reviewer | launchd `lazar.repo-reviewer` |
| Providers / model default | `lazar-env.sh`, `memory/model.txt`, kernel |
| Skills, hooks, sandbox, VERIFY | kernel on every `lazar -p` turn |

Agent TUI does **not** implement heartbeat, `.tui-alerts` wake banners, or the
Go TUI’s `/add-dir` access picker. Those remain kernel- or lazartui-side.

### Gaps vs the Go `lazartui` (presentation only)

| Go lazartui | Agent TUI `/runtime lazar` |
|-------------|----------------------------|
| Live `tool_use` / `tool_result` chrome | Text flattened (`text_delta` only) |
| `/add-dir` → kernel `--add-dir` | Not wired (set `LAZAR_NO_SANDBOX` or kernel policy yourself) |
| Wake poll of `workspace/.tui-alerts` | Not read |
| Skill `/` autocomplete, Ctrl+S steer-kill | Use Agent TUI’s own slash/queue UX |
| `ensureProxy` on backends that need it | Start proxy yourself if `LAZAR_BACKEND` requires it |

Kernel intelligence (bash tool, skills, memory, session logs) is the same binary.

### Where the Go TUI still lives

**Not in this repo.** The old Lazar TUI is under the lazar home tree:

| Path | What |
|------|------|
| `~/lazar/workspace/tui/` | Go source (`main.go`, …) — primary tree |
| `~/lazar/workspace/lazartui` | Built Go binary |
| `~/lazar/workspace/lazartui.sh` | Launcher (build + `lazar-env` + exec) |
| `/usr/local/bin/lazartui` → `lazartui.sh` | PATH entry |
| `~/lazar/skills/lazartui/` | Skill wrapper |
| `~/lazar/workspace/tui-watchdog.sh` + `lazar.tui.plist` | Optional Ghostty keep-alive (may be unloaded) |

This fork only ships the **client** `agent-tui-lazar-runtime` that spawns the
kernel the same way `lazartui` does for turns.

### Phase crate

```
crates/codegen/agent-tui-lazar-runtime/
  src/lib.rs              # LazarRuntimePool, discover_active_model, stream parse
  examples/parity_turn.rs # one-shot JSON turn for evals
  scripts/parity-eval.sh  # CLI vs pool behavioral parity
```

```rust
use agent_tui_lazar_runtime::{LazarRuntimePool, PoolConfig};

let pool = LazarRuntimePool::new(PoolConfig {
    lazar_bin: "lazar".into(),
    cwd: Some(agent_tui_lazar_runtime::lazar_home()),
    ..Default::default()
});
let res = pool.start_text_turn("hello", None).await?;
```

---

## Explicit non-goals (for now)

- Implementing Claude or ChatGPT **OAuth** inside Agent TUI  
- Sending Claude/Codex/Lazar tokens through Grok cli-chat-proxy  
- Replacing Grok’s sampler for xAI models  
- Replacing the Go `lazartui` product chrome (alerts, `/add-dir` UI, tool folds)  
- One mega-enum of “providers” in the pager  

---

## Config sketch

```toml
[runtimes]
# Global idle for all warm runtimes
idle_timeout_secs = 600

[runtimes.claude]
enabled = true
# sidecar | disabled
mode = "sidecar"
# path to node bridge entry (or bundled)
# bridge_command = ["node", "…/claude-bridge.mjs"]

[runtimes.codex]
enabled = true
# daemon | spawn | attach
mode = "daemon"
listen = "unix://"
# require detect_codex() before enabling
require_local_auth = true
```

---

## Open decisions

1. **Claude bridge host:** Node sidecar (recommended) vs shell out to `claude` only?  
2. **Codex attach vs manage daemon:** prefer `daemon start` once vs Agent TUI owns process?  
3. **Workspace isolation:** one runtime per cwd / git root, or one global?  
4. **Eager warm:** on TUI start if both CLIs detected, or only on first model switch?  

Default recommendation: **Codex daemon + attach**, **Claude Node sidecar**,  
**warm on first use**, idle 10 min, one runtime per Agent TUI process (shared
cwd = TUI cwd).
