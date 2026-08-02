# ONE CORE + ADDONS

**Law:** there is **one** Agent TUI core binary. Runtimes are **addons**. Product
names are **skins** (brand + which addons are on) — not separate codebases and
**not** separate linked copies of the TUI.

```text
┌──────────────────────────────────────────────────────────────┐
│  CORE  (single linked binary: agent-tui)                     │
│  pager · sessions · slash · tools UI · permissions · themes  │
└────────────────────────────┬─────────────────────────────────┘
                             │ turns · models · readiness
      ┌──────────┬───────────┼───────────┬──────────┬──────────┐
      ▼          ▼           ▼           ▼          ▼
   [grok]     [codex]    [claude]    [lazar]   [hermes]
   sampler    app-server CLI         kernel    hermes chat
   addon      addon      harness     -p        -q -Q
                             │
                             ▼
                  Product profile (skin + policy)
                  brand · default addon · lock · allow-list
                  from argv[0] symlink name or AGENT_TUI_PRODUCT
```

## Zero core duplication

| What | How |
|------|-----|
| **One artifact** | `cargo build -p agent-tui-bin` → `target/*/agent-tui` |
| **Product names** | Symlinks to that file (`scripts/link-product-bins.sh`) |
| **Identity** | `argv[0]` basename → product preset, unless `AGENT_TUI_PRODUCT` is set |

```sh
cargo build -p agent-tui-bin
./scripts/link-product-bins.sh              # target/debug skins
# cargo build -p agent-tui-bin --release && ./scripts/link-product-bins.sh release
```

| Name (symlink → `agent-tui`) | Product | Default addon | Locked? |
|------------------------------|---------|---------------|---------|
| `agent-tui` | Platform (default) | grok | no — all addons |
| `agent-multi` | All providers | grok | no — all addons |
| `grok` | Grok alone | grok | yes |
| `lazartui` | Lazar alone | lazar | yes |
| `codex` | Codex alone | codex | yes |
| `claude` | Claude alone | claude | yes |
| `hermes` | Hermes alone | hermes | yes |

Same inode for every name. No re-link of the pager per product.

```sh
./target/debug/agent-tui              # multi: /runtime grok|codex|claude|lazar|hermes
./target/debug/agent-multi            # product=all
./target/debug/grok
./target/debug/codex                  # needs: codex login
./target/debug/claude                 # needs: Claude Code login
./target/debug/hermes                 # needs: hermes on PATH + ~/.hermes config
source ~/lazar/workspace/lazar-env.sh && ./target/debug/lazartui
```

Override anytime: `AGENT_TUI_PRODUCT=hermes ./target/debug/agent-tui`  
(env wins over argv[0]).

## Core owns

- Scrollback, prompt, slash commands, sessions, permissions, themes, MCP, skills UI
- Routing: active addon → start turn / list models
- Persistence: `runtime.toml` under config home (`active`, per-addon model keys)

Config home: `$AGENT_TUI_HOME` → legacy `$GROK_HOME` → `~/.agent-tui`.  
Lazar product may use `~/.lazar` when that tree exists and home is unset.

## Addons own

| Addon | Crate | Turns | Models / auth |
|-------|--------|-------|----------------|
| **grok** | built-in sampler / ACP | HTTP SSE | sampler catalog; OIDC / `XAI_API_KEY` |
| **codex** | `agent-tui-codex-runtime` | warm app-server | `model/list`; `~/.codex` |
| **claude** | `agent-tui-claude-runtime` | `claude -p` + resume | CLI aliases; Claude Code login |
| **lazar** | `agent-tui-lazar-runtime` | `lazar -p` stream-json | kernel `LAZAR_MODEL`; `lazar-env.sh` |
| **hermes** | `agent-tui-hermes-runtime` | `hermes chat -q -Q` + `--resume` | `config.yaml` / `HERMES_MODEL` |

Addons never reimplement vendor OAuth when a local harness already exists.
Design detail: [LOCAL_CLI_AUTH.md](LOCAL_CLI_AUTH.md).

## Product profile (skin + policy)

File: `~/.agent-tui/product.toml` (or `AGENT_TUI_PRODUCT_FILE`, or named preset).

| Mode | Knobs | UX |
|------|--------|-----|
| **Platform / all** | default or `AGENT_TUI_PRODUCT=all` | Multi-addon; `/runtime` switches; `/model` per active addon |
| **Single-product** | `lock_runtime = true`, one addon in `addons` | Brand + locked runtime; `/model` = that addon only |
| **Primary + friends** | `default_runtime = X`, `lock_runtime = false`, multi `addons` | Brand X, still switchable |

```toml
# Single-product example — also docs/product.lazar.example.toml
id = "lazar"
name = "Lazar"
title_token = "lazar"
default_runtime = "lazar"
lock_runtime = true
addons = ["lazar"]              # preferred; alias: enabled_runtimes
```

```toml
# Kitchen-sink — docs/product.all.example.toml
id = "all"
name = "Agent TUI (all providers)"
default_runtime = "grok"
lock_runtime = false
addons = ["grok", "codex", "claude", "lazar", "hermes"]
```

Named presets (`AGENT_TUI_PRODUCT=…`):  
`agent-tui` / `platform` · `all` / `multi` · `grok` · `lazar` · `codex` · `claude` · `hermes`.

Example tomls: `docs/product.*.example.toml`.

## Commands

| Command | Meaning |
|---------|---------|
| `/runtime` `/provider` `/rt` | List / switch **addons** (blocked when product locks runtime) |
| `/model` | Catalog of the **active addon only** |

## Code map

| Concern | Location |
|---------|----------|
| Single binary entry + argv[0] skin | `crates/codegen/agent-tui-bin` (`lib.rs` + `main.rs`) |
| Symlink helper | `scripts/link-product-bins.sh` |
| Product brand + allow-list | `agent-tui-pager` `product_profile.rs` |
| Addon ids + turn routing | `runtime_backend.rs` |
| Addon catalog (meta) | `runtime_addon.rs` |
| Vendor harness detail | `docs/LOCAL_CLI_AUTH.md` |

## Non-goals

- Separate monorepos or fully linked binaries per product
- Hardcoding vendor model IDs into core
- Sending third-party tokens through the Grok chat proxy
- Putting product brand inside vendor kernels (Lazar/Hermes own their identity)

## Evolution

1. **Now:** one linked core + symlink skins + product profiles + five addons (shipped).
2. **Next:** `dyn` addon registry so new runtimes register without growing match arms.
3. **Optional:** release installers create product symlinks next to `agent-tui`.
