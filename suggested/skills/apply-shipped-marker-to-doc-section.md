---
name: apply-shipped-marker-to-doc-section
description: Add a "(shipped)" marker to a doc section the right way — verify every claim on the wire first, scope the section to only what it asserts, attribute and date the change — so the marker records live behavior rather than manufacturing false evidence.
---

# Apply a '(shipped)' marker to a doc section

The marker is operative, not decorative: a claim inside a "(shipped)"-marked section outranks unmarked prose, earlier-revision quotes, and older convention docs in drift adjudication. The standing audit (`audit-shipped-markers-against-live-behavior`) catches bad markers after the fact; this skill is the authoring-side procedure that keeps them from being written wrong in the first place. The convention's own Why-section names the risk: "a future editor could add or drop it casually, silently changing the evidentiary weight of every claim in the section."

## When to mark

- You are documenting behavior that has actually landed in the running product (the exemplar: `docs/LOCAL_CLI_AUTH.md`'s "TUI usage (shipped)" table).
- The audit's direction two found an unmarked live-behavior section that is "a weaker witness than it should be."

Never mark a section describing planned or aspirational behavior — "mislabeling a design intention as shipped manufactures false evidence."

## Procedure

1. **Probe before you mark.** Extract the section's individually checkable claims and verify each against the running product — for vendor-runtime claims, run the probe-vendor-readiness-rung skill on a current build launched by path (never Spotlight/Dock/bundle id). A claim you cannot probe does not go under the marker.
2. **Scope the section to what it asserts.** The marker's weight is section- and subject-scoped — it lends nothing to claims the section does not itself make. Move design rationale (the "Why a warm connection" genre) outside the marked section, the way `docs/LOCAL_CLI_AUTH.md` separates the shipped table from the rationale in the same file.
3. **Check the frozen facts.** If any claim in the section is the disputed fact of an open drift-ledger row, stop — a TBD row freezes the fact, and a shipped marker would assert a value with elevated evidentiary weight. Close or adjudicate the row first.
4. **Attribute and date the marker.** Record author and date on the change — an unattributed value cannot serve as evidence later, and the /model-scope reconciliation was carried by the marker *plus* timestamps.
5. **Sweep for now-outranked claims.** Adding the marker raises this section above unmarked and older statements. Re-grep the workspace for contrary claims the new marker now outranks — each one is either a doc fix in this change or a drift-ledger registration, never left to be discovered by the next adjudication.

## Removing or downgrading a marker

Stripping a marker is the same act in reverse — it silently demotes every claim in the section. Only do it inside a registered reconciliation ("fix the claim or strip the marker in the reconciliation — never leave a false claim under a shipped marker"), attributed and dated like the addition.
