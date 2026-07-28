---
name: resolve-agent-tui-config-home
description: Resolve the Agent TUI config home directory the way the product does — $AGENT_TUI_HOME first, legacy $GROK_HOME as an accepted fallback, ~/.agent-tui as the default — before reading or editing runtime.toml or any other config-home file from a script, probe, or doc snippet.
---

# Resolve the Agent TUI config home

Two sites restate this chain independently — the runtime.toml reference ("resolved via `$AGENT_TUI_HOME` with legacy `$GROK_HOME` still accepted as a fallback. Any tooling that reads this file must resolve the home through that chain, never a hardcoded path") and the Grok model-pick drift workflow ("resolving the home via `$AGENT_TUI_HOME`, legacy `$GROK_HOME` fallback"). Per the extraction convention, that is the extraction signal: this skill is the shared procedure, and probes, scripts, and doc snippets should point here rather than restating the chain.

## The chain

1. `$AGENT_TUI_HOME`, if set — the renamed config surface's current name.
2. `$GROK_HOME`, if set — the legacy name, still accepted per the renamed-config-surface convention (a renamed config surface keeps its legacy env var as an accepted fallback).
3. `~/.agent-tui` — the default.

```sh
CONFIG_HOME="${AGENT_TUI_HOME:-${GROK_HOME:-$HOME/.agent-tui}}"
```

## Rules

- **Never hardcode `~/.agent-tui`** in tooling, probes, or copy-pasteable snippets. Per the operative-snippets convention, a hardcoded path in a doc one-liner is a real defect, not a nitpick — someone with `$AGENT_TUI_HOME` set will read or edit the wrong file and every downstream conclusion (drift probes included) is invalidated.
- **Probes must resolve before reading.** The readiness-rung probe and any drift adjudication that checks `runtime.toml` (e.g. "did a `<vendor>_model` key change?") must resolve the home first — a probe reading the default path on a machine using an override reports a false negative.
- **Do not drop the legacy fallback** when writing new tooling; retiring `$GROK_HOME` support is a release-surface change for the fork, not a cleanup any one script may make unilaterally.
