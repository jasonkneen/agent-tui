# Convention: external vendors integrate via local harness reuse — never Agent TUI OAuth, never tokens to the Grok proxy, never SamplerConfig HTTP

**Rule.** When Agent TUI adds or maintains a non-Grok vendor (Claude, Codex, Lazar, future vendors), it uses the vendor's own local, already-authenticated runtime — "vendor harnesses + local login, not Agent TUI OAuth" (`AGENTS.md`). Three prohibitions are absolute:

1. Do **not** implement Agent TUI-side OAuth for a vendor whose local CLI login can be reused (Claude Code login in keychain / `~/.claude`; Codex ChatGPT login in `~/.codex/auth.json`).
2. Do **not** send third-party tokens to the Grok chat proxy.
3. Do **not** force Claude/Codex through `SamplerConfig` HTTP — use a runtime bridge (Claude Agent SDK client; `codex app-server` JSON-RPC).

**Grounding.**
- `AGENTS.md`, "Multi-vendor runtimes (not raw OAuth)": "Prefer **vendor harnesses + local login**, not Agent TUI OAuth", followed by the three Do-not bullets quoted above verbatim.
- `docs/LOCAL_CLI_AUTH.md`: "**Goal:** use vendors the way their products do — **not** by reimplementing OAuth or raw chat HTTP when a local, already-authenticated harness exists", and "Inference goes through a **warm runtime connection**, not a one-shot `reqwest` to `api.anthropic.com`."

**Why:** the vendor harness owns credential storage, refresh, and the wire contract — reimplementing them forks a moving surface the fork doesn't control. Sending a third-party token to the Grok proxy crosses a credential boundary (the proxy is a different trust domain), and routing Claude/Codex through `SamplerConfig` HTTP would pretend a JSON-RPC/SDK runtime is an SSE sampler, losing streaming semantics and the warm-connection latency model.

**How to apply:** when wiring a new vendor, first check whether a local authenticated harness exists (extend `agent_tui_shell::auth::local_cli` for detection); if it does, build a runtime bridge around it with the warm-pool behaviors (idle timeout, health probe, optional eager warm) rather than adding OAuth flows or raw HTTP clients. Treat any PR that adds a vendor token to proxy request headers, or a Claude/Codex entry to `SamplerConfig`, as a convention violation regardless of whether it works.