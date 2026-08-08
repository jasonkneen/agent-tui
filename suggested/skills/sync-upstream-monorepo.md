---
name: sync-upstream-monorepo
description: Port Grok Build monorepo syncs from upstream/main into this Agent TUI fork via SOURCE_REV alignment, xai-* checkout, agent-tui rename port, and fork remmerge. Use when syncing upstream, catching up monorepo, or SOURCE_REV is behind.
---

# Sync upstream monorepo into Agent TUI

> **Canonical skill + scripts:** `scripts/upstream-sync/` and (if installed) `~/.agent-tui/skills/sync-upstream-monorepo/`.
> **Workflow checklist:** [sync-upstream-into-agent-tui.md](../workflows/sync-upstream-into-agent-tui.md).

## One-liner

```sh
bash scripts/upstream-sync/status.sh
bash scripts/upstream-sync/apply.sh          # clean tree required
bash scripts/upstream-sync/apply.sh --commit # after cargo check is green
```

## Why this exists

- `upstream/main` and `fork/agent-tui` often share **no merge-base** → no naive merge/cherry-pick of the tip.
- Monorepo commits only update **`xai-*`** trees + `SOURCE_REV`.
- Product is **`agent-tui-*`** (renamed) plus fork addons (product skins, Claude/Codex/Lazar/Hermes runtimes, GitHub release identity).

## Non-negotiables

Do **not** rewrite wire contracts: `xai-grok-cli`, `X-XAI-Token-Auth`, `xai.api_key`, `XAI_API_KEY`.  
Do **not** open PRs against upstream (source-transparency only).  
Do **not** bulk-overwrite `agent-tui-bin` or `agent-tui-*-runtime` from xai.

## Steps (agent)

1. `git fetch upstream main` + `bash scripts/upstream-sync/status.sh`
2. If pending commits: `bash scripts/upstream-sync/apply.sh` (or step through `checkout-xai.sh` → `port-to-agent-tui.py --bulk|--remmerge|--manifests`)
3. Patch root `Cargo.toml` for **new** workspace dependency pins seen on `upstream/main`
4. `cargo check -p agent-tui-bin`
5. Verify bin entry, updater repo id, wire contracts
6. Commit as `Synced from monorepo` with old→new `SOURCE_REV`

## Script map

| Path | Role |
|------|------|
| `scripts/upstream-sync/status.sh` | Pending sync report |
| `scripts/upstream-sync/checkout-xai.sh` | xai trees = upstream tip |
| `scripts/upstream-sync/port-to-agent-tui.py` | bulk / remmerge / manifests |
| `scripts/upstream-sync/apply.sh` | Full pipeline |
| `scripts/upstream-sync/rename.py` | Rename + protect list |
