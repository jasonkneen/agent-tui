<div align="center">

# Agent TUI (`agent-tui`)

**Agent TUI** is a terminal-based AI coding agent — a packaging fork of SpaceXAI's
Grok Build. It runs as a full-screen TUI that understands your codebase, edits
files, executes shell commands, searches the web, and manages long-running tasks
— interactively, headlessly for scripting/CI, or embedded in editors via the
Agent Client Protocol (ACP).

Grok / xAI remain fully supported as **providers** (login via Grok.com or
`XAI_API_KEY`). See [FORK.md](FORK.md) for fork details and what was renamed.

[Install](#install-a-prebuilt-release) ·
[Building from source](#building-from-source) ·
[Documentation](#documentation) ·
[Releasing](RELEASING.md) ·
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
- macOS, Linux, and Windows are supported build hosts.

### Windows

Install one of the following so proto codegen can find `protoc`:

```powershell
cargo install dotslash          # resolves bin/protoc automatically, or
choco install protoc            # any protoc on PATH also works
```

A DotSlash launcher is a JSON file that relies on a `#!` line, which Windows
cannot execute directly, so `bin/protoc` is resolved via `dotslash -- fetch`
rather than run in place. Everything else is automatic: the MSVC linker
settings the build needs (an 8 MiB main-thread stack, and PDB symbol
truncation) are applied by `crates/codegen/agent-tui-bin/build.rs`.

Build hosts other than `x86_64`/`aarch64` MSVC are untested. The interactive
TUI has had less exercise on Windows than the CLI subcommands.

```sh
cargo run -p agent-tui-bin              # build + launch the TUI
cargo build -p agent-tui-bin --release  # release binary: target/release/agent-tui
cargo check -p agent-tui-bin            # fast validation
```

### Install a prebuilt release

```sh
# macOS / Linux — latest stable from GitHub Releases
curl -fsSL https://raw.githubusercontent.com/jasonkneen/agent-tui/fork/agent-tui/crates/codegen/agent-tui-pager/scripts/install.sh | bash
```

```powershell
# Windows
irm https://raw.githubusercontent.com/jasonkneen/agent-tui/fork/agent-tui/crates/codegen/agent-tui-pager/scripts/install.ps1 | iex
```

| | |
|--|--|
| Repo | [jasonkneen/agent-tui](https://github.com/jasonkneen/agent-tui) |
| Releases | [github.com/jasonkneen/agent-tui/releases](https://github.com/jasonkneen/agent-tui/releases) |
| Pin version | `bash -s 0.1.220` |
| Alpha channel | `AGENT_TUI_CHANNEL=alpha … \| bash` |
| Maintainer ship process | **[RELEASING.md](RELEASING.md)** |
| Fork / rename notes | [FORK.md](FORK.md) |

On first launch it opens your browser to authenticate (Grok.com) or use
`XAI_API_KEY` — see the
[authentication guide](crates/codegen/agent-tui-pager/docs/user-guide/02-authentication.md).

Config defaults to `~/.agent-tui` (override with `$AGENT_TUI_HOME`; legacy
`$GROK_HOME` is still accepted).

## Documentation

| Doc | Audience |
|-----|----------|
| [RELEASING.md](RELEASING.md) | Maintainers — cut tags, CI release, npm, troubleshooting |
| [FORK.md](FORK.md) | What differs from upstream Grok Build |
| [AGENTS.md](AGENTS.md) | Automation / coding-agent constraints |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Upstream vs fork contribution notes |
| [User guide](crates/codegen/agent-tui-pager/docs/user-guide/) | End users — auth, shortcuts, config, MCP, skills, … |

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
