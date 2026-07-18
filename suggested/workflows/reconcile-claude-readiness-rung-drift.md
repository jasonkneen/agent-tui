# Workflow: reconcile the Claude readiness-rung drift (detect-only vs catalog-shipped)

`docs/LOCAL_CLI_AUTH.md` makes two claims about where Claude sits on the vendor readiness ladder, and they cannot both be operative:

- **Claim A — detect-only.** The TUI usage table annotates "`/runtime claude` — Select Claude (detect-only until Agent SDK bridge)", and `suggested/agent-updates/detect-only-runtime-state.md` grounds an entire convention on it: "no turns route to it until its real runtime bridge (warm pool / SDK client) lands. Claude is currently in this state."
- **Claim B — catalog rung reached via a live harness.** The same doc, a few lines later: "After `/runtime codex` or `/runtime claude`, Agent TUI loads that vendor's model catalog and **`/model` switches it**", with the Claude catalog served by the "Claude Code Agent SDK harness via `claude -p --output-format json` (+ sticky `--resume`)" and selection persisted as `claude_model` in `~/.agent-tui/runtime.toml`.

If a working harness answers catalog requests, Claude is no longer detect-only in the plain sense — it is at least on the catalog rung of the readiness ladder (detect, catalog, inference ship independently). The open question the drift freezes: **does the `claude -p` harness also route inference turns, or does it serve the catalog only?** Until adjudicated, the "detect-only" label and the harness description are competing values for one fact.

## 1. Register the drift

Open a drift-ledger row per the register-doc-drift skill. Disputed fact: *Claude's readiness rung in Agent TUI (detect-only / catalog-only / fully routable)*. Competing values: Claim A and Claim B above, each with its verbatim quote, source path, and (where available) author/date. Mark the canonical answer **TBD** — per convention, a TBD row still freezes the fact: no new or edited doc may assert either rung until this closes.

## 2. Establish ground truth from behavior, not docs

On-wire/runtime behavior is canonical over any doc for live services. In a logged-in environment:

1. Run `/runtime claude`, then `/model` — does a live Claude catalog load? (Confirms the harness exists and the catalog rung is real.)
2. Send a turn — does inference route through the Claude harness, or is the turn refused/no-op? This is the decisive probe: routed turns mean the Agent SDK bridge has effectively landed; a refused turn means the harness is catalog-scoped and "detect-only" merely needs renaming to "catalog-only, inference pending".
3. Cross-check `~/.agent-tui/runtime.toml` for `claude_model` writes and the warm-pool behavior (idle timeout, health probe) if turns route — a routing path without the three pool behaviors is a separate defect, not evidence of readiness.

A doc's rationalization ("detect-only is intentional") is not evidence — probe it like any claim.

## 3. Adjudicate and scrub

- **If turns route:** the detect-only label is stale. Run the promote-detect-only-vendor-runtime workflow for Claude; sweep every operative surface that carries the annotation — the `docs/LOCAL_CLI_AUTH.md` table, `/runtime` help text, the `AGENTS.md` runtime table, the switch-vendor-runtime skill, and `suggested/agent-updates/detect-only-runtime-state.md` (which must stop citing Claude as its live example). Re-grep the whole workspace for "detect-only" + Claude rather than trusting this list.
- **If the harness is catalog-only:** the readiness-ladder terminology is canonical — replace "detect-only" with the precise rung ("catalog shipped, inference pending the Agent SDK bridge") everywhere the annotation appears, so the label can no longer contradict the catalog paragraph beside it.

Either way, edit only the disputed fact; enumerate the invariants that stay unchanged (no Agent TUI OAuth, no `SamplerConfig` HTTP for Claude, detection stays discovery-only, `runtime.toml` stays the persistence surface).

## 4. Close the row

Record the adjudicated rung with author and date, write the canonical value into the owning convention doc (the reconciliation is finished only when this lands), preserve the historical decision record in the ledger, and close the row.
