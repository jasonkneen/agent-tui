# Workflow: integrate a new external vendor runtime into Agent TUI

Claude (Claude Agent SDK warm sidecar) and Codex (`codex app-server` via `CodexRuntimePool`) were integrated with the same shape: reuse the vendor's own local, already-authenticated harness instead of reimplementing OAuth or raw chat HTTP (`docs/LOCAL_CLI_AUTH.md`: "use vendors the way their products do"). This workflow captures that shape for the next vendor.

## Steps

1. **Add a detection helper** in `agent_tui_shell::auth::local_cli` that answers only "is this vendor's CLI logged in?" — probing its local login state (keychain, config dir like `~/.claude` or `~/.codex/auth.json`). Detection seeds discovery; it is never an inference path.
2. **Build the runtime client as its own crate**, following `agent-tui-codex-runtime` (`CodexRuntimePool`): one warm connection per vendor runtime, with
   - **idle timeout** (~5–15 min without a turn → graceful shutdown),
   - **health probe** (ping/noop before reuse; respawn if dead),
   - optional **eager warm** on TUI start when the CLI is detected and enabled.
   These are the properties that amortize the cold-start costs `docs/LOCAL_CLI_AUTH.md` enumerates: process spawn, auth materialization + initialize handshake, TLS / first model connect / tool index load.
3. **Stream over the runtime's native channel** — the vendor SDK's message stream, JSON-RPC notifications on a persistent socket (`item/agentMessage/delta`, `turn/completed`, …), or whatever the harness natively emits. Do **not** force the vendor through `SamplerConfig` HTTP; use a runtime bridge (`AGENTS.md`).
4. **Respect the hard boundaries** (`AGENTS.md`): no Agent TUI OAuth for the vendor, never send third-party tokens to the Grok chat proxy, and do not rename ACP method IDs.
5. **Register the vendor in both tables in the same change**: the Multi-vendor runtimes table in `AGENTS.md` (Vendor / Runtime / Auth) and the fuller table in `docs/LOCAL_CLI_AUTH.md` (adding transport). Both docs currently enumerate Claude, Codex, and Grok — a runtime that lands without its rows is invisible to the next maintainer.

## Verify

- With the vendor's CLI logged out, detection reports not-available and no runtime spawns.
- With it logged in, a first turn spawns the runtime, a second turn reuses the warm connection, and the idle timeout eventually shuts it down gracefully.
- Grep the new crate for direct provider-API HTTP clients constructed from detected credentials — there should be none (inference goes through the harness only).