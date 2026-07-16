# Getting Started

Agent TUI is a terminal-based AI coding assistant from SpaceXAI. It runs as a TUI (Terminal User Interface) that understands your codebase, executes shell commands, edits files, searches the web, and manages tasks.

You can use it interactively as a full-screen TUI, run it headlessly for scripting and CI/CD, or integrate it into editors via the Agent Client Protocol (ACP).

---

## Installation

**Agent TUI** is a packaging fork of Grok Build. Prefer a prebuilt release, or build from source.

### Prebuilt (GitHub Releases)

```bash
curl -fsSL https://raw.githubusercontent.com/jasonkneen/agent-tui/fork/agent-tui/crates/codegen/agent-tui-pager/scripts/install.sh | bash
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/jasonkneen/agent-tui/fork/agent-tui/crates/codegen/agent-tui-pager/scripts/install.ps1 | iex
```

Useful variants:

```bash
# specific version
curl -fsSL …/install.sh | bash -s 0.1.220

# alpha channel
AGENT_TUI_CHANNEL=alpha curl -fsSL …/install.sh | bash
```

Maintainer release process: **`RELEASING.md`** at the repository root.
Releases: https://github.com/jasonkneen/agent-tui/releases

### Build from source

```bash
cargo run -p agent-tui-bin                 # debug
cargo build -p agent-tui-bin --release     # binary: target/release/agent-tui
```

Put the release binary on your `PATH` (example):

```bash
mkdir -p ~/.agent-tui/bin
cp target/release/agent-tui ~/.agent-tui/bin/
export PATH="$HOME/.agent-tui/bin:$PATH"
```

Or install the npm package wrappers (when published for this fork):

```bash
npm i -g @agent-tui/agent-tui
```

> **Note:** Official `https://x.ai/cli/install.sh` installs upstream Grok Build (`grok`), not this fork.

Verify the installation:

```bash
agent-tui --version
```

Config defaults to `~/.agent-tui` (override with `$AGENT_TUI_HOME`; legacy `$GROK_HOME` is still accepted).
---

## First Launch

Start Agent TUI by running:

```bash
agent-tui
```

On first launch, Agent TUI opens your browser to authenticate with grok.com. After you sign in, Agent TUI stores your credentials in `~/.agent-tui/auth.json`, where they persist across sessions. Agent TUI refreshes your credentials automatically and prompts you to sign in again when they can no longer be renewed.

If you prefer API key authentication (e.g., for CI/CD or environments without a browser), set the `XAI_API_KEY` environment variable instead:

```bash
export XAI_API_KEY="xai-..."
agent-tui
```

See [Authentication](02-authentication.md) for the full set of auth options including OIDC, external auth providers, and device code flow.

---

## Basic Interaction

Once authenticated, Agent TUI presents a full-screen TUI with two main areas:

- **Scrollback** -- the conversation history showing your prompts, the agent's responses, tool calls, file edits, and more.
- **Prompt** -- the input area at the bottom where you type messages.

Type a message and press `Enter` to send it. Agent TUI reads files, runs commands, and edits code as needed. Each tool run streams into the scrollback in real time.

Press `Tab` to move focus between the prompt and the scrollback. While a turn is running, `Ctrl+C` cancels it (or clears a non-empty draft first); `Esc` is a no-op mid-turn. Idle, press `Esc` twice within 800ms to clear a non-empty prompt, or (with an empty prompt and conversation messages) to open rewind — see [Keyboard Shortcuts](03-keyboard-shortcuts.md#escape). With the scrollback focused, use the arrow keys to select entries and to collapse or expand them. To navigate with `j`/`k` and fold with `h`/`l` instead, enable Vim mode.

### File References

Use `@` in your prompt to attach files:

```
@src/main.rs              # Attach a file
@src/main.rs:10-50        # Attach lines 10-50
@src/                     # Browse a directory
```

The `@` operator opens a fuzzy file picker. By default it respects `.gitignore` and hides dotfiles. Prefix with `!` to search hidden files:

```
@!.github                 # Search hidden files
@!.env                    # Attach a .env file
```

### Permissions

By default, Agent TUI asks for permission before executing shell commands or editing files. You can approve individually or toggle always-approve mode:

- Press `Ctrl+O` to toggle always-approve mode
- Use the `--yolo` flag at launch: `agent-tui --yolo`
- Type `/always-approve` in the prompt to toggle the mode

---

## Key Concepts

### Sessions

Every conversation is a **session**. Sessions are automatically saved to `~/.agent-tui/sessions/` and can be resumed later. Each session tracks the full conversation history, tool calls, file edits, and task state.

- Start a new session: `Ctrl+N` or `/new`
- Resume a previous session: `/resume` in the TUI, or `--resume <ID>` from the CLI
- Continue the most recent session: `agent-tui -c`

### Scrollback

The scrollback is the main display area. It shows:

- **User prompts** -- your messages, rendered as sticky headers
- **Agent messages** -- the agent's responses with full markdown rendering and syntax highlighting
- **Thinking blocks** -- Agent TUI's reasoning process (collapsible)
- **Tool calls** -- file edits (with inline diffs), command executions, search results, and more
- **Task lists** -- TODO items tracking progress

Collapse or expand the selected entry with the `Left`/`Right` arrow keys (or `h`/`l` and `e` in Vim mode). In Vim mode, press `y` to copy its content and `Y` to copy its metadata (for example, the command that ran). Press `Enter` to open it in the fullscreen viewer (in any mode).

### Tools

Agent TUI has built-in tools for:

| Tool | Description |
|------|-------------|
| `read_file` / `search_replace` | Read and edit files with line-precise changes |
| `grep` | Regex search across your codebase (powered by ripgrep) |
| `list_dir` | List directory contents |
| `run_terminal_command` | Execute shell commands |
| `web_search` / `web_fetch` | Search the web and fetch URLs |
| `todo_write` | Create and manage task lists |
| `spawn_subagent` | Spawn parallel subagent sessions |
| `memory_search` | Search cross-session memory |

Tools can be extended with [MCP servers](05-configuration.md#mcp-servers) for integrations like GitHub, databases, and more.

### Slash Commands

Type `/` in the prompt to access commands. These provide quick actions without writing a full prompt:

```
/model grok-build                 # Switch model
/compact                          # Compress conversation history
/always-approve                   # Toggle always-approve mode
/new                              # Start a new session
```

See [Slash Commands](04-slash-commands.md) for the complete reference.

---

## Common Launch Options

```bash
# Launch the interactive TUI and submit an initial prompt as the first turn
agent-tui "fix the failing auth test and run it"

# Initial prompt in a new git worktree. Use --worktree=<name> (with `=`) so the
# prompt isn't swallowed as the worktree name — `agent-tui -w "refactor module X"`
# would treat "refactor module X" as the worktree label, not the prompt.
agent-tui --worktree=feat "refactor module X"

# Base the worktree on a specific branch (e.g. main) instead of the current HEAD:
agent-tui -w --ref main "implement feature from main"


# Start in a specific project directory
agent-tui --cwd ~/projects/my-app

# Add project-specific rules
agent-tui --rules "Always use TypeScript. Prefer functional components."

# Auto-approve all tool executions
agent-tui --yolo

# Use a specific model
agent-tui -m grok-build

# Resume a previous session
agent-tui --resume <session-id>

# Continue the most recent session
agent-tui -c

# Experimental scrollback-native render mode. Sticky: plain `agent-tui` reopens in
# the mode last chosen via --minimal/--fullscreen (or /minimal//fullscreen).
agent-tui --minimal

# Back to the standard fullscreen TUI (and make it sticky again)
agent-tui --fullscreen

# Headless mode (for scripts)
agent-tui -p "Explain this codebase"
```

---

## Headless Mode

run agent-tui non-interactively for scripting, CI/CD, and automation:

```bash
agent-tui -p "Your prompt here"
```

Output formats:

| Format | Flag | Description |
|--------|------|-------------|
| `plain` | (default) | Human-readable text |
| `json` | `--output-format json` | Single JSON object with `text`, `stopReason`, `sessionId`, and `requestId` |
| `streaming-json` | `--output-format streaming-json` | NDJSON event stream for real-time processing |

Example CI/CD usage:

```bash
agent-tui -p "Review changes for bugs" --output-format json --yolo | jq -r '.text'
```

---

## Project Rules (AGENTS.md)

Add per-project instructions by creating an `AGENTS.md` file in your repository. Agent TUI reads these files and injects their contents as a project-instructions message at the start of the conversation:

```
~/.agent-tui/AGENTS.md           # Global rules (apply to all projects)
<repo-root>/AGENTS.md       # Repository-level rules
<cwd>/AGENTS.md             # Directory-level rules (highest priority)
```

Deeper files take precedence. Agent TUI also reads `CLAUDE.md` files for compatibility.

---

## Where to Go Next

| Document | What You Will Learn |
|----------|-------------------|
| [Authentication](02-authentication.md) | Browser login, API keys, OIDC, external auth, device code flow |
| [Keyboard Shortcuts](03-keyboard-shortcuts.md) | Complete reference for all key bindings |
| [Slash Commands](04-slash-commands.md) | All available `/` commands |
| [Configuration](05-configuration.md) | config.toml, pager.toml, environment variables |
