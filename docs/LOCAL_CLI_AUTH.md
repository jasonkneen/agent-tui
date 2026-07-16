# External agent runtimes (Claude Agent SDK + Codex app-server)

**Goal:** use vendors the way their products do — **not** by reimplementing OAuth
or raw chat HTTP when a local, already-authenticated harness exists.

| Vendor | Runtime | Transport | Auth |
|--------|---------|-----------|------|
| **Claude** | [Claude Agent SDK](https://github.com/anthropics/claude-agent-sdk-typescript) (`@anthropic-ai/claude-agent-sdk`) | Long-lived SDK client / subprocess; stream messages | Reuse Claude Code login (keychain / `~/.claude`) — **no Agent TUI OAuth** |
| **Codex** | `codex app-server` (+ optional `daemon`) | JSON-RPC over **stdio / unix socket / ws**; turn events stream as notifications | Reuse `~/.codex/auth.json` ChatGPT login — **no Agent TUI OAuth** |
| **Grok / xAI** | Existing `agent-tui-sampler` | HTTP SSE to cli-chat-proxy | Existing OIDC / API key |

Credential **detection** (`auth::local_cli`) only answers “is this CLI logged in?”
and seeds discovery. **Inference** goes through a **warm runtime connection**,
not a one-shot `reqwest` to `api.anthropic.com`.

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

For Claude Agent SDK, streaming is the SDK’s async message stream over its
long-lived client (internally Claude Code harness).

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
│    └─ Codex   → CodexAppServerBridge               │
│         │                                          │
│  ConnectionPool (per LocalRuntimeId)               │
│    spawn | reuse | idle-evict | reconnect          │
└─────────┬──────────────────┬───────────────────────┘
          │                  │
          ▼                  ▼
   Claude Agent SDK     codex app-server
   (Node/TS or CLI      (JSON-RPC: stdio |
    long session)        unix:// | ws://)
          │                  │
          └──── auth from ───┘
           local CLI stores only
```

### Layers (keep tight)

| Layer | Responsibility |
|-------|----------------|
| **L0 Detect** | `local_cli::detect_claude()` / future `detect_codex()` — “CLI present + authed?” |
| **L1 Runtime** | Spawn/attach Claude Agent SDK or `codex app-server` |
| **L2 Pool** | Always-on with idle timeout, health, single-flight reconnect |
| **L3 Bridge** | Map Agent TUI turns ↔ vendor protocol events → pager stream |
| **L4 Route** | Model id / user choice picks runtime; Grok stays default |

Do **not** fold Claude/Codex into `SamplerConfig` HTTP. They are **different
backends** behind a shared “stream of agent events” trait.

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
| **C** | **Claude Agent SDK sidecar** (TS) + Rust bridge; warm session + idle timeout |
| **D** | `RuntimeRouter` + model picker: Grok (sampler) \| Claude (SDK) \| Codex (app-server) |
| **E** | Shared pool metrics, reconnect, optional eager warm; codex `daemon` / `unix://` attach |

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
let (thread_id, turn) = pool.start_text_turn("hello").await?;
```

Requires `codex` on `PATH` (uses local `~/.codex` auth automatically).

Order rationale: Codex app-server is pure JSON-RPC over a socket (fits Rust
well). Claude Agent SDK is Node/Python-native — wrap as a small sidecar so the
TUI stays Rust.

---

## Explicit non-goals (for now)

- Implementing Claude or ChatGPT **OAuth** inside Agent TUI  
- Sending Claude/Codex tokens through Grok cli-chat-proxy  
- Replacing Grok’s sampler for xAI models  
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
