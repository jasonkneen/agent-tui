---
name: repoint-restating-sites-after-extraction
description: After pulling a shared skill out of two or more independent restatements of one procedure, sweep every site that still embeds the restated steps and replace each inline copy with a pointer to the extracted skill plus only that site's specific deltas — so the extraction ends the duplication instead of adding a third copy.
---

# Repoint the restating sites after extracting a shared skill

The extraction convention has two clauses — "pull the shared skill, repoint both sites" — and only the first has ever been executed as a procedure. The workspace exhibits the gap twice, in the same change-set that performed the extractions:

- `suggested/skills/probe-vendor-readiness-rung.md`: "Both readiness reconciliation workflows embed this same procedure; this skill is the shared extraction" — yet both reconciliation workflows still carry their inline probe steps.
- `suggested/skills/sweep-vendor-rung-label-surfaces.md`: "this skill is the shared procedure, and those sites should point here" — the detect-only convention, the promote-to-routable workflow, and the ship-catalog-rung workflow still each restate the three-surface list inline.

An extraction that leaves the inline copies live has not removed two restatements; it has added a third. Worse, the copies and the skill now age independently — the next edit to one is a manufactured drift.

## When to run it

- In the same change as any new shared-skill extraction (preferred — the same-change coupling law applies).
- As a backfill, when an extraction already shipped with its sites unrepointed (the two instances above are the seed list).

## Steps

1. **Enumerate restating sites from present reality.** Re-grep the workspace for distinctive phrases of the extracted procedure (surface names, command strings, step wording). The extraction doc's own list of named sites is advisory — named remaining instances always are.
2. **Classify each hit before touching it.** A genuine restatement embeds the same procedure for the same purpose. If a site's version differs materially in steps or outcome, it is not a restatement — stop and check whether the difference is an unregistered drift before repointing anything.
3. **Replace the embedded steps with a pointer.** At each confirmed site, cut the inline procedure down to one line — "run the <skill-name> skill" — plus only the site-specific parameters (which vendor, which surface, which precondition). The site keeps its own trigger and context; it delegates the procedure.
4. **Preserve historical decision records.** If a site's inline copy carries attributed decisions or grounding quotes that other docs cite, scrub the operative steps but keep the record — reconciliation scrubs operative references, never history.
5. **Re-grep for survivors.** After editing the named sites, sweep the whole workspace again for the procedure's distinctive phrases — skills, wiki pages, and convention docs quoting the old inline steps are also operative surfaces.

## Rules

- The repoint lands in the same change as the extraction; when the extraction already shipped without it, this skill is the sanctioned backfill.
- A repointed site never re-inlines the steps later "for convenience" — a reader who needs the procedure follows the pointer; a site that needs different steps is a drift to register, not a fork to make silently.
