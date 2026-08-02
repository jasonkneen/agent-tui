---
name: stamp-or-retire-superseded-convention-doc
description: Given a confirmed superseded/superseder pair of convention docs, apply the standard disposition — retire the old doc or stamp it with an attributed supersession banner — then scrub surviving quotes. The shared procedure behind the same-change coupling rule and the backfill sweep.
---

# Stamp or retire a superseded convention doc

Two sites restate this procedure independently: the same-change convention (`superseding-convention-retires-old-doc-same-change.md`) requires it at reconciliation time, and the backfill workflow (`backfill-supersession-markers.md`, steps 3–4) applies it retroactively. Per the extraction convention, two independent restatements of one procedure are the extraction signal — this skill is the shared procedure; both sites should point here.

## When to run it

- **At reconciliation time** — a drift reconciliation is landing a new convention doc that supersedes an existing one. Per the same-change law, this disposition ships in the same change.
- **During the backfill sweep** — an already-superseded doc is discovered live with no marker.

## 0. Confirm the supersession is genuine

Only proceed for a pair asserting **incompatible operative rules for one fact** — adjacent rules lumped by wording are not a supersession. Check the drift ledger: a closed row's canonical value identifies the survivor. If no row exists, register the drift first; if the row is open/TBD, **stop** — a marker cannot be applied while the canonical answer is genuinely TBD (the frozen fact forbids asserting either value).

## 1. Choose the disposition

- **Retire** the superseded doc if nothing cites it as a historical decision record — no ledger row, reconciliation workflow, or convention quotes it for its decision history.
- **Stamp** it if its quotes or decision history must be preserved (reconciliation scrubs operative references but preserves historical decision records). Banner at the very top:

  ```
  > **Superseded by <new-doc> on <date> — do not apply.** (stamped by <author>, <date>)
  ```

Attribution is mandatory either way — an unattributed value cannot serve as evidence later.

## 2. Scrub the survivors

Sweep skills, workflows, and wiki docs that restate the superseded rule as operative. Seed from the pair's known citations, then **re-grep the whole workspace** — named remaining instances are advisory. Historical quotes inside ledger rows and decision records stay; operative restatements are updated to the surviving rule.

## 3. Verify the end state

Exactly one live convention doc asserts an operative rule for the fact. A workspace state where two live convention docs assert opposite operative rules is itself a defect, regardless of which is newer — timestamps alone do not resolve the conflict for a reader who finds only one doc; only the in-file marker or removal does.
