# Agent TUI

Bring Agent TUI into your terminal. Fast, flicker-free CLI built for plans, subagents, and parallel work.

**[Homepage](https://x.ai/cli)** | **[Documentation](https://docs.x.ai/build/overview)**

## Install

```bash
# Preferred for this fork: build from source
cargo build -p agent-tui-bin --release
# binary: target/release/agent-tui
```

Or install with npm (when published):

```bash
npm i -g @agent-tui/agent-tui
```

> Official `https://x.ai/cli/install.sh` installs upstream Grok Build (`grok`), not Agent TUI.


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
