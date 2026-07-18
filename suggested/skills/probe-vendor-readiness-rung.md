---
name: probe-vendor-readiness-rung
description: Empirically determine which rung of the vendor readiness ladder (detect, catalog, inference) a vendor runtime actually occupies in a running Agent TUI build, so docs and drift adjudications rest on behavior rather than labels.
---

# Probe a vendor's actual readiness rung in a live Agent TUI

Doc labels for a vendor's readiness rung can lag shipped behavior — the Claude "detect-only" annotation coexisting with a working catalog harness is the registered example. For live services, on-wire behavior is canonical over any doc, so rung questions are settled by this probe, not by reading tables. Both readiness reconciliation workflows embed this same procedure; this skill is the shared extraction.

## Setup

- Launch a **current build by path** from the build output (never Spotlight/Dock/bundle id — bundle shadowing gives you a stale binary).
- Ensure the vendor's own CLI is logged in (Claude Code login for Claude, `~/.codex/auth.json` for Codex). There is no Agent TUI-side login; a not-logged-in CLI makes every rung read as failed.

## The probe, rung by rung

**Rung 1 — detect.** Run `/runtime`. Does the vendor appear with a ready status? Ready → rung 1 confirmed. Missing/not-ready with a logged-in CLI → detection itself is broken; stop and triage.

**Rung 2 — catalog.** Run `/runtime <vendor>`, then `/model`. Record exactly what the picker lists: the vendor's live catalog, the Grok catalog, or both. If the vendor catalog loads, pick a model and confirm the write: a `<vendor>_model` key appears in `~/.agent-tui/runtime.toml` and applies on the next thread (not mid-thread).

**Rung 3 — inference.** With the vendor selected, send a turn. Does the response come from the vendor's runtime (subprocess/socket activity for that harness), or does it fall through to Grok / fail inert? Route to vendor → rung 3. Also check the warm behavior: a second turn should be faster than the first (warm connection reuse), and turns must not spawn a fresh harness per request.

## Recording the result

- State the rung as the highest **consecutive** rung passed — rungs complete in order, and an out-of-order result (e.g. inference works but detection doesn't) indicates a broken build, not a ladder state.
- Record the probe outcome with build version, date, and your name — an unattributed value cannot serve as evidence later.
- If the observed rung contradicts an operative label or convention doc, do not edit the label ad hoc: register (or update) the drift-ledger row and run its owning reconciliation workflow.

## Traps

- A loaded catalog does **not** imply routable inference — catalog-shipped/inference-pending is a sanctioned state; test rung 3 separately.
- A `ready` status is detection only ("is this CLI logged in?") — never report it as evidence for rungs 2 or 3.