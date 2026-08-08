# Terminal Support and Troubleshooting

Grok Build runs as a full-screen TUI. It relies on terminal support for color,
clipboard, keyboard input, mouse input, and full-screen display. Terminals,
multiplexers, containers, and SSH sessions can handle these features differently.

## Diagnose and Fix Terminal Problems

Run `/doctor` in Grok to check the current session and see available fixes. If
Grok cannot start, run `grok doctor` in your shell. Use `grok doctor --json`
for a machine-readable report.

Doctor checks the terminal, multiplexer, color support, keyboard and newline
behavior, clipboard routes, and microphone availability when audio capture is
included. The in-app command can also check live session details such as
notification focus tracking and sandbox profile conflicts.

A report can contain issues or recommendations and still exit successfully.
`grok doctor --json` reports the same color capability when piped. Microphone
checks do not start recording, so Doctor cannot detect macOS permission failures
that appear only as silence during capture.

`/terminal-setup`, `/terminal-check`, and `/terminal-info` remain aliases for
`/doctor`.

When Doctor finds an explicit unhealthy tmux setting, `/doctor fix` lists the
available automatic fixes. Apply one named fix at a time, for example
`/doctor fix tmux-clipboard` or `grok doctor fix dcs-passthrough --yes`.
Doctor can persist these four tmux options:

- `terminal.tmux-clipboard` — `set -g set-clipboard on`
- `terminal.dcs-passthrough` — `set -wg allow-passthrough on`
- `terminal.tmux-extended-keys` — `set -g extended-keys on`
- `terminal.tmux-truecolor` — `set -as terminal-features ",*:RGB"`

A tmux fix edits only the persistent config on the computer hosting the affected
tmux server, including remote sessions. Plain tmux uses the real
`$HOME/.tmux.conf`; Byobu-tmux uses its effective `BYOBU_CONFIG_DIR` and refuses
to guess if that directory is unavailable or unsafe. Grok preserves the file's
line endings and mode, makes a backup when changing an existing file, and
refuses conflicting or ambiguous direct assignments.

Grok deliberately does **not** run `tmux source-file` or change the live tmux
server. Reload with the exact command shown after apply, or detach and reattach,
then run `/doctor` again. Until reload, the live finding is expected to remain.
The conservative config scan checks direct global assignments only; review
sourced files, conditionals, plugins, and generated tmux setup yourself.

---

## Detected Terminals

Agent TUI detects these terminal emulators from environment variables:

- **Apple Terminal** (Terminal.app)
- **Ghostty**
- **iTerm2**
- **Warp**
- **WezTerm**
- **Kitty**
- **Alacritty**
- **Rio**
- **foot** (Wayland-native, Linux)
- **VS Code**, **Cursor**, **Windsurf**, and **Zed** integrated terminals
- **JetBrains** IDE terminals (IntelliJ, PhpStorm, and others)
- **Agent TUI Desktop**
- **VTE**-based terminals (GNOME Terminal, GNOME Console, Tilix)
- **Windows Terminal**

Detection has these limitations:

- Inside tmux, the variables Agent TUI needs to identify the terminal don't reach the pager.
- Over SSH, many terminal variables aren't forwarded.
- tmux's global environment (`tmux -g`) reflects the first client that attached to the server, not your current session.

---

## Common Problems and Fixes

### Problem: Colors look wrong or lack truecolor

**Cause**: `COLORTERM` not set or tmux not configured for 24-bit RGB.

**Fix**: Apply the two settings above, then restart Agent TUI.

**Verify**: Run `/terminal-setup`. Expect `color truecolor` and `themes all`. If `color` is `256` or `basic`, the issues section has the unlock fix.

### Problem: Clipboard problems

Agent TUI writes to the clipboard through up to three routes, which match the **Clipboard routes** section of `/terminal-setup`:

Inside tmux there are two separate questions: what color Grok emits, and what
color survives the multiplexer. The `color` line answers the first. For the
second, when the attached client is not marked `RGB`, tmux rewrites every
24-bit color to the nearest color the outer terminal's terminfo advertises,
which can be as few as eight. Themes then look washed out even though `color`
reads `truecolor`. Doctor reports this as `terminal.tmux-truecolor`. Reload
your tmux config and then detach and reattach: the server reads the new option
only on reload, and a client fixes its color depth only at attach, so neither
step alone changes anything.

### Clipboard problems

**Linux Wayland**: on compositors that support the data-control protocol (GNOME 48+, KDE, Sway, Hyprland — the `data-control` line in `/terminal-setup` shows `yes`) copies work even if the terminal loses focus mid-copy. On older compositors (GNOME 46/47), keep the terminal focused until the copy toast confirms, and install the `wl-clipboard` package (provides `wl-copy`) for the most reliable route — Agent TUI shows a startup warning when this applies. If data-control misbehaves on your compositor, set `GROK_CLIPBOARD_NO_DATA_CONTROL=1` to stop Agent TUI from speaking that protocol entirely — copies then go through the CLI tools (`wl-copy`/`xclip`).

**Linux X11 selections**: X11 **PRIMARY** and **CLIPBOARD** are separate. Selecting text usually fills PRIMARY; an explicit Copy action fills CLIPBOARD. In Agent TUI:

- An unmodified middle click reads PRIMARY only when `DISPLAY` is non-empty. Pure X11 can fall back to the native arboard reader. XWayland must have `xclip` or `xsel` on `PATH`; Agent TUI deliberately disables the arboard fallback there so it cannot substitute Wayland PRIMARY.
- `Ctrl+V` reads CLIPBOARD only and never falls back to PRIMARY. To fill CLIPBOARD from a shell, run `printf %s "text" | xclip -selection clipboard`.
- `Shift+Insert` remains the terminal-native selected-text paste. Native Wayland PRIMARY behavior is compositor/terminal-specific and is not inferred from `TERM` or an incoming mouse event.

**SSH and selected text**: a remote Agent TUI process usually cannot read the local terminal's PRIMARY or CLIPBOARD selection. Use terminal-native `Shift+Insert`, or hold `Shift` while middle-clicking when your terminal uses that gesture to bypass mouse reporting. The terminal then sends the local selection through the PTY instead of asking the remote process to access it.

**Known limitation — Apple Terminal + SSH**:
Apple Terminal ignores OSC 52, so copying from a Agent TUI session over SSH can't reach your local clipboard. Use the workaround below.

**Temporary workaround**: Use `agent-tui wrap ssh` instead of plain `ssh` (for example, `agent-tui wrap ssh user@host`). It runs the command in a local PTY that intercepts OSC 52 sequences, including tmux-wrapped ones, and writes their contents to your local clipboard. The same command wraps anything else whose clipboard can't reach you — for example `agent-tui wrap docker exec -it <container> bash` or `agent-tui wrap kubectl exec -it <pod> -- bash`.

> **Warning**: `agent-tui wrap` is **experimental** and may misbehave in some setups.

**iTerm2 setting**:
iTerm2 requires explicit permission for OSC 52:

1. iTerm2 → **Settings** → **General** → **Selection**
2. Enable **"Applications in terminal may access clipboard"**

This setting is off by default for security reasons. Without it, OSC 52 writes from Agent TUI (or any TUI) will be ignored.

**Fix for other cases**:
- `set -g set-clipboard on` in tmux config
- For other terminals over SSH, switch to iTerm2, Ghostty, WezTerm, or Kitty for native OSC 52 support

### Problem: Fullscreen / alternate screen not activating (inline mode)

**Cause**: Zellij, tmux control mode (`tmux -CC`), or config set to `never`.

**Fix**:
- In Zellij or control mode, Agent TUI intentionally runs inline (no alt screen).
- Set `[terminal] alt_screen = "always"` in `~/.agent-tui/pager.toml` to force fullscreen.
- Use the CLI flag `--no-alt-screen` to disable alt-screen mode entirely (useful for debugging or when the alternate screen causes issues in your terminal).

### Problem: Zellij keybindings interfere with Agent TUI (Ctrl+g, Ctrl+o, etc.)

Zellij intercepts many Ctrl/Alt key combinations before they reach full-screen TUIs like Agent TUI.

**Best fix** (Zellij 0.41+): Switch to the **"Unlock-First (non-colliding)"** preset:

1. Press `Ctrl+o` → `c` (open Configuration)
2. Go to **"Change Mode Behavior"**
3. Select **"Unlock-First (non-colliding)"**
4. Press `Enter` (or `Ctrl+a` to save permanently)

After this, Zellij starts **locked**. Most keys pass through to Agent TUI. Press `Ctrl+g` to temporarily unlock Zellij when you need its pane/session management.

Zellij recommends this approach for TUI users.

### Problem: `Ctrl+Enter` doesn't interject in WezTerm

**Cause**: WezTerm ships with the Kitty keyboard protocol disabled. Agent TUI relies on it to tell `Ctrl+Enter` (interject) and `Shift+Enter` (send in multiline mode) apart from plain `Enter`. Most other terminals enable the protocol when Agent TUI requests it.

For the same reason, in Apple Terminal, Agent TUI binds `Ctrl+O` to interject.

**Fix**:

Add this after `config = wezterm.config_builder()` in `~/.config/wezterm/wezterm.lua`:

```lua
config.enable_kitty_keyboard = true
```

Reload (`Cmd+Shift+R` or restart WezTerm) and restart `agent-tui`.

**Verify**: Run `/terminal-setup` inside Agent TUI. While a turn is active, you see the interject hint, and `Ctrl+Enter` interjects.

**Quick workaround** (no global change):

```lua
table.insert(config.keys, {
  key = "Enter",
  mods = "CTRL",
  action = wezterm.action.SendString("\x1b[13;5u"),
})
```

### Problem: `Shift+Enter` doesn't insert a newline in VS Code

**Cause**: VS Code's integrated terminal (and the Cursor / Windsurf / Zed
forks) use xterm.js, which only partially implements the Kitty keyboard
protocol — it mis-encodes shifted printable keys (`!@#$%^&*()` arrive as
plain digits). Agent TUI therefore never negotiates the protocol for these
terminals. Without it, xterm.js sends a bare `CR` for `Shift+Enter`,
byte-for-byte identical to plain `Enter`, so the chord can't be told apart
and the prompt submits.

This also affects VS Code reached **over SSH** (e.g. into a devbox or
container): `TERM_PROGRAM` isn't forwarded, so Agent TUI sees an `Unknown`
terminal and skips the protocol for the same reason.

**Fix**: Use **`Alt+Enter`** to insert a newline. xterm.js delivers it
reliably as `ESC`+`CR` regardless of the keyboard protocol, and Agent TUI's
prompt hint bar advertises `Alt+Enter: newline` whenever it detects this
situation. Run `/terminal-setup` to confirm — the `newline` row shows
`Alt+Enter` when `Shift+Enter` is unavailable.

### Problem: Mouse scrolling stops working (native scrollbar takes over)

If Agent TUI's mouse-driven scrolling stops responding and your terminal falls back to its native scrollbar, mouse reporting is off.

**Apple Terminal**: Go to **View > Allow Mouse Reporting** (keyboard shortcut `Cmd+R`) to re-enable it. A checkmark appears next to the option when active.

**iTerm2**: Open **Settings** (`Cmd+,`) → **Profiles** → **Terminal** → ensure **"Enable mouse reporting"** is checked. Alternatively, restart iTerm2.

### Problem: Byobu + GNU screen

Byobu on screen has best-effort support only. Prefer Byobu on tmux.

---

## Still Stuck?

Run `/feedback` to report it.