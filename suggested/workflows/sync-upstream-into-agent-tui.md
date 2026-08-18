---
name: sync-upstream-into-agent-tui
description: Checklist to port upstream monorepo syncs into the Agent TUI fork without destroying product skins or wire contracts.
---

# Workflow: sync upstream monorepo → Agent TUI

## Preconditions

- [ ] On `fork/agent-tui` (or intended release branch)
- [ ] Working tree clean (or intentional `FORCE=1`)
- [ ] Remote `upstream` → `https://github.com/xai-org/grok-build.git`
- [ ] Read [FORK.md](../../FORK.md) rename boundary + [AGENTS.md](../../AGENTS.md) do-not-break list

## Execute

### A. Discover

```sh
git fetch upstream main
bash scripts/upstream-sync/status.sh
```

- [ ] Note our `SOURCE_REV` and matching upstream commit
- [ ] Skim pending commit bodies (session reaping, compaction, extra CA, …)
- [ ] If pending empty → **done**

### B. Apply

```sh
bash scripts/upstream-sync/apply.sh
```

Or stepwise:

```sh
bash scripts/upstream-sync/checkout-xai.sh
python3 scripts/upstream-sync/port-to-agent-tui.py --bulk
# base rev = matching upstream commit from status.sh
export UPSTREAM_SYNC_BASE_REV=<match-sha>
python3 scripts/upstream-sync/port-to-agent-tui.py --remmerge --base-rev "$UPSTREAM_SYNC_BASE_REV"
python3 scripts/upstream-sync/port-to-agent-tui.py --manifests
```

### C. Workspace root (manual)

Diff upstream `Cargo.toml` for new pins and members:

```sh
git show upstream/main:Cargo.toml | head -n 400
```

- [ ] New `[workspace.dependencies]` crates added (e.g. `ctor`, `rustls`, `zip`, `rhai`, `encoding_rs`, …)
- [ ] New product members present: `agent-tui-extra-ca`, `agent-tui-workflow` (when those crates exist)
- [ ] Path deps under `[workspace.dependencies]` for any new `agent-tui-*` crates
- [ ] `clippy.toml` reasons mention `agent_tui_*` not `xai_*` for disallowed spawn/canonicalize helpers

### D. Verify

- [ ] `cat SOURCE_REV` == `git show upstream/main:SOURCE_REV`
- [ ] `cargo check -p agent-tui-bin`
- [ ] `head crates/codegen/agent-tui-bin/src/main.rs` — product skins entry still present
- [ ] `rg 'jasonkneen/agent-tui' crates/codegen/agent-tui-update/src/version.rs`
- [ ] `rg 'xai-grok-cli' crates/codegen/agent-tui-auth crates/codegen/agent-tui-http | head`
- [ ] New `version.rs` functions from `xai-grok-update` exist on the fork file (identity constants still fork)
- [ ] No `@agent-tui-official` / `agent-tui-org-shared`; pager `npm/` matches pre-sync tree names (`agent-tui*`, not extra `grok*`)
- [ ] Pager `Cargo.toml` still has `which` + four runtime crates
- [ ] Optional: `cargo test -p agent-tui-extra-ca` / focused crate tests for touched areas

### E. Commit

```sh
bash scripts/upstream-sync/apply.sh --commit
# or manually:
git add -A
git commit -m "Synced from monorepo" -m "SOURCE_REV: <old> → <new>"
```

- [ ] Commit does **not** rebrand wire contracts
- [ ] Commit does **not** repoint installers at `x.ai/cli`

## Failure playbook

| Problem | Action |
|---------|--------|
| `cargo` feature/dep errors on manifests | Re-run `port-to-agent-tui.py --manifests`; fix one crate from renamed xai `Cargo.toml` |
| Missing modules/tests | Copy from `crates/**/xai-*` counterpart + rename |
| Lost product_profile / runtime_addon | `git show HEAD:…` or remmerge with `--head-rev` pointing at pre-sync tip |
| `agent-tui-bin` is huge full main | Restore thin main from pre-sync; keep lib product prefix |
| Dirty tree blocked apply | Stash/commit WIP, or `FORCE=1 bash scripts/upstream-sync/apply.sh` |
| Restored `version.rs` missing new APIs (`cli_base_urls`, …) | Port new functions from `xai-grok-update/src/version.rs`; keep fork URL/npm constants |
| Missing fork-only persist helper (`append_external_runtime_message`, …) | Restore that function from pre-sync HEAD; do not revert the remmerged file |
| `update_catalog` takes 1 arg but fork callers pass 2 | Adapt `runtime_backend` (catalog-only + `set_current`) |
| unresolved crate `which` in pager | Add `which = { workspace = true }` to pager `Cargo.toml` |
| `@agent-tui-official` / extra `npm/grok*` | Delete trees absent on pre-sync HEAD; fix `reinstall_hint` |

## Out of scope

- Sending patches **to** upstream (never)
- Renaming provider wire IDs or model IDs
- Replacing fork release identity with official Grok channels
