# Convention: the Claude harness runs `claude -p --output-format json` with a sticky `--resume` session

**Rule.** Claude integration in Agent TUI rides the Claude Code Agent SDK harness invoked as **`claude -p --output-format json`**, holding a **sticky `--resume`** session so successive requests continue one Claude Code session rather than starting fresh. Two implementations are forbidden in either direction:

1. **No fresh-session invocations per request** — dropping `--resume` and spawning an unrelated `claude -p` each time discards session continuity and re-pays the harness's full startup cost every turn.
2. **No bypassing the harness** — replacing the CLI invocation with a direct `api.anthropic.com` client built from detected credentials violates the detection/inference split.

**Grounding.**
- `docs/LOCAL_CLI_AUTH.md`, "TUI usage (shipped)": "**Claude** — Claude Code Agent SDK harness via `claude -p --output-format json` (+ sticky `--resume`)".
- Same doc's runtime table: Claude's auth is "Reuse Claude Code login (keychain / `~/.claude`) — **no Agent TUI OAuth**" — the CLI harness is what carries that reuse.
- Same doc, "Why a warm connection": cold-start cost is dominated by process spawn, auth materialization + initialize handshake, and first-connect work — the costs sticky `--resume` amortizes for a subprocess-shaped vendor, exactly as the warm socket does for Codex.

**Why:** Claude is the vendor whose runtime is a subprocess harness rather than a persistent socket, so the warm-connection principle takes a different concrete shape here — session stickiness via `--resume` instead of a held socket. Without this convention written down, the two failure modes are each locally tempting: per-request fresh spawns look simpler, and a direct API call looks faster to wire; both silently destroy the properties (continuity, harness-owned auth, amortized startup) the architecture depends on. `--output-format json` is equally load-bearing — it is the machine-parseable contract the TUI consumes, and a change to plain text output breaks parsing with no type-level signal.

**How to apply:** when building or reviewing Claude-side code, check that requests flow through the harness invocation with the sticky session identifier threaded between calls, and that output parsing targets the JSON format. When the full Agent SDK bridge lands (ending Claude's detect-only state), preserve the same properties — one continued session, harness-owned auth, structured output — rather than treating the bridge as license to redesign the transport. When debugging Claude-turn latency or lost conversational context, first verify the `--resume` stickiness is intact.
