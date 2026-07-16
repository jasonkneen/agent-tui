---
name: install-agent-tui
description: Install Agent TUI from prebuilt releases or from source (pinned Rust toolchain, dotslash protoc), put the binary on PATH, and dual-install alongside the official grok CLI using separate config homes.
---

# Install Agent TUI

Agent TUI (`agent-tui`) is the packaging fork of Grok Build — a terminal AI coding agent usable interactively, headlessly for CI, or via ACP (`crates/codegen/agent-tui-pager/docs/user-guide/01-getting-started.md`).

## Option A: prebuilt release

Repo: [jasonkneen/agent-tui](https://github.com/jasonkneen/agent-tui) · default branch for scripts: `fork/agent-tui`.

```bash
curl -fsSL https://raw.githubusercontent.com/jasonkneen/agent-tui/fork/agent-tui/crates/codegen/agent-tui-pager/scripts/install.sh | bash
# pin:  … | bash -s 0.1.220
# alpha: AGENT_TUI_CHANNEL=alpha … | bash
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/jasonkneen/agent-tui/fork/agent-tui/crates/codegen/agent-tui-pager/scripts/install.ps1 | iex
```

To **cut** a release (maintainers), use skill `release-agent-tui` / root **`RELEASING.md`**.

## Option B: build from source

Requirements (`README.md`):
- **Rust** — pinned by `rust-toolchain.toml`; `rustup` installs it automatically on first build.
- **protoc** — codegen resolves `bin/protoc` (a dotslash launcher) or falls back to `protoc` on `PATH` / `$PROTOC`.
- macOS and Linux are supported hosts; Windows builds are best-effort and untested from this tree.

```bash
cargo run -p agent-tui-bin                 # build + launch the TUI (debug)
cargo build -p agent-tui-bin --release     # binary: target/release/agent-tui
cargo check -p agent-tui-bin               # fast validation
```

Put the release binary on `PATH`:

```bash
mkdir -p ~/.agent-tui/bin
cp target/release/agent-tui ~/.agent-tui/bin/
export PATH="$HOME/.agent-tui/bin:$PATH"
```

## Dual install with official `grok`

The fork and the official CLI coexist because they use different config homes (`FORK.md`): official Grok Build keeps `~/.grok` / `$GROK_HOME`; Agent TUI uses `~/.agent-tui` / `$AGENT_TUI_HOME` (legacy `$GROK_HOME` is still accepted). Auth is unchanged from upstream — log in via Grok.com or set `XAI_API_KEY`.

## Verify

Launch `agent-tui`; the full-screen TUI should start and accept a provider login. For scripting/CI, run it headlessly per the getting-started guide.
