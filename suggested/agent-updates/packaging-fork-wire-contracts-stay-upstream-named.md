# Convention: provider wire contracts keep upstream names in a packaging fork — rename product identity only

**Rule.** When rebranding a packaging/product fork (Grok Build → Agent TUI), rename only product-identity surfaces: product name, binary, crate packages, config home. Anything a provider's servers see on the wire keeps its upstream name: auth method IDs, credential env vars, model IDs, production endpoints, User-Agent shapes, and magic header values. "Only **provider wire contracts** stay xAI-named — not crate packages" (`FORK.md`).

**Grounding.**
- `FORK.md`, "What we deliberately did **not** change": lists auth method IDs (`xai.api_key`, `grok.com`, `oidc`, `cached_token`), env credentials (`XAI_API_KEY`, `GROK_CODE_XAI_API_KEY`), model IDs (`grok-build`, `composer-2-fast`), endpoints (`*.grok.com`), and User-Agent (`grok-shell/...`) as frozen "so auth, models, and backends keep working."
- The hardest evidence is the header rule: "**`X-XAI-Token-Auth` header value** must stay `xai-grok-cli` (proxy rejects unknown values → 401 / paywall loop). Do **not** rename this to the product name."

**Why:** the server side of the wire contract is not under the fork's control. A rename that reaches an auth method ID, model ID, or magic header doesn't produce a compile error — it produces a runtime 401, a missing model catalog, or a paywall loop that looks like an account problem. The fork table in `FORK.md` exists precisely because the safe rename boundary is invisible from the code alone.

**How to apply:** before renaming any identifier during fork maintenance, classify it — product identity (rename) vs wire contract (freeze). When a string could be either (the word "composer" names both a model family and the prompt input box), check what reads it: if a remote service parses it, it's frozen. Record every new frozen surface in `FORK.md`'s "did not change" list so the boundary survives the next rebrand pass.
