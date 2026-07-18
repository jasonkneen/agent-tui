# Workflow: reconcile the /model-scope drift (Grok-only vs runtime-scoped catalog)

Two governance surfaces now give two values for one fact — the scope of the `/model` command:

- **Value A** — `suggested/agent-updates/model-command-is-grok-only.md` (and `suggested/skills/switch-agent-tui-vendor-runtime.md`, "Know the boundary with /model"): "`/model` selects among **Grok** models only … must not add non-Grok entries to the `/model` list"; a user report that "/model doesn't show Claude" is "working as designed."
- **Value B** — `docs/LOCAL_CLI_AUTH.md`, "TUI usage (shipped)" (modified *after* the convention doc): "After `/runtime codex`, Agent TUI loads Codex `model/list` and **`/model` shows the Codex catalog**. Selection is stored in `~/.agent-tui/runtime.toml` (`codex_model`) and applied on the next thread." Under Value B, `/model` is scoped to the *active runtime*, not to Grok.

The two values are genuinely incompatible: Value A forbids exactly the behavior Value B documents as shipped.

## 1. Register the drift

Open an open-drift-ledger row (per the register-a-doc-drift skill): disputed fact = "scope of `/model`", competing values A and B with their sources and modification timestamps, canonical = TBD. Per convention, the open row **freezes the fact** — until adjudication, no new or edited doc may assert either "Grok-only" or "runtime-scoped" as settled.

## 2. Establish ground truth on the wire

Per the convention *for live services, on-wire behavior is canonical over any doc*, adjudicate from the running TUI, not from either document:

1. Launch a current `agent-tui` build (by path from the build output, per the launch convention).
2. Run `/runtime codex` (requires a logged-in `~/.codex`), then open `/model`.
3. Record what the picker actually lists: Grok catalog, Codex catalog (`model/list` results), or both.
4. Confirm whether a selection writes `codex_model` into `~/.agent-tui/runtime.toml` and whether it applies on the next thread.

If shipped behavior matches Value B, note that Value A's own grounding quote ("`/model` only switches **Grok** models") was lifted from an *earlier revision* of `docs/LOCAL_CLI_AUTH.md` — the convention doc is phantom-grounded against the current source and needs the re-quote/re-ground repair, not just a value swap.

## 3. Adjudicate and rewrite the convention

Assuming Value B wins, the convention's true invariant survives in a narrower form and must be restated, not deleted (a reconciliation edits only the disputed fact):

- **What changes:** `/model` is *runtime-scoped* — it lists the catalog of whichever vendor `/runtime` selected (Grok catalog under `/runtime grok`, Codex `model/list` under `/runtime codex`).
- **What stays:** `/model` is still **not a vendor switcher** — choosing the *vendor* remains `/runtime`'s job, persisted in `runtime.toml`; and cross-vendor entries must never be forced into one flat list that would imply a shared `SamplerConfig` HTTP path. Enumerate these unchanged invariants in the row.

Amend `suggested/agent-updates/model-command-is-grok-only.md` under the rule-amendment procedure (retitle or supersede it — its current title states the disputed value).

## 4. Sweep the operative surfaces

Re-grep the whole workspace (named instances are advisory) for `/model` scope claims. Known stale sites:

- `suggested/agent-updates/model-command-is-grok-only.md` — title, Rule, and Grounding all assert Value A.
- `suggested/skills/switch-agent-tui-vendor-runtime.md` — "`/model` only switches **Grok** models … 'that is working as designed.'"
- Any `AGENTS.md` / user-guide text mirroring the old table.

Update operative references to the canonical value; preserve historical decision records unchanged.

## 5. Close the row

Record the adjudicated canonical value, the on-wire evidence (with author and date), and the swept sites in the ledger row; the reconciliation is finished only when the canonical value is written into the (amended) convention doc.
