# Agent notes — Agent TUI fork

This repository is a packaging fork of SpaceXAI Grok Build. Product binary is
**`agent-tui`**, config home is **`~/.agent-tui`**. Provider wire contracts
(xAI / Grok.com) stay upstream-named — see [FORK.md](FORK.md).

## Do not break

- Auth method IDs, `XAI_API_KEY`, model IDs, API endpoints
- `X-XAI-Token-Auth: xai-grok-cli` header value (proxy rejects renames)

## Releases & install

Full maintainer procedure: **[RELEASING.md](RELEASING.md)**.

| What | Where |
|------|--------|
| Tag → multi-arch binary release | `.github/workflows/release.yml` |
| PR CI | `.github/workflows/ci.yml` |
| User install scripts | `crates/codegen/agent-tui-pager/scripts/install.{sh,ps1}` |
| In-app updater | `crates/codegen/agent-tui-update` |
| Release identity constants | `crates/codegen/agent-tui-update/src/version.rs` |

Canonical GitHub repo: **`jasonkneen/agent-tui`**  
Default branch (for raw install script URLs): **`fork/agent-tui`**  
Release assets: `agent-tui-{version}-{os}-{arch}[.exe]`  
npm (optional): `@agent-tui/agent-tui`

When changing release URLs, asset names, or repo id, update **all** of:

1. `version.rs` constants (`GH_RELEASE_REPO`, download/API bases, prefixes)
2. `install.sh` / `install.ps1` defaults and comments
3. `auto_update.rs` reinstall / manual-install hints
4. `RELEASING.md`, `FORK.md`, `README.md`, getting-started guide
5. `suggested/skills/install-agent-tui.md` (and release skill if present)

Do **not** re-point installers at `x.ai/cli` or `@xai-official/grok` for this fork.

## Build

```sh
cargo run -p agent-tui-bin
cargo build -p agent-tui-bin --release
# shipping profile (matches CI release):
cargo build --profile release-dist -p agent-tui-bin --features release-dist
```

## Docs map

| Audience | Doc |
|----------|-----|
| Users (install) | README, user-guide `01-getting-started.md` |
| Fork semantics | FORK.md |
| Maintainers (ship) | RELEASING.md |
| Agents (this file) | AGENTS.md |
