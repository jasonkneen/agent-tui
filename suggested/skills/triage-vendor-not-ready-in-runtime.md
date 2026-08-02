---
name: triage-vendor-not-ready-in-runtime
description: Diagnose a vendor runtime that shows missing or not-ready in /runtime — order the checks so a stale binary or a logged-out vendor CLI isn't misdiagnosed as broken detection wiring, since a not-logged-in CLI makes every rung read as failed.
---

# Triage a vendor that reads not-ready in /runtime

The readiness-rung probe stops at rung 1 with: "Missing/not-ready with a logged-in CLI → detection itself is broken; stop and triage." This skill is that triage. The ordering matters because the two cheapest causes — a stale binary and a logged-out CLI — each make every downstream check read as failed, so they must be excluded before touching detection wiring.

## 1. Rule out a stale binary

Launch a **current build by path** from the build output — never Spotlight, Dock, or bundle id. Bundle shadowing hands you an old binary whose `/runtime` predates the vendor's detection wiring entirely; re-run `/runtime` in the fresh build before concluding anything.

## 2. Rule out a logged-out vendor CLI

There is **no Agent TUI-side login** — detection reuses the vendor CLI's own auth (Claude: the Claude Code login, keychain / `~/.claude`; Codex: `~/.codex/auth.json`). Verify the vendor's CLI works standalone in the same shell environment: run the vendor CLI directly and confirm it is authenticated. A not-logged-in CLI makes every rung read as failed — log in via the vendor's own product and re-probe before going further.

## 3. Now suspect the detection wiring

With a current build and a confirmed-logged-in CLI, a still-missing vendor means `auth::local_cli` detection itself is failing. Check in order:

- **Is the CLI discoverable from the TUI's environment?** The detector answers "is this CLI logged in?" — a CLI installed under a PATH or home the TUI process doesn't see reads as absent. Compare the shell that works with the environment the TUI launches under.
- **Is the credential where the detector looks?** Detection reads the vendor's own credential surface (keychain / `~/.claude` for Claude, `~/.codex/auth.json` for Codex). A nonstandard home or relocated auth file breaks discovery even though the CLI works interactively.
- **Is the vendor wired at all in this build?** A vendor absent from `/runtime` entirely (rather than shown not-ready) may simply not have its detect rung in this version — check the release notes before debugging.

## 4. Record what you found

State the diagnosis as which layer failed: stale binary, CLI auth, environment/discovery, or detection code. If the outcome contradicts a doc's rung label for the vendor, that is a drift — register it rather than silently correcting either surface, and remember detection is discovery-only: whatever the fix, it must not route inference through credentials the detector found.