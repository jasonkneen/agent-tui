---
name: switch-agent-tui-vendor-runtime
description: Check vendor runtime readiness and switch which vendor (Grok, Codex, Claude) serves Agent TUI turns using the shipped /runtime command family, understanding what persists where and which vendors are actually routable today.
---

# Switch the active vendor runtime in Agent TUI

Agent TUI ships a command surface for selecting which vendor runtime serves turns (`docs/LOCAL_CLI_AUTH.md`, "TUI usage (shipped)"). This is the user-facing layer over the warm-harness architecture — selection here never triggers OAuth; it routes to the vendor's local, already-authenticated runtime.

## 1. Check readiness first

```
/runtime
```

Shows Grok / Codex / Claude readiness via local CLI detect (`auth::local_cli` — "is this CLI logged in?"). A vendor that shows not-ready needs its own CLI logged in (Claude Code login for Claude, `~/.codex/auth.json` ChatGPT login for Codex) — there is no Agent TUI-side login to run.

`/provider` and `/rt` are aliases for `/runtime`.

## 2. Switch the runtime

| Command | Effect |
|---------|--------|
| `/runtime grok` | Built-in xAI agent (the default) — existing sampler HTTP SSE |
| `/runtime codex` | Route turns through the warm `codex app-server` (reuses `~/.codex` auth) |
| `/runtime claude` | Select Claude — **detect-only until the Agent SDK bridge lands**; selecting it does not yet route inference |

The choice is persisted in `~/.agent-tui/runtime.toml`, so it survives restarts. To script or reset the selection, edit that file rather than hunting for another config surface.

## 3. Know the boundary with /model

`/model` only switches **Grok** models. It is not a vendor switcher — picking a Codex or Claude path goes through `/runtime`, never `/model`. If a user reports "/model doesn't show Claude," that is working as designed.

## Troubleshooting

- Vendor missing from `/runtime` readiness → its local CLI is not logged in; log in with the vendor's own tool (never add Agent TUI OAuth).
- Codex selected but turns are slow on first use → the warm pool may be doing an eager or lazy spawn; subsequent turns ride the warm connection (see the warm-harness architecture doc).
- Selection not sticking → check `~/.agent-tui/runtime.toml` exists and is writable; remember the config home resolves via `$AGENT_TUI_HOME` (legacy `$GROK_HOME` accepted).