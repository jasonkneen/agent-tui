# Agent notes — Agent TUI fork

This repository is a packaging fork of SpaceXAI Grok Build. Product binary is
**`agent-tui`**, config home is **`~/.agent-tui`**. Provider wire contracts
(xAI / Grok.com) stay upstream-named — see [FORK.md](FORK.md).

## Architecture law: ONE CORE + ADDONS

**One core TUI.** Runtimes are **addons**. Products are **profiles** (brand +
which addons are on), not forks.

| Layer | What | Code / config |
|-------|------|----------------|
| **Core** | Pager, sessions, slash, tools UI, permissions | `agent-tui-pager`, shell |
| **Addons** | grok · codex · claude · lazar · hermes | `runtime_addon` + `*-runtime` crates |
| **Product** | Brand, default addon, lock, allow-list | `product_profile` + `product.toml` / argv[0] |

Full write-up: **[docs/CORE_AND_ADDONS.md](docs/CORE_AND_ADDONS.md)**.  
**Zero core duplication:** one `agent-tui` binary; product names are symlinks
(`scripts/link-product-bins.sh`).

```
cargo build -p agent-tui-bin && ./scripts/link-product-bins.sh

./target/debug/agent-tui          # all providers; /runtime switches
./target/debug/agent-multi        # product=all (same inode)
./target/debug/grok|codex|claude|hermes|lazartui
```

## Do not break

- Auth method IDs, `XAI_API_KEY`, model IDs, API endpoints
- `X-XAI-Token-Auth: xai-grok-cli` header value (proxy rejects renames)

## Runtime addons (not raw OAuth)

Prefer **vendor harnesses + local login**, not Agent TUI OAuth:

| Addon | Runtime | Auth |
|--------|---------|------|
| Claude | **Claude CLI harness** (`claude -p` + resume) | Claude Code login |
| Codex | **`codex app-server`** (JSON-RPC; stream notifications) | `~/.codex` ChatGPT login |
| Grok | Existing sampler HTTP SSE | Existing OIDC / API key |
| Lazar | **`lazar -p` stream-json** (spawn-per-turn; `agent-tui-lazar-runtime`) | Kernel providers (`lazar-env.sh` / `LAZAR_MODEL`) |
| Hermes | **`hermes chat -q -Q`** (spawn-per-turn; `agent-tui-hermes-runtime`) | Hermes config / auth (`~/.hermes`) |

- Detect helpers: `agent_tui_shell::auth::local_cli` (Claude first)
- Codex app-server client: `agent-tui-codex-runtime` (`CodexRuntimePool`, warm + idle timeout)
- Lazar kernel client: `agent-tui-lazar-runtime` (`LazarRuntimePool`; providers stay in the kernel)
- Vendor design: [docs/LOCAL_CLI_AUTH.md](docs/LOCAL_CLI_AUTH.md)
- **Lazar addon:** `source ~/lazar/workspace/lazar-env.sh` then `/runtime lazar`
- **Lazar product (core skinned):** `AGENT_TUI_PRODUCT=lazar` or `./scripts/lazartui-agent.sh` (`docs/product.lazar.example.toml`)
- **Lazar parity eval:** `bash crates/codegen/agent-tui-lazar-runtime/scripts/parity-eval.sh`
- **Go `lazartui` is not in this repo** — `~/lazar/workspace/tui/` (presentation only; kernel is the addon)
- Do **not** rename ACP method IDs (`xai.api_key`, `grok.com`, …)
- Do **not** send third-party tokens to the Grok chat proxy
- Do **not** force Claude/Codex/Lazar through `SamplerConfig` HTTP — use an addon bridge

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
