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
6. Close remmerge seams (identity restore vs remmerged callees vs fork-only callers) — playbook in the [workflow](../workflows/sync-upstream-into-agent-tui.md) and `~/.agent-tui/skills/sync-upstream-monorepo/references/architecture.md`
7. Commit as `Synced from monorepo` with old→new `SOURCE_REV`

## After apply — remmerge seams

`apply.sh` bulk-overwrites shared crates, remmerges (shared files take *theirs*), then restores a few identity/fork files from pre-sync HEAD. Those three steps create **API seams**, not random missing files.

| Symptom | Fix (do not revert the remmerged file) |
|---------|----------------------------------------|
| `cannot find function` on restored `version.rs` | Port **new functions** from `xai-grok-update/src/version.rs`; keep `jasonkneen/agent-tui` / `@agent-tui/agent-tui`. `AGENT_TUI_CLI_BASE_URL` first, then `GROK_CLI_BASE_URL`. |
| `cannot find function` only fork callers use | Restore that **function** from `$UPSTREAM_SYNC_HEAD_REV` (e.g. `append_external_runtime_message`) |
| `update_catalog` argument-count mismatch | Adapt `runtime_backend`: catalog-only + `set_current`. Refresh must not clobber the displayed model. |
| unresolved crate `which` in pager | Re-add `which = { workspace = true }` (manifests rewrite pager `Cargo.toml` from xai) |
| `@agent-tui-official/grok` / `agent-tui-org-shared` / extra `npm/grok*` | Delete trees absent on pre-sync HEAD; restore `reinstall_hint` to `@agent-tui/agent-tui` + `jasonkneen/agent-tui` |

## Script map

| Path | Role |
|------|------|
| `scripts/upstream-sync/status.sh` | Pending sync report |
| `scripts/upstream-sync/checkout-xai.sh` | xai trees = upstream tip |
| `scripts/upstream-sync/port-to-agent-tui.py` | bulk / remmerge / manifests |
| `scripts/upstream-sync/apply.sh` | Full pipeline |
| `scripts/upstream-sync/rename.py` | Rename + protect list |
