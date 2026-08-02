# External vendor runtimes — Claude, Codex, and Lazar via local harnesses

Agent TUI integrates non-Grok vendors the way their own products do — through a **local, already-authenticated harness** — rather than reimplementing OAuth or raw chat HTTP (`docs/LOCAL_CLI_AUTH.md`).

## The runtime table

| Vendor | Runtime | Transport | Auth |
|--------|---------|-----------|------|
| **Claude** | Claude Code harness (`claude -p`; Agent SDK sidecar optional) | Subprocess stream / sticky `--resume` | Reuses Claude Code login (keychain / `~/.claude`) — no Agent TUI OAuth |
| **Codex** | `codex app-server` (+ optional `daemon`) | JSON-RPC over stdio / unix socket / ws; turn events stream as notifications | Reuses `~/.codex/auth.json` ChatGPT login — no Agent TUI OAuth |
| **Grok / xAI** | Existing `agent-tui-sampler` | HTTP SSE to cli-chat-proxy | Existing OIDC / API key |
| **Lazar** | `lazar -p --output-format stream-json` (`agent-tui-lazar-runtime`) | Spawn-per-turn JSONL; sticky `--session` | Kernel providers (`lazar-env.sh` / `LAZAR_MODEL`) — no Agent TUI provider code |

Streaming shapes: Codex holds a warm JSON-RPC socket; Claude uses a sticky subprocess session; Lazar re-spawns the kernel each turn (same contract as the Go `lazartui` at `~/lazar/workspace/tui/`).

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
- Do **not** force Claude/Codex/Lazar through `SamplerConfig` HTTP — use a runtime bridge

See the companion convention doc for why these boundaries exist and how to apply them when adding a vendor.