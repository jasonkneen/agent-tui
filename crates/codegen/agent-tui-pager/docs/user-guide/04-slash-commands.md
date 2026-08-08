# Slash Commands

Type `/` in the prompt to access commands. Each command runs an action immediately and autocompletes as you type.

Commands come from two places: **shell builtins**, handled by the agent backend (agent-tui-shell), and **pager builtins**, handled by the pager frontend (agent-tui-pager). Both show up in the same menu, and any enabled skill with `user-invocable: true` appears there too. If a skill reuses a built-in name such as `login`, the built-in keeps `/login` and the skill stays available as `/plugin-name:login` — the menu badges both so the collision is visible.

Every command below lists its aliases where it has them. A few commands only appear when a feature or session state enables them; those cases are called out inline. The menu is also filtered by render mode — see [`/minimal` and `/fullscreen`](#minimal-and-fullscreen).

---

## Session Management

### `/new`

Start a new session, clearing the current conversation.

```
/new
```

Aliases: `/clear`

### `/resume`

Open the session picker to load a previous session from disk.

```
/resume
```

### `/dashboard`

Open the [Agent Dashboard](23-dashboard.md): live roster of top-level sessions in this pager (peek, reply, dispatch, pin, rename, stop, attach). Aliases: `/agents-dashboard`, `/sessions`.

Not `/config-agents` (alias `/agents`), which manages agent *definitions* and personas. Hidden in minimal mode; disable with `GROK_AGENT_DASHBOARD=0` or `[dashboard].enabled = false`.

### `/compact [context]`

Compress conversation history to save context window space. Optionally specify what to preserve.

```
/compact
/compact keep the auth implementation details
```

When the context window fills up, Agent TUI auto-compacts at 85% usage (configurable via `[session] auto_compact_threshold_percent` in config.toml).

### `/context`

Show context window usage and session stats: a categorical breakdown (system prompt, messages, reasoning/overhead, free), plus informational rows for tool definitions, the skills listing, and MCP server announcements with their estimated token cost.

```
/context
```

### `/session-info`

Show session details including model, turn count, and context usage.

```
/session-info
```


### `/rewind` (alias: `/undo`)

Roll the conversation back to an earlier turn and discard everything after it. `/undo` is the same command.

```
/fork
```

### `/rewind`

Rewind the conversation to an earlier turn, discarding everything after it.

```
/rewind
```

### `/copy`

Copy the most recent response to the clipboard. Pass a number to copy the Nth-latest response.

```
/copy
/copy 2
```

### `/export`

Export the current conversation to a file or the clipboard.

```
/export
```

### `/quit`

Quit the application.

```
/quit
```

Aliases: `/exit`

### `/home`

Exit the current session and return to the welcome screen.

```
/home
```

Aliases: `/welcome`

### `/delete`

Delete the current session's history. Confirms first. Returns to the welcome screen, or to the dashboard when you opened the session from the dashboard.

To delete a session you are not in, open `/resume` or the welcome session list and press `d` then `y`. On the dashboard, press `Ctrl+X` twice or click `[✗]`.

### `/rename`

Rename the current session.

```
/rename new session title
```

Aliases: `/title`

---

## Model and Mode

### `/runtime [vendor]`

Switch which **runtime addon** serves turns (not a model pick). Lists readiness
when called with no argument. Architecture: [ONE CORE + ADDONS](../../../../../docs/CORE_AND_ADDONS.md).

| Command | Addon |
|---------|--------|
| `/runtime grok` | Built-in xAI agent (default on platform) |
| `/runtime codex` | Local `codex app-server` (`~/.codex` auth) |
| `/runtime claude` | Claude Code CLI harness |
| `/runtime lazar` | Local **lazar** kernel (`lazar -p`; providers stay in the kernel) |
| `/runtime hermes` | **Hermes** Agent CLI (`hermes chat -q`) |

```
/runtime
/runtime lazar
```

Aliases: `/provider`, `/rt`

Selection is stored in `runtime.toml` under the config home. For Lazar, source
`~/lazar/workspace/lazar-env.sh` first so keys and `LAZAR_MODEL` are set.

**Product skins (zero core duplication):** one binary `agent-tui`; names like
`grok` / `lazartui` / `hermes` are symlinks (`scripts/link-product-bins.sh`).
Single-product skins lock `/runtime` and brand the UI. Multi-provider:
`./agent-tui` or `./agent-multi`. See [CORE_AND_ADDONS.md](../../../../../docs/CORE_AND_ADDONS.md).

### `/model <name>`

Switch model **within the active runtime**. Accepts model IDs or display names
(case-insensitive). After `/runtime codex|claude|lazar`, the catalog comes from
that vendor’s harness (not a hardcoded list). For Grok reasoning models you can
also pass an effort level as a second argument:

```
/model grok-build
/model Agent TUI
/model Reasoning X high
```

Aliases: `/m`

### `/effort <level>`

Set reasoning effort on the **current** model without re-selecting it. Levels: `low`, `medium`, `high`, `xhigh`. Only works when the active model supports reasoning effort.

```
/effort high
/effort low
```

### `/always-approve` and `/auto`

True **toggles** for the permission mode — both stay in the completion menu, and
running the active mode again turns it off:

| Command | When off | When already on |
|---|---|---|
| `/always-approve` | Skip all permission prompts | Back to ask |
| `/auto` | Classifier approves safe tools (dangerous ones may still prompt) | Back to ask |

Running the other command while one mode is on **switches** modes (for example,
`/auto` while always-approve is on switches to auto).

`/auto` is only offered when the auto permission-mode feature is enabled. You
can also change mode with `Shift+Tab` (cycle), `Ctrl+O`, or `/settings`.

```
/always-approve
/auto
```

### `/multiline`

Toggle multiline input mode. When enabled, `Enter` inserts a newline and `Shift+Enter` (or `Alt+Enter`) sends the message. Mid-turn, bare `Enter` on an empty composer still force-sends the top queued follow-up (send now).

```
/multiline
```

Aliases: `/ml`

### `/history`

Open the prompt-history search: fuzzy-search this session's prompts, newest first — type to filter, press `Enter`/`Tab` to drop a match back into the prompt.

For quick recall, press `↑` on an empty prompt instead: the panel opens with your most recent prompt already filled into the input, `↑`/`↓` step through entries (each one lands in the input), `↓` at the newest entry closes the panel, and typing edits the recalled prompt in place.

```
/history
```

### `/compact-mode`

Toggle compact display mode. Reduces padding and visual spacing for denser output.

```
/compact-mode
```

### `/vim-mode`

Toggle vim-style scrollback keybindings (j/k, h/l, g/G, y/Y, …). When off
(default), bare-letter and `Shift+letter` keys in the scrollback focus the
prompt and type the character. Persists to `[ui].vim_mode` in `config.toml`.

```
/vim-mode
```

### `/minimal` and `/fullscreen`

Reopen the current session in the other render mode. `/minimal` (offered while you're in fullscreen) switches to the experimental scrollback-native mode; `/fullscreen` (offered while you're in minimal; alias `/full`) switches back to standard fullscreen mode. Both relaunch the pager on the same conversation for this session only — they don't touch `config.toml`, and the relaunch banner reminds you how to switch back. The `--minimal` / `--fullscreen` CLI flags are session-scoped the same way. To make plain `grok` open in a given mode by default, use `/settings` → **Default screen mode** or set `[ui] screen_mode`.

A handful of commands only work in one of the two modes, because the surface they drive doesn't exist in the other: `/find`, `/jump`, `/timeline`, `/theme`, `/tutorial`, `/workflows`, and `/dashboard` are fullscreen-only, while `/expand` and `/edit-prompt` are minimal-only. Those are hidden from the command menu and the palette in the mode they can't run in. If you type one out anyway, Grok says why — and points you at whichever is actually useful. When the other mode is the only way to get it, that's the mode switch: `/theme isn't available in minimal mode (minimal renders with your terminal's own palette). Run /fullscreen to switch this session.` When this mode already does the job another way, it names that instead: `/expand isn't available in fullscreen mode — press Tab to focus the scrollback, then → on the block.` Everything else works in both. Note that `--no-alt-screen` still counts as fullscreen here, so it keeps the fullscreen-only commands.

### `/plan`

Enter plan mode.

```
/plan [description]
```

### `/view-plan`

Open the current saved plan preview. Aliases: `/show-plan`, `/plan-view`.

```
/view-plan
```

---

## Memory

The `/flush`, `/dream`, and `/memory` commands require `--experimental-memory` or `GROK_MEMORY=1`. `/remember` is always available.

### `/memory`

Browse, view, and manage your saved memories. Pass `on` or `off` to enable or disable memory.

```
/memory
/memory off
```

Aliases: `/mem`

### `/flush`

Save current session knowledge to memory immediately. Triggers an LLM-generated summary of the session's most important content.

```
/flush
```

Use this when you want to preserve important context before compaction or at any point in a session.

### `/dream`

Run memory consolidation -- merge session logs into organized topics.

```
/dream
```

### `/remember`

Save a note to memory immediately, without waiting for an automatic summary.

```
/remember the staging deploy uses the eu-west cluster
```

---

## Hooks and Plugins

The `/hooks`, `/plugins`, `/marketplace`, and `/skills` commands open the same extensions modal on different tabs.

### `/hooks`

Open the extensions modal on the Hooks tab. From the modal you can view loaded hooks, add or remove custom hooks, and enable or disable them individually. The modal does not grant project trust -- see [10-hooks.md](10-hooks.md) for the trust model.

The shell also advertises individual `/hooks-list`, `/hooks-trust`, `/hooks-add`, `/hooks-remove`, and `/hooks-untrust` commands; in the pager these are folded into the `/hooks` modal.

### `/plugins`

Open the extensions modal on the Plugins tab. From the modal you can view installed plugins, install new ones from the marketplace, and manage trust.

```
/plugins
```

The shell additionally supports subcommands (`/plugins list`, `/plugins install <source>`, `/plugins uninstall <name>`, `/plugins update`, `/plugins reload`). In the pager, the modal does the same work visually.

### `/marketplace`

Open the extensions modal on the Marketplace tab to browse and install plugins.

```
/marketplace
```

### `/skills`

Open the extensions modal on the Skills tab to view installed skills.

```
/skills
```

---

## Media Generation

### `/imagine <description>`

Generate an image from a text description.

```
/imagine a golden sunset over a calm ocean with silhouetted palm trees
```

### `/imagine-video <description>`

Generate a video from an image or text description. Plans shots, generates source images, and animates them with `image_to_video`.

```
/imagine-video a cat playing piano in a jazz club
```

---

## Scheduling

### `/loop [interval] <prompt>`

Run a prompt on a recurring interval. Specify the interval as `30m`, `1 hour`, or `every 2 days`. If you omit it, Agent TUI prompts you.

```
/loop 30m check deploy status
/loop check deploy status every hour
```

Interval format: `Ns` (seconds, min 60), `Nm` (minutes), `Nh` (hours), `Nd` (days). Intervals under 60 seconds are raised to the 60-second minimum.

Recurring tasks auto-expire after 7 days. Cancel with `scheduler_delete` (the job ID is provided when the loop is created).

---

## Other

### `/goal`

Set, manage, or check an autonomous goal. Agent TUI works toward the objective across turns and reports progress.

```
/goal Migrate the auth module to the new API
/goal status
```

Arguments: `<objective>`, `status`, `pause`, `resume`, or `clear`. **Availability:** appears only when the goal feature is enabled and the `update_goal` tool is in the session toolset.

### `/deep-research <query>`

Kick off a background research workflow. It plans a bounded set of questions, gathers structured claims with source evidence, cross-checks each claim on an independent verifier shard, and renders only the claims that survive, with their verified source locators. Failed shards, dropped claims, and researcher uncertainties are reported as coverage limitations, and the report is marked **Partial** whenever any remain.

```
/deep-research Compare the migration risks of PostgreSQL 17 and MySQL 9
```

The command returns right away — follow progress in `/workflows`, and the final report appears in the conversation on its own.

Model-launched workflows may set `agent_budget` on the `workflow` tool. It's an absolute cumulative cap on logical child-agent calls: every `agent()` call and every item in a `parallel()` panel spends one slot, while schema-correction retries don't. The default is 128, explicit values run 1–1,024, and a panel that would cross the remaining budget is rejected before any of its children launch. Separately, a host-configured cap (32 by default) bounds how many children run at a time per run; larger panels queue and still act as a barrier. `budget()` reports the cap as `total`, admitted calls as `spent`, `reserved` (always zero), and `remaining`. Named slash launches use the default budget.

### `/workflow`

Switch the TUI color theme.

```
/theme
```

Project workflows live in `.grok/workflows/*.rhai`; user workflows live in `~/.grok/workflows/*.rhai`. A same-process pause/resume continues the original immutable script, args, and `agent_budget` cap from committed host-call results — to iterate, edit the returned script copy and launch it as a new run.

A budget-limited run is different: it only resumes through a model/tool resume request that supplies an `agent_budget` above the admitted agent count. A bare `/workflow resume <name>` can't raise the cap, so it rejects budget-limited runs. Runs interrupted by a process restart aren't resumed at all, because external effects have no stable cross-process identity. And resume is not exactly-once: an external effect whose result wasn't committed before a same-process pause can run again.

### `/workflows`

Open the live workflows **run** dashboard — active and retained runs, not a catalog of saved definitions. Each row shows the run's display name, phase, agent roster, progress, and result. Inside a run's detail view, `p` pauses, `r` resumes an ordinary pause, and `x` stops. Budget-limited runs can't bare-resume: `r` returns the shell's rejection (raise the cap with a model/tool resume that passes a higher `agent_budget`), while `x` still stops. `s` saves the run's script, but it's hidden for known built-ins and numbered duplicate handles — for those, choose a new unique `meta.name` and save the edited script explicitly.

---

## Other

### `/theme`

Switch the color theme. Alias: `/t`.

### `/feedback [message]`

Report an issue or send feedback. A message sends immediately. With none, a pane opens for a longer report: `Enter` sends, `Esc` discards.

```
/feedback
/feedback Something isn't working correctly
```

### `/btw`

Send an aside to the agent without interrupting the current task.

```
/btw also check the error handling
```

### `/mcps`

Open the MCP servers management modal.

```
/mcps
```

### `/terminal-setup`

Show terminal capability detection and setup info — including color level, which themes are available, clipboard routes, and fix instructions for common issues (truecolor, tmux clipboard, keyboard protocol).

```
/terminal-setup
```

Aliases: `/terminal-check`, `/terminal-info`

### `/release-notes`

View release notes for the current version.

```
/release-notes
```

Aliases: `/changelog`

### `/docs`

Browse the built-in How-to Guides, open the online Build docs, or jump straight to a guide by title. Aliases: `/howto`, `/guides`.

```
/docs
/docs web
/docs Getting Started
```

- Bare `/docs` (or `/docs how-to`) opens the How-to Guides picker
- `/docs web` opens https://docs.x.ai/build/overview in the browser
- `/docs <title>` opens a specific guide (case-insensitive title match)

Aliases: `/howto`, `/guides`

### `/import-claude`

Open the Claude settings import modal to bring over `~/.claude` settings: permissions, environment variables, MCP servers, hooks, and paths.

```
/import-claude
```

---

## Agents and Personas

### `/config-agents`

Open the agents modal to view and manage agent definitions, set the default agent, and switch the active one.

```
/config-agents
```

Aliases: `/agents`

Not the live multi-session [Agent Dashboard](23-dashboard.md) (`/dashboard` / `Ctrl+\`).

### `/personas`

Manage personas -- create, edit, and delete personas. A subagent can apply a persona to shape its behavior.

```
/personas
```

---

## Account and Billing

### `/login`

Log in or re-authenticate with your account without leaving the session.

```
/login
```

### `/logout`

Log out and return to the login screen.

```
/logout
```

### `/usage`

View credit usage or manage billing.

```
/usage
```

### `/privacy`

Open Settings on **Coding data, retention, and training**, where you choose
**Opt in** or **Opt out**. Takes no arguments.

```
/privacy
```

This setting doesn't touch `[features] telemetry`, `trace_upload`, or your external OTEL settings — see [Monitoring Usage](24-monitoring-usage.md#related-settings). On team accounts only a team admin can change it, and admins can also enable or disable Zero Data Retention for the team ([how to enable ZDR](https://docs.x.ai/developers/faq/security#how-to-enable-zdr)). When the choice isn't yours to make, the row says so — `ZDR` or `· Admin Managed` — instead of opening the chooser.

---

## Configuration and UI

### `/settings`

Open the settings modal to view and change configuration interactively.

```
/settings
```

Aliases: `/config`, `/preferences`, `/prefs`

### `/timestamps`

Toggle message timestamps on or off.

```
/timestamps
```

---

## Skills as Slash Commands

Any enabled skill with `user-invocable: true` in its SKILL.md frontmatter appears as a slash command. (A skill turned off via `/skills` is not advertised.) For example, if you have a skill at `~/.agent-tui/skills/commit/SKILL.md`, you can invoke it with:

```
/commit fix typo in README
```

Skills from plugins also appear as slash commands. When multiple skills share the same name (across scopes), use the qualified form:

```
/local:commit      # Project-scoped skill
/user:commit       # User-scoped skill
```

Built-in commands always win the bare name. Name a skill "compact" and `/compact` still runs the built-in — the skill stays available as `/local:compact` (or `/acme:compact` for a plugin). Both appear in the slash menu: the built-in is tagged `built-in` and the skill is tagged `skill · local` / `skill · acme`.

---

## Autocomplete

The slash command menu supports fuzzy search. Start typing after `/` to filter available commands. The menu shows:

- Command name
- Description
- Argument hint (if the command accepts arguments)
- Source (builtin, skill scope, plugin name)

Press `Tab` or `Enter` to select a command from the autocomplete menu.
