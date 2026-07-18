---
name: switch-model-within-vendor-runtime
description: Browse the active vendor runtime's live model catalog with /model, switch the model within that vendor, verify the vendor-scoped persistence key in ~/.agent-tui/runtime.toml, and know when the pick takes effect and what it does NOT change.
---

# Switch models within the active vendor runtime

`/model` operates on the **active runtime's** catalog, not a fixed Grok list (`docs/LOCAL_CLI_AUTH.md`, "TUI usage (shipped)": "After `/runtime codex` or `/runtime claude`, Agent TUI loads that vendor's model catalog and **`/model` switches it**"). The older "`/model` only switches Grok models" reading is superseded — do not apply it, and do not quote skills that still assert it.

## 1. Confirm which vendor is active

```
/runtime
```

`/model` always switches *within* the currently selected vendor. To change vendors, use `/runtime <vendor>` first (aliases `/provider`, `/rt`) — `/model` never changes which vendor serves turns.

## 2. Open the catalog and pick a model

```
/model
```

The list you see is fetched **live from the vendor's own runtime** — never a hardcoded list:

| Active runtime | Catalog source |
|---|---|
| Grok / xAI | Built-in sampler catalog |
| Codex | `model/list` over the warm `codex app-server` connection |
| Claude | Claude Code Agent SDK harness via `claude -p --output-format json` (+ sticky `--resume`) |

## 3. Know where the pick persists and when it applies

Selection is stored as a **vendor-scoped key** in `~/.agent-tui/runtime.toml` — `codex_model` / `claude_model` — alongside the runtime selection itself, and it **applies on the next thread**, not mid-thread. To verify or script a pick, read/edit that file rather than hunting for another config surface; each vendor's key is independent, so switching runtimes later restores that vendor's last pick.

## 4. Know what a catalog does NOT prove

Per the vendor readiness ladder, catalog ships independently of inference. Claude is the load-bearing example: `/runtime claude` loads a live catalog and `/model` persists `claude_model`, yet turn routing may still await the Agent SDK bridge. Seeing a vendor's models in `/model` does not mean turns route to that vendor — check the vendor's readiness rung before concluding anything about routing.

## Troubleshooting

- **Catalog is empty or fails to load** — the vendor's own CLI is not logged in (`~/.codex/auth.json` for Codex, Claude Code login for Claude). Log in with the vendor's tool; there is no Agent TUI-side login.
- **Pick doesn't seem to take effect** — it applies at the next thread boundary by design; start a new thread rather than expecting a mid-thread swap.
- **Expected vendor's models missing** — you're on a different active runtime; run `/runtime` to check, then `/runtime <vendor>` before `/model`.
- **Tempted to add a model ID by hand** — don't: catalogs are live-fetched from the vendor runtime, and a literal non-Grok model ID in fork source is a defect.