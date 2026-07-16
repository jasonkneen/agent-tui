# Agent TUI

Bring Agent TUI into your terminal. Fast, flicker-free CLI built for plans, subagents, and parallel work.

**[GitHub](https://github.com/jasonkneen/agent-tui)** | **[Releases](https://github.com/jasonkneen/agent-tui/releases)** | **[Docs](https://github.com/jasonkneen/agent-tui/tree/fork/agent-tui/crates/codegen/agent-tui-pager/docs/user-guide)**

## Install

Preferred — GitHub Releases installer:

```bash
curl -fsSL https://raw.githubusercontent.com/jasonkneen/agent-tui/fork/agent-tui/crates/codegen/agent-tui-pager/scripts/install.sh | bash
```

Or npm (when this package is published):

```bash
npm i -g @agent-tui/agent-tui
```

Or build from source:

```bash
cargo build -p agent-tui-bin --release
# binary: target/release/agent-tui
```

> Official `https://x.ai/cli/install.sh` installs upstream Grok Build (`grok`), not Agent TUI.
> Maintainer publish steps: repo root `RELEASING.md`.


## Get Started

```bash
# Launch the interactive TUI
agent-tui

# Run a single task
agent-tui -p "Explain this codebase"
```

On first launch, Agent TUI opens your browser to authenticate. For CI or headless environments, use an API key from [console.x.ai](https://console.x.ai):

```bash
export XAI_API_KEY="xai-..."
```

## Update

```bash
agent-tui update
```

Or if installed via npm:

```bash
npm i -g @agent-tui/agent-tui@latest
```

## Supported Platforms

| Platform | Architecture |
|---|---|
| macOS | Apple Silicon (arm64) |
| Linux | x86_64, arm64 |
| Windows | x86_64 |

## Documentation

For full documentation including configuration, MCP servers, custom models, headless mode, agent mode, and more, visit [docs.x.ai/build/overview](https://docs.x.ai/build/overview).

## Feedback

Run `/feedback` inside Agent TUI to report issues or send feedback directly.
