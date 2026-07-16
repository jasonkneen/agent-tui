# Agent TUI fork

This tree is a packaging/product fork of [Grok Build](https://x.ai/cli) (`grok-build`).

| | Upstream (Grok Build) | This fork |
|--|----------------------|-----------|
| Product name | Grok Build | **Agent TUI** |
| Binary | `grok` / `xai-grok-pager` | **`agent-tui`** |
| Crates | `xai-grok-*`, leaf `xai-*` | **`agent-tui-*`** (all product + utility crates) |
| Config home | `~/.grok` / `$GROK_HOME` | **`~/.agent-tui`** / `$AGENT_TUI_HOME` (legacy `$GROK_HOME` still accepted) |
| Providers | Grok.com, xAI API | **Unchanged** — same wire IDs and endpoints |
| Models | Grok, Composer, … (xAI catalog) | **Unchanged** — model ids like `grok-build`, `composer-2-fast` stay as-is |

## Fork baseline

- Branched from: `dev-reviewed` @ `eb2eca1` (see `git log --oneline -1` on first fork commit).
- License: Apache-2.0; copyright notices from SpaceXAI / upstream retained.

## What we deliberately did **not** change

These remain stable so auth, models, and backends keep working:

- Auth method IDs: `xai.api_key`, `grok.com`, `oidc`, `cached_token`
- Env credentials: `XAI_API_KEY`, `GROK_CODE_XAI_API_KEY`
- **Model IDs** in the xAI/Grok catalog, including **Grok** models (`grok-build`, …) and **Composer** models (`composer-2-fast`, …) — Composer is a model name, not a separate product or provider
- Production API endpoints (`*.grok.com`, etc.)
- API User-Agent shape for xAI services (still `grok-shell/...` where required for server dashboards)
- Leaf utility crates were also renamed from `xai-*` → `agent-tui-*` (worktree, ACP client lib, tools protocol, etc.). Only **provider wire contracts** stay xAI-named — not crate packages.

> Note: pager source also uses the English word “composer” for the **prompt input box**. That is unrelated to the Composer model family and is left alone.

## Dual install with official `grok`

Use different config homes:

```sh
# Official Grok Build (default)
# ~/.grok

# This fork (default)
# ~/.agent-tui

# Optional: reuse your existing Grok config tree
export AGENT_TUI_HOME="$HOME/.grok"
```

## Build

```sh
cargo run -p agent-tui-bin
cargo build -p agent-tui-bin --release   # target/release/agent-tui
```

## Migration from `~/.grok`

```sh
cp -a ~/.grok ~/.agent-tui   # optional one-time copy
agent-tui
```
