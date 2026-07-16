# Agent TUI theme catalog and the truecolor quantization constraint

Agent TUI draws all TUI colors from a central theme, switchable at runtime, with an `auto` option (alias `system`) that follows the OS light/dark appearance (`crates/codegen/agent-tui-pager/docs/user-guide/06-theming.md`).

## The operational constraint: truecolor

Most themes require a truecolor terminal. On 256-color or 16-color terminals their palettes are quantized and "lose their character." Only two themes are documented as quantization-safe:

- **GrokNight** (default) — "survives quantization cleanly on 256-color and 16-color terminals."
- **GrokDay** — the light counterpart, also truecolor-optional.

When recommending or scripting a theme for CI captures, SSH sessions, or minimal terminals, pick from these two; everything else assumes truecolor.

## Catalog (names are case-insensitive)

| Theme | Config-name aliases | Truecolor |
|-------|--------------------|-----------|
| GrokNight (default, dark) | `groknight`, `grok-night`, `dark` | No |
| GrokDay (light) | `grokday`, `grok-day`, `light`, `day` | No |
| TokyoNight | `tokyonight`, `tokyo-night`, `tokyo` | Yes |
| RosePineMoon | `rosepine`, `rose-pine`, `rosepine-moon`, `rose-pine-moon` | Yes |
| OscuraMidnight | `oscura`, `oscura-midnight` | Yes |
| OpenCode | `opencode`, `open-code` | Yes |
| Vercel | `vercel`, `geist` | Yes |
| Copilot | `copilot`, `github-copilot`, `github` | Yes |
| NERV | `nerv`, `evangelion`, `eva`, `unit-01` | Yes |
| Catppuccin | `catppuccin`, `catppuccin-mocha`, `mocha` | Yes |
| Nord | `nord` | Yes |
| Gruvbox | `gruvbox`, `gruvbox-dark` | Yes |
| Dracula | `dracula` | Yes |
| Auto (follows system) | `auto`, `system` | — |

Note the generic aliases: `dark` resolves to GrokNight and `light`/`day` to GrokDay, so config files using the generic names track the fork's defaults rather than a specific third-party palette.

Beyond theme choice, scrollback layout, animations, and block styling are adjusted through the configuration files — see the theming chapter of the user guide for those keys.