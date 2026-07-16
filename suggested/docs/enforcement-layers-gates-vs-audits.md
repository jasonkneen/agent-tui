---
name: enforcement-layers-gates-vs-audits
description: The workspace's two enforcement layers — standing bidirectional audits, which catch failures whose aftermath leaves record and reality disagreeing (apparent blessing, phantom references), and decision-time gates, the only layer that can catch failures whose aftermath leaves them agreeing (invisible removals) — with the mapping from each named failure family to its layer, and the design rule for choosing a layer when authoring a new counter-design.
metadata:
  type: project
---

# Two enforcement layers: decision-time gates vs standing audits

The split is drawn by the newest failure family in its own words: in the apparent-blessing family "a bidirectional audit can still catch the lie," but after an invisible removal "there is no lie left to catch" — record and reality genuinely agree (`suggested/docs/invisible-removal-failure-family.md`). The workspace already operates both layers; this doc is the derived index that names them. Content lives in the family catalogs and gate conventions cited below — fixes land there first.

## Layer 1: Standing audits (after the fact)

Bidirectional record-vs-reality sweeps, registered in the standing-audit registry and dispatched by rotation. They work because the failure's aftermath is a *disagreement* an enumeration from reality can surface:

- **Apparent blessing** — the record falsely testifies skipped work was covered; the reality → record direction exposes the testimony.
- **Phantom references** — a pointer cell names a dead address; resolving every pointer exposes it (the family's instances each map to a detecting audit, `suggested/docs/phantom-reference-failure-family.md`).

## Layer 2: Decision-time gates (at the moment of choice)

Checks that run *before* an irreversible decision lands, because afterward no audit has anything to find:

- **The survivor sweep** — retirement is admissible only after a workspace sweep by the target's invariant identity comes back empty (`suggested/agent-updates/sweep-for-survivors-before-retiring.md`).
- **The conflicted-operator protocol** — defer out of the pressured moment, land as a reviewed change, justify with third-party-checkable evidence (`suggested/agent-updates/conflicted-operator-needs-explicit-justification.md`).
- **The genuine-process-change test** for stage removal, and **the bind-time conformance check** — "Bind time is the only gate that can catch this" (`suggested/agent-updates/registry-rows-bind-only-conforming-workflows.md`).

## The mapping

| Failure family | Aftermath | Catchable by audit? | Enforcement layer |
|---|---|---|---|
| Apparent blessing | Record lies about coverage | Yes — bidirectional sweep | Standing audits |
| Phantom reference | Pointer disagrees with reality | Yes — resolve every pointer | Standing audits (+ same-change coupling as prevention) |
| Invisible removal | Record and reality agree | **No** | Decision-time gates only |

## The design rule

When authoring a counter-design for a newly named failure, first ask what its aftermath looks like: if record and reality end up disagreeing, a standing audit (registered, conforming, bidirectional) is the right layer; if they end up agreeing, an audit is false comfort — the counter-design must be a gate at the decision moment, and the gate itself should follow the conflicted-operator counter-design, since the invisible failures are exactly the ones whose cheapest outcome spares the decider.