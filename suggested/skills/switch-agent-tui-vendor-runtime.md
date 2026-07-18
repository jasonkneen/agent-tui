---
name: switch-agent-tui-vendor-runtime
description: Check vendor runtime readiness and switch which vendor (Grok, Codex, Claude, Lazar) serves Agent TUI turns using the shipped /runtime command family, understanding what persists where and which vendors are actually routable today.
---

# Switch the active vendor runtime in Agent TUI

Agent TUI ships a command surface for selecting which vendor runtime serves turns (`docs/LOCAL_CLI_AUTH.md`, "TUI usage (shipped)"). This is the user-facing layer over the warm-harness architecture — selection here never triggers OAuth; it routes to the vendor's local, already-authenticated runtime (or the Lazar kernel).

## 1. Check readiness first

```
/runtime
```

Shows Grok / Codex / Claude / Lazar readiness. A vendor that shows not-ready needs its own CLI installed/logged in (Claude Code for Claude, `~/.codex/auth.json` for Codex, `lazar` on PATH or `$LAZAR_HOME/bin/lazar` for Lazar) — there is no Agent TUI-side login to run.

`/provider` and `/rt` are aliases for `/runtime`.

## 2. Switch the runtime

| Command | Effect |
|---------|--------|
| `/runtime grok` | Built-in xAI agent (the default) — existing sampler HTTP SSE |
| `/runtime codex` | Route turns through the warm `codex app-server` (reuses `~/.codex` auth) |
| `/runtime claude` | Route turns through the Claude Code CLI harness |
| `/runtime lazar` | Route turns through the local lazar kernel (`lazar -p` stream-json; kernel owns providers/models) |

The choice is persisted in `~/.agent-tui/runtime.toml`, so it survives restarts. To script or reset the selection, edit that file rather than hunting for another config surface.

### Lazar prerequisites

```sh
source ~/lazar/workspace/lazar-env.sh   # keys + LAZAR_MODEL
# optional: LAZAR_NO_SANDBOX=1 for writes outside ~/lazar
agent-tui
/runtime lazar
```

The Go product TUI (`lazartui`) is **not** in this repo — source is
`~/lazar/workspace/tui/`. Agent TUI only ships the kernel client.

## 3. Know the boundary with /model

`/model` switches models **within the active runtime's catalog**, not across vendors. Change vendors with `/runtime` first. Catalog sources:

| Runtime | Catalog |
|---------|---------|
| Grok | Built-in sampler |
| Codex | `model/list` over app-server |
| Claude | Claude Code harness |
| Lazar | Kernel-reported active model (`LAZAR_MODEL` / `memory/model.txt`) |

## Troubleshooting

- Vendor missing from `/runtime` readiness → its local CLI is not installed/logged in; use the vendor's own tool (never add Agent TUI OAuth).
- Lazar selected but turns fail → env not sourced (`lazar-env.sh`); or `lazar` not on PATH.
- Codex selected but turns are slow on first use → the warm pool may be doing a spawn; subsequent turns ride the warm connection.
- Selection not sticking → check `~/.agent-tui/runtime.toml` exists and is writable; config home resolves via `$AGENT_TUI_HOME` (legacy `$GROK_HOME` accepted).
