---
name: prepare-wire-probe-environment
description: Stand up a trustworthy environment before any on-wire Agent TUI adjudication — current build launched by path, logged-in vendor CLI, config home resolved via the $AGENT_TUI_HOME → $GROK_HOME chain — so probe results reflect shipped behavior rather than a stale binary or a logged-out CLI.
---

# Prepare a trustworthy wire-probe environment for an Agent TUI adjudication

On-wire behavior is canonical over any doc for live services — which makes the probe environment itself load-bearing: a probe run against a stale binary or a logged-out CLI manufactures false on-wire evidence with canonical weight. Four artifacts independently restate the same setup (the readiness-rung probe's Setup section, step 2 of the /model-scope reconciliation, step 2 of the Grok model-pick reconciliation, and the shipped-marker audit); per the extraction convention, this skill is the shared procedure and those sites should point here.

## 1. Launch a current build by path

Launch the build **by path from the build output** — never via Spotlight, Dock, or bundle id. Bundle shadowing hands you a stale binary, and every subsequent observation describes the wrong artifact.

## 2. Confirm the vendor CLI is logged in

Ensure the vendor's own CLI is authenticated: Claude Code login (keychain / `~/.claude`) for Claude, `~/.codex/auth.json` for Codex. There is no Agent TUI-side login — a not-logged-in CLI makes **every** rung read as failed, so a logged-out probe proves nothing about shipped behavior.

## 3. Resolve the config home through the chain

When the probe reads or verifies `~/.agent-tui/runtime.toml`, resolve the home via `$AGENT_TUI_HOME` with legacy `$GROK_HOME` still accepted as a fallback — never a hardcoded path. Watching the wrong file reports a real persistence write as missing.

## 4. Then hand off to the probe

With the environment sound, run whatever needed the wire: the readiness-rung probe, a drift reconciliation's ground-truth step, or the shipped-marker audit. Record the environment facts — build path, CLI login state, resolved config home — alongside the probe result, so an adjudication citing the probe can show its evidence was gathered soundly.

## Failure reading

If the wire appears to contradict shipped docs, re-check these preconditions **before** registering a drift: a stale binary or logged-out CLI produces exactly the "label says X, wire says not-X" signature that would otherwise register as a false drift-ledger row.