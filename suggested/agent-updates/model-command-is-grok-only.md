# Convention: /model switches Grok models only — cross-vendor selection is /runtime, persisted in runtime.toml

**Rule.** Agent TUI has two selection commands with disjoint scopes. `/model` selects among **Grok** models only. Choosing which *vendor* serves turns — Grok, Codex, or Claude — goes through `/runtime` (aliases `/provider`, `/rt`), and that choice is persisted in `~/.agent-tui/runtime.toml`. Code, docs, and skills must not present `/model` as a vendor switcher, must not add non-Grok entries to the `/model` list, and must not invent a second persistence location for the runtime choice.

**Grounding.**
- `docs/LOCAL_CLI_AUTH.md`, "TUI usage (shipped)": "`/model` only switches **Grok** models. Choice is persisted in `~/.agent-tui/runtime.toml`."
- Same table: `/runtime` shows Grok / Codex / Claude readiness; `/runtime codex` routes turns through warm `codex app-server`; `/runtime claude` is "detect-only until Agent SDK bridge"; `/provider` and `/rt` are aliases.
- The split mirrors the architecture: vendors are whole runtimes (SDK client, JSON-RPC socket, HTTP SSE sampler) with different transports and auth, not interchangeable model IDs in one catalog — forcing them into `/model` would imply they share the Grok sampler path, which the harness-reuse conventions forbid.

**Why:** merging vendor selection into the model picker would blur the hard boundary the fork's runtime architecture draws — Claude and Codex must never ride `SamplerConfig` HTTP, and a `/model claude-...` entry would invite exactly that wiring. Keeping the vendor choice in its own command with its own persistence file (`runtime.toml`) also keeps the detect-only state expressible: Claude can appear in `/runtime` readiness before inference routing exists, which a flat model list cannot represent.

**How to apply:** when adding a vendor or model, decide which surface owns it — a new Grok model ID goes in the `/model` catalog; a new vendor gets a `/runtime` entry backed by a runtime pool. When writing user docs or skills, quote the two commands with their exact scopes and name `~/.agent-tui/runtime.toml` as the persistence location. When reviewing a change, treat a non-Grok entry in the model list, or runtime selection persisted anywhere other than `runtime.toml`, as a convention violation.
