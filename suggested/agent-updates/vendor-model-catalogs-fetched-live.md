# Convention: vendor model catalogs are fetched live from the vendor's own runtime — never hardcoded

**Rule.** When a vendor runtime is selected, Agent TUI loads that vendor's model catalog **from the vendor's own runtime**, and `/model` then switches within that live catalog. Each vendor has exactly one catalog source:

- **Codex** — `model/list` over the warm `codex app-server` connection
- **Claude** — the Claude Code harness via `claude -p --output-format json`
- **Lazar** — kernel-reported active model (`LAZAR_MODEL` / `memory/model.txt`); not a multi-model remote list
- **Grok / xAI** — the existing built-in sampler catalog

No surface — code, docs, or skills — may hardcode another vendor's model IDs into Agent TUI, and no second catalog source may be invented for a vendor that already has one.

**Grounding.**
- `docs/LOCAL_CLI_AUTH.md`, "TUI usage (shipped)": after `/runtime codex`, `/runtime claude`, or `/runtime lazar`, Agent TUI loads that vendor's model catalog and **`/model` switches it**. Selection is stored in `~/.agent-tui/runtime.toml` (`codex_model` / `claude_model` / `lazar_model`).
- Same doc vendor table rows for Codex / Claude / Lazar catalog sources.
- The harness-reuse boundary in the same doc: vendors integrate "the way their products do", never by reimplementing the vendor's surfaces — a hardcoded model list is exactly such a reimplementation.

**Why:** vendor model lineups change out from under any pinned list — a hardcoded catalog silently offers dead models and hides new ones, with no compile-time or CI signal when it drifts. Fetching through the vendor's own runtime means the catalog is exactly what the vendor's harness would offer its own product, and it rides the already-warm connection, so freshness costs nothing. It also keeps the fork's frozen-contract discipline intact: model IDs remain vendor-owned wire strings that Agent TUI transports but never authors.

**How to apply:** when adding a vendor or extending catalog behavior, wire the catalog query into that vendor's runtime pool (the same connection that serves turns), never a static list or a one-shot HTTP call. When reviewing a change, treat a literal non-Grok model ID appearing in fork source as a defect unless it is quoting the vendor's wire contract in a frozen-contract context. When writing docs or skills that mention available models, describe *how the catalog is fetched* rather than enumerating model names that will go stale.
