# External vendor runtimes — Claude Agent SDK and Codex app-server via warm local harnesses

Agent TUI integrates non-Grok vendors the way their own products do — through a **local, already-authenticated harness** — rather than reimplementing OAuth or raw chat HTTP (`docs/LOCAL_CLI_AUTH.md`).

## The runtime table

| Vendor | Runtime | Transport | Auth |
|--------|---------|-----------|------|
| **Claude** | Claude Agent SDK (`@anthropic-ai/claude-agent-sdk`) | Long-lived SDK client / subprocess; async message stream | Reuses Claude Code login (keychain / `~/.claude`) — no Agent TUI OAuth |
| **Codex** | `codex app-server` (+ optional `daemon`) | JSON-RPC over stdio / unix socket / ws; turn events stream as notifications (`item/agentMessage/delta`, `turn/completed`, …) | Reuses `~/.codex/auth.json` ChatGPT login — no Agent TUI OAuth |
| **Grok / xAI** | Existing `agent-tui-sampler` | HTTP SSE to cli-chat-proxy | Existing OIDC / API key |

Note the streaming asymmetry: Codex streaming is JSON-RPC notifications on a persistent socket, not classic HTTP SSE — same latency properties (one open channel, incremental events); Claude streaming is the SDK's message stream over its long-lived client (internally the Claude Code harness).

## Detection vs inference — two separate concerns

`agent_tui_shell::auth::local_cli` (Claude checked first) only answers "is this CLI logged in?" and seeds discovery. **Inference never goes through a one-shot `reqwest` to `api.anthropic.com`** — it goes through a warm runtime connection (`docs/LOCAL_CLI_AUTH.md`).

## Why a warm connection

Cold-start cost is dominated by: (1) process spawn, (2) auth materialization + initialize handshake, (3) TLS / first model connect / tool index load. So Agent TUI keeps **one connection per vendor runtime** (per workspace or global — config later) with:

- **Idle timeout** — e.g. 5–15 min without a turn → graceful shutdown
- **Health probe** — ping / noop before reuse; respawn if dead
- **Eager warm** — optional on TUI start if that CLI is detected + enabled

This is the "always on (with timeout) SSE connection for speed."

## Hard boundaries (from `AGENTS.md`)

- Do **not** rename ACP method IDs (`xai.api_key`, `grok.com`, …)
- Do **not** send third-party tokens to the Grok chat proxy
- Do **not** force Claude/Codex through `SamplerConfig` HTTP — use a runtime bridge

See the companion convention doc for why these boundaries exist and how to apply them when adding a vendor.