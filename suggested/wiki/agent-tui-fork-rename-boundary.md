# Agent TUI fork rename boundary — what was renamed vs what must stay xAI-named

This tree is a **packaging/product fork** of Grok Build (`grok-build`), branched from `dev-reviewed` @ `eb2eca1`, Apache-2.0 with upstream SpaceXAI copyright retained (`FORK.md`). Grok/xAI remain fully supported **as providers** — the fork changes packaging identity, never wire behavior.

## Renamed surfaces (product identity)

| Surface | Upstream | This fork |
|--|--|--|
| Product name | Grok Build | **Agent TUI** |
| Binary | `grok` / `xai-grok-pager` | **`agent-tui`** |
| Crates | `xai-grok-*`, leaf `xai-*` | **`agent-tui-*`** (all product *and* utility crates — worktree, ACP client lib, tools protocol, …) |
| Config home | `~/.grok` / `$GROK_HOME` | **`~/.agent-tui`** / `$AGENT_TUI_HOME` (legacy `$GROK_HOME` still accepted) |

## Frozen surfaces (provider wire contracts)

These stay upstream-named so auth, models, and backends keep working:

- **Auth method IDs**: `xai.api_key`, `grok.com`, `oidc`, `cached_token`.
- **Env credentials**: `XAI_API_KEY`, `GROK_CODE_XAI_API_KEY`.
- **Model IDs** in the xAI/Grok catalog — Grok models (`grok-build`, …) *and* Composer models (`composer-2-fast`, …). Composer is a **model name**, not a separate product or provider.
- **Production API endpoints** (`*.grok.com`, etc.).
- **API User-Agent shape** for xAI services (still `grok-shell/...` where required for server dashboards).
- **`X-XAI-Token-Auth` header value** must stay `xai-grok-cli` — the proxy rejects unknown values, producing a 401 / paywall loop. Never rename this to the product name.

The dividing line: **only provider wire contracts stay xAI-named — crate packages do not** (`FORK.md`).

## Terminology trap: two "composers"

The pager source also uses the English word "composer" for the **prompt input box**. That usage is unrelated to the Composer model family and is deliberately left alone — don't "fix" it during renames.

## Dual install with official `grok`

Both CLIs coexist by using different config homes: official Grok Build keeps `~/.grok`; Agent TUI uses `~/.agent-tui` / `$AGENT_TUI_HOME`. The fork still accepts legacy `$GROK_HOME` for migration.
