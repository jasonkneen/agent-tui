---
name: sweep-vendor-rung-label-surfaces
description: When a vendor runtime moves up (or is discovered to sit on a different rung of) the readiness ladder, update every surface that labels its rung — the /runtime help text, the docs/LOCAL_CLI_AUTH.md usage table, and the AGENTS.md runtime table — as one coupled change, so no surface keeps asserting a stale rung.
---

# Sweep the vendor rung-label surfaces after a readiness change

Three artifacts independently restate the same surface list: the detect-only convention says to "label it detect-only in every surface that mentions it (the `/runtime` help, `docs/LOCAL_CLI_AUTH.md` table, `AGENTS.md` runtime table)"; the promote-to-routable workflow scrubs "the detect-only annotations" from the same trio; the ship-catalog-rung workflow's step 5 updates the same three to "catalog-shipped / inference-pending." Per the extraction convention, two independent restatements of one procedure are the extraction signal — this skill is the shared procedure, and those sites should point here.

## When to run it

- A vendor gains a rung (detect → catalog, catalog → inference) via the owning workflow.
- A drift adjudication or a readiness probe establishes that a vendor's actual rung differs from its labels (the Claude "detect-only" annotation coexisting with a shipped catalog harness is the registered example).

## The surfaces

| Surface | What to update |
|---|---|
| `/runtime` help text | The per-vendor annotation shown in the running TUI (e.g. "detect-only until Agent SDK bridge") |
| `docs/LOCAL_CLI_AUTH.md`, "TUI usage (shipped)" table | The command row's parenthetical, and any prose describing the vendor's wiring — this is a "(shipped)" section, so a stale claim here actively outranks correct newer prose elsewhere in drift adjudication |
| `AGENTS.md` runtime table | The vendor's row in the multi-vendor runtime table |

After editing the named three, re-grep the whole workspace for the old rung label — named instances are advisory, and skills or wiki pages quoting the old annotation are also operative surfaces.

## Rules

- **Label the rung you can prove.** State the highest consecutive rung confirmed by the readiness probe (probe-vendor-readiness-rung), not the rung a plan intends to ship. Mislabeling an intention as shipped manufactures false evidence under the "(shipped)" marker convention.
- **All surfaces move in the same change.** A partial sweep leaves two live surfaces asserting different rungs — a self-inflicted drift-ledger row.
- **Use the ladder's vocabulary.** "Detect-only", "catalog-shipped / inference-pending", or fully routable — invented phrasings force the next reader to adjudicate synonyms.
