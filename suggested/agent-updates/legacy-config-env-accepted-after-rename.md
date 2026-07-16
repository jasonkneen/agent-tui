# Convention: a renamed config surface keeps its legacy env var as an accepted fallback

**Rule.** When the fork renames a user-facing configuration surface, the new name becomes primary but the upstream/legacy environment variable remains accepted. Concretely: the config home is **`~/.agent-tui`** driven by **`$AGENT_TUI_HOME`**, and the legacy **`$GROK_HOME`** is still honored. Code that resolves the config home goes through this fallback chain — it never hardcodes only the new name, and it never silently removes the legacy acceptance.

**Grounding.**
- `FORK.md`, fork comparison table: "Config home … **`~/.agent-tui`** / `$AGENT_TUI_HOME` (legacy `$GROK_HOME` still accepted)".
- `AGENTS.md`: "Product binary is **`agent-tui`**, config home is **`~/.agent-tui`**" — the rename is real and product-level, yet FORK.md deliberately preserves the legacy variable alongside it.
- The dual-install guide (`FORK.md`, "Dual install with official `grok`") relies on the two products using *different* config homes (`~/.grok` vs `~/.agent-tui`) — which is why the legacy variable is a fallback, not a replacement for the fork's own variable.

**Why:** the rename boundary splits identity (fully forked) from compatibility (fully preserved). Users migrating from upstream, and scripts written against upstream, have `$GROK_HOME` baked in; dropping it would break them with no compile-time or CI signal — exactly the class of invisible breakage the fork's release-surface conventions exist to prevent. This is also the fork-side instance of the workspace-wide persistence law: when a path or name moves, the old one is demoted to an explicit fallback — never deleted.

**How to apply:** any new code path that derives a location from the config home resolves it through the existing chain (`$AGENT_TUI_HOME`, then legacy `$GROK_HOME`, then the default `~/.agent-tui`) rather than reading one variable directly. When reviewing a change that touches config-home resolution, treat removal of the legacy acceptance as a convention violation unless it ships with an explicit, documented deprecation decision recorded in `FORK.md`. When documenting configuration, list the fork name first and note the legacy variable, as `FORK.md` does — never document the legacy name as the primary interface. If a future rename adds a third name, the chain grows; it does not rotate.
