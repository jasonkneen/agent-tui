---
name: screen-sweep-for-grok-false-defects
description: Before flagging a Grok/xAI file or line as violating a vendor-scoped rule (live-fetched catalog, <vendor>_model key, harness-reused auth, warm-runtime inference, $AGENT_TUI_HOME home), check it against the legacy exception map — Grok predates the vendor-scoped scheme and every one of its documented departures is grandfathered, not a defect. Use during any rule sweep, rename sweep, or code review that touches Grok surfaces.
---

# Screen a vendor-rule sweep or review for Grok false defects

Grok is Agent TUI's built-in default and predates the vendor-scoped external-runtime scheme that Codex and Claude follow. As a result "almost every vendor-scoped rule has a Grok-shaped exception," and "a sweep or review that applies the vendor-scoped rules to Grok files false defects." This skill is the applying-side procedure that keeps those false defects from being filed.

It is the complement of the `grandfathered-exceptions-are-not-templates` convention — not a duplicate. That convention blocks copying Grok's shape *forward* onto a new vendor ("citing Grok's shape as precedent is a defect"). This skill blocks flagging Grok's shape *in place* ("this Grok file violates the live-catalog rule" is a false positive). Both are needed: one guards the future, one guards the sweep in front of you.

## When to run it

- Any sweep that applies a vendor-scoped rule across the fork (rung-label sweeps, live-catalog audits, `runtime.toml` keyset checks, config-home-path audits).
- Any code review or reconciliation whose scope includes Grok / xAI / sampler surfaces.

## Procedure

1. **Before filing a defect on a Grok surface, look it up in the exception map** (`suggested/wiki/grok-legacy-exception-map.md`). For each candidate finding, find the row whose "vendor-scoped rule" it invokes:

   | Vendor-scoped rule you're applying | Grok's grandfathered behavior — NOT a defect |
   |---|---|
   | Catalog fetched live from the vendor's own runtime | Grok rides the **built-in sampler catalog** — no external harness to query |
   | Model pick persists as a `<vendor>_model` key | **No `grok_model` key** — its pick predates the vendor-scoped scheme (and where it *does* persist is a frozen TBD drift) |
   | Inference rides a warm vendor runtime | Grok uses the existing **`agent-tui-sampler`** (HTTP SSE to cli-chat-proxy) |
   | Auth reuses the vendor CLI's login, "no Agent TUI OAuth" | Grok uses the existing **OIDC / API key** flow |
   | Config home is `$AGENT_TUI_HOME` | Legacy **`$GROK_HOME`** is still an accepted fallback in the chain |
   | Vendors occupy a rung of the readiness ladder | Grok holds **all three rungs by construction** — it is the default, not an integrated vendor |

2. **If the finding matches an exception row, drop it** — it is a false defect, not a violation. Do not "fix" Grok to match the vendor-scoped shape.

3. **If it does NOT match a row, it is a real finding** — file it. The exception map covers only Grok's documented pre-scheme departures; a genuine bug on a Grok surface (a wrong sampler default, a broken `$GROK_HOME` fallback) is still a defect.

4. **Guard the frozen fact.** The Grok model-pick persistence is a registered open drift (runtime.toml key vs sampler-owned). While its canonical answer is TBD, do not assert either value — not in a finding, not in a "fix." A TBD row freezes the fact.

5. **If you find a Grok departure the map doesn't list,** the gap is the finding: add the row to the exception map (with its source) in the same change, rather than filing a defect against Grok or silently ignoring it.

## The trap this prevents

A reviewer greps for vendor-scoped compliance and finds Grok's hardcoded catalog, its native auth, and its keyless model persistence *before* finding the convention docs. Each looks like a violation. Filing them buries real findings under noise and pressures someone to "fix" a grandfathered surface — the same forward-copy hazard the exceptions-are-not-templates convention names, reflected backward.
