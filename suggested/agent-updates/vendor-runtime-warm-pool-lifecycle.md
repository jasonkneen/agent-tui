# Convention: vendor runtime pools keep one warm connection — idle timeout, health probe, optional eager warm; never per-request spawns

**Rule.** Every external-vendor runtime (Claude Agent SDK sidecar, `codex app-server` via `CodexRuntimePool`, and any future vendor) maintains **one warm connection per vendor runtime** (per workspace or global), with exactly three lifecycle behaviors:

1. **Idle timeout** — ~5–15 min without a turn → graceful shutdown
2. **Health probe** — ping/noop before reuse; respawn if dead
3. **Eager warm** (optional) — on TUI start, when that vendor's CLI is detected and enabled

Spawning the runtime per request — or holding a connection open forever with no idle timeout or health check — is a defect in either direction.

**Grounding.**
- `docs/LOCAL_CLI_AUTH.md`, "Why a warm connection": "Agent TUI keeps **one connection per vendor runtime** (per workspace or global — config later)", followed by the three bullets (idle timeout, health probe, eager warm) quoted above. It also enumerates the cold-start costs this amortizes: process spawn, auth materialization + initialize handshake, TLS / first model connect / tool index load.
- `AGENTS.md`, Multi-vendor runtimes: "Codex app-server client: `agent-tui-codex-runtime` (`CodexRuntimePool`, warm + idle timeout)".
- `suggested/workflows/integrate-new-vendor-runtime.md`, step 2 requires the same three properties for every new runtime crate.

**Why:** cold-start cost dominates external-vendor latency — a per-request spawn re-pays process launch, handshake, and TLS on every turn, defeating the entire point of the harness-reuse architecture ("the always on (with timeout) SSE connection for speed"). Conversely, a connection with no idle timeout leaks a subprocess indefinitely, and reuse without a health probe hands a turn to a dead socket and fails it. The three behaviors are the minimal set that makes a warm pool both fast and safe.

**How to apply:** when building or reviewing a runtime crate, check for all three behaviors explicitly — a pool missing any one is incomplete, not a style choice. Model new pools on `agent-tui-codex-runtime` (`CodexRuntimePool`). When debugging vendor latency, first confirm the turn actually rode the warm connection rather than triggering a cold respawn; when debugging stuck turns, check the health probe fired before reuse. Streaming stays on the runtime's native channel (SDK message stream, JSON-RPC notifications) — the warm connection is the transport, never a reason to fall back to one-shot HTTP.
