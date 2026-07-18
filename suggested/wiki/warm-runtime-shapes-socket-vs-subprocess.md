# The two warm-runtime shapes — held socket vs sticky subprocess session

Every external vendor runtime in Agent TUI follows the warm-connection principle: never pay cold-start (process spawn, auth materialization + initialize handshake, TLS / first model connect / tool index load — `docs/LOCAL_CLI_AUTH.md`, "Why a warm connection") per request. But the principle has **two concrete shapes**, and the shipped vendors demonstrate one each:

| Shape | Exemplar | Transport | What "warm" means | What amortizes the cold start |
|---|---|---|---|---|
| **Held socket** | Codex — `codex app-server` via `CodexRuntimePool` | JSON-RPC over stdio / unix socket / ws; turn events stream as notifications | One long-lived server connection per vendor runtime | The open connection itself: handshake and tool index are paid once, reused every turn |
| **Sticky subprocess session** | Claude — Claude Code Agent SDK harness | `claude -p --output-format json` invocations | One continuing session, carried by a sticky `--resume` id across invocations | Session continuity: successive requests continue one Claude Code session instead of starting fresh (`suggested/agent-updates/claude-harness-sticky-resume.md`: "the costs sticky `--resume` amortizes for a subprocess-shaped vendor, exactly as the warm socket does for Codex") |

## Why the distinction matters

The warm-pool lifecycle convention (idle timeout, health probe, optional eager warm) was written against the socket exemplar, and its wording — "one warm connection" — reads naturally only for Codex. The Claude row shows the same three lifecycle obligations map onto session shape differently:

- **Idle timeout** — socket: close the connection; subprocess session: let the sticky session lapse rather than holding resources indefinitely.
- **Health probe before reuse** — socket: ping/noop, respawn if dead; subprocess session: verify the `--resume` id still resolves, start a fresh session if it doesn't.
- **Eager warm** — socket: spawn the server on TUI start when the CLI is detected; subprocess session: optionally establish the initial session early.

## Choosing a shape for a new vendor

The shape is dictated by what the vendor's own harness offers — vendors integrate "the way their products do" (`docs/LOCAL_CLI_AUTH.md`):

- Vendor ships a **persistent server** (app-server, daemon, socket API) → held socket; model on `agent-tui-codex-runtime` / `CodexRuntimePool`.
- Vendor ships only a **CLI with session resumption** → sticky subprocess session; model on the Claude harness (`claude -p --output-format json` + sticky `--resume`), with a machine-parseable output format as the contract the TUI consumes.

Both shapes sit behind the same hard boundaries: detection (`auth::local_cli`) stays discovery-only, inference never becomes a one-shot HTTP call built from detected credentials, and neither shape ever rides `SamplerConfig` HTTP. What is forbidden in each shape is symmetric — per-request spawns for a socket vendor, fresh-session invocations (dropping `--resume`) for a subprocess vendor — because both re-pay the full cold start every turn.
