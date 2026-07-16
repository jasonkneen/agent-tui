<div align="center">

# Agent TUI (`agent-tui`)

**Agent TUI** is a terminal-based AI coding agent — a packaging fork of SpaceXAI's
Grok Build. It runs as a full-screen TUI that understands your codebase, edits
files, executes shell commands, searches the web, and manages long-running tasks
— interactively, headlessly for scripting/CI, or embedded in editors via the
Agent Client Protocol (ACP).

Grok / xAI remain fully supported as **providers** (login via Grok.com or
`XAI_API_KEY`). See [FORK.md](FORK.md) for fork details and what was renamed.

[Building from source](#building-from-source) ·
[Documentation](#documentation) ·
[Repository layout](#repository-layout) ·
[License](#license)

</div>

---

## Building from source

Requirements:

- **Rust** — the toolchain is pinned by [`rust-toolchain.toml`](rust-toolchain.toml);
  `rustup` installs it automatically on first build.
- **protoc** — proto codegen resolves [`bin/protoc`](bin/protoc) (a
  [dotslash](https://dotslash-cli.com) launcher) or falls back to a `protoc` on
  `PATH` / `$PROTOC`.
- macOS and Linux are supported build hosts; Windows builds are best-effort
  and not currently tested from this tree.

```sh
cargo run -p agent-tui-bin              # build + launch the TUI
cargo build -p agent-tui-bin --release  # release binary: target/release/agent-tui
cargo check -p agent-tui-bin            # fast validation
```

On first launch it opens your browser to authenticate (Grok.com) or use
`XAI_API_KEY` — see the
[authentication guide](crates/codegen/agent-tui-pager/docs/user-guide/02-authentication.md).

Config defaults to `~/.agent-tui` (override with `$AGENT_TUI_HOME`; legacy
`$GROK_HOME` is still accepted).

## Documentation

The user guide ships with the pager crate:
[`crates/codegen/agent-tui-pager/docs/user-guide/`](crates/codegen/agent-tui-pager/docs/user-guide/)
— getting started, keyboard shortcuts, slash commands, configuration, theming,
MCP servers, skills, plugins, hooks, headless mode, sandboxing, and more.

Upstream product docs (Grok Build): [docs.x.ai/build/overview](https://docs.x.ai/build/overview).

## Repository layout

| Path | Contents |
|------|----------|
| `crates/codegen/agent-tui-bin` | Composition-root package; builds the `agent-tui` binary |
| `crates/codegen/agent-tui-pager` | The TUI: scrollback, prompt, modals, rendering |
| `crates/codegen/agent-tui-shell` | Agent runtime + leader/stdio/headless entry points |
| `crates/codegen/agent-tui-tools` | Tool implementations (terminal, file edit, search, ...) |
| `crates/codegen/agent-tui-workspace` | Host filesystem, VCS, execution, checkpoints |
| `crates/codegen/...` | The rest of the CLI crate closure (config, MCP, markdown, sandbox, ...) |
| `crates/common/`, `crates/build/`, `prod/mc/` | Small shared leaf crates pulled in by the closure |
| `third_party/` | Vendored upstream source (Mermaid diagram stack) |

> [!IMPORTANT]
> The root `Cargo.toml` (workspace members, dependency versions, lints,
> profiles) is **generated** — treat it as read-only when syncing from upstream.
> Prefer editing per-crate `Cargo.toml` files. This fork edits the root member
> list for the `agent-tui-*` renames.

## License

Apache-2.0. See [LICENSE](LICENSE) and [THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES).
Upstream copyright: SpaceXAI. This fork retains attribution.
