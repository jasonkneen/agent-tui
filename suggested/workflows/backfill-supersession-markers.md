# Workflow: backfill supersession markers on already-superseded convention docs

The same-change coupling rule — "a superseding convention doc retires or supersession-marks the doc it replaces in the same change" — only governs changes made *after* it landed. The workspace already exhibits the failure it names: `suggested/agent-updates/model-command-is-grok-only.md` still states its rule with no supersession marker alongside its superseder (`model-command-runtime-scoped-supersedes-grok-only.md`), and the same pair exists for `detect-only-runtime-state.md` versus the shipped Claude catalog harness in `docs/LOCAL_CLI_AUTH.md`. This workflow is the one-time (and repeatable) backfill sweep.

## 1. Enumerate candidate pairs from present reality

Per the audit convention, enumerate from reality, never from a remembered list. List every doc in `suggested/agent-updates/`, and for each, search the workspace for other convention docs, drift-ledger rows, or reconciliation workflows that quote it as superseded, phantom-grounded, or describing "a prior state." Seed the sweep with the two named instances above, then re-grep — named remaining instances are advisory.

## 2. Confirm each pair is a genuine supersession

For each candidate pair, verify the two docs assert incompatible operative rules for one fact (not adjacent rules lumped by wording). Check the drift ledger: if the fact has a closed row, the canonical value identifies the survivor. If no row exists and the conflict is real, register the drift first — a marker cannot be applied while the canonical answer is genuinely TBD.

## 3. Stamp or retire the superseded doc

For each confirmed pair, choose:
- **Retire** the old doc if nothing cites it as a historical decision record.
- **Stamp** it with an explicit banner at the top — `Superseded by <new-doc> on <date> — do not apply` — if its quotes or decision history must be preserved (reconciliation scrubs operative references but preserves historical decision records).

Record author and date on the stamp; an unattributed marker cannot serve as evidence later.

## 4. Scrub survivors that quote the dead rule

Sweep skills, workflows, and wiki docs that restate the superseded rule as operative (e.g. `switch-agent-tui-vendor-runtime.md`'s "Know the boundary with /model" section asserting Grok-only scope). Update each to the canonical value or point it at the superseding doc.

## 5. Close out

Update any open drift-ledger rows this sweep resolves, and verify the end state: for every superseded rule, exactly one live convention doc asserts the operative value, and every surviving copy of the old doc carries the banner. A workspace state where two live convention docs assert opposite rules is itself a defect — the sweep is done only when a re-grep finds none.
