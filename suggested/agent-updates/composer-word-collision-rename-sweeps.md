# Convention: the pager's 'composer' prompt-input strings are not the Composer model family — rename and model sweeps skip them

**Rule.** The word "composer" appears in the tree with two unrelated meanings, and both are excluded from rename work for different reasons:

1. **Composer model IDs** (`composer-2-fast`, …) are frozen provider wire contract — "Composer is a model name, not a separate product or provider" (`FORK.md`). They stay upstream-named like all model IDs.
2. **The pager's prompt input box** is called the "composer" in pager source as an ordinary English word. `FORK.md` is explicit: "That is unrelated to the Composer model family and is left alone."

A grep for `composer` during a rename sweep, a model-catalog audit, or a fork-identity change therefore yields hits from two populations, neither of which may be touched as part of product renaming.

**Grounding.**
- `FORK.md`, deliberately-unchanged list: "**Model IDs** in the xAI/Grok catalog, including … **Composer** models (`composer-2-fast`, …) — Composer is a model name, not a separate product or provider."
- `FORK.md`, note: "pager source also uses the English word 'composer' for the **prompt input box**. That is unrelated to the Composer model family and is left alone."

**Why:** the fork's rename boundary is enforced by grep sweeps (the change-release-identity workflow verifies with "zero operative hits" greps). A sweeper who finds `composer` strings and doesn't know the collision will either "fix" a frozen model ID (breaking the provider wire contract) or rename UI-internal prompt-box identifiers (churning upstream-shared code for no reason). The collision is invisible without this note because both populations look equally renameable.

**How to apply:** when running any rename, brand, or model-catalog sweep, classify every `composer` hit before editing: catalog/wire context → frozen model ID, leave it; pager UI context (input box, editor widget) → English word, leave it. If a sweep's verification grep must be run on `composer`, record both populations as expected residual hits rather than driving them to zero. When adding new UI code, avoid introducing a third meaning of the word.