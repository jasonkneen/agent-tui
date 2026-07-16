---
name: set-agent-tui-theme
description: Switch or configure the Agent TUI theme — at runtime or via config files — resolving alias names, following the OS with auto/system, and picking a quantization-safe theme (GrokNight/GrokDay) for CI captures, SSH, or 256-color terminals.
---

# Set or script an Agent TUI theme

Agent TUI draws all TUI colors from a central theme, switchable **while it is running**, with thirteen built-in themes plus `auto` (alias `system`) that follows the OS light/dark appearance (`crates/codegen/agent-tui-pager/docs/user-guide/06-theming.md`).

## 1. Decide: does the target terminal have truecolor?

This is the one constraint that matters before picking a name:

- **Truecolor terminal** (modern local terminals): any theme in the catalog works.
- **256-color or 16-color** (CI captures, some SSH sessions, minimal terminals): only two themes are documented as quantization-safe — **GrokNight** (default dark; "survives quantization cleanly on 256-color and 16-color terminals") and **GrokDay** (light). Every other theme "loses its character" when quantized.

When scripting a theme for CI or a capture pipeline, always pick from those two.

## 2. Resolve the theme name

Theme names are **case-insensitive** and most themes accept several config-name aliases (full table: `suggested/wiki/agent-tui-theme-catalog.md`). The load-bearing aliases:

- `dark` → GrokNight, `light` / `day` → GrokDay — the generic names track the fork's defaults rather than pinning a third-party palette, so prefer them in shared config files.
- `auto` / `system` → follow the OS appearance (switches between light and dark automatically).
- Third-party palettes go by their obvious names and variants, e.g. `tokyonight`/`tokyo`, `catppuccin`/`mocha`, `rose-pine-moon`, `gruvbox`, `dracula`, `nord`, `copilot`/`github`, `nerv`/`evangelion`.

## 3. Apply it

- **Interactively:** switch the theme from inside the running TUI — no restart needed (the theming chapter covers the in-app switch).
- **Persistently:** set the theme key in the configuration files, using an alias from step 2. The same config files also control scrollback layout, animations, and block styling — see the theming chapter (`docs/user-guide/06-theming.md`) for those keys; they are separate from theme choice.

## Verify

- The TUI repaints in the new palette immediately (runtime switch) or on next launch (config).
- With `auto`, toggle the OS light/dark appearance and confirm the TUI follows.
- For a CI/SSH target, confirm the pick is GrokNight or GrokDay; if a truecolor theme was requested there, warn that it will quantize rather than silently applying it.
