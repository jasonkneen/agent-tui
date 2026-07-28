# Grok, the pre-scheme built-in vendor — the legacy exception map

Grok / xAI is Agent TUI's built-in default, and it predates the vendor-scoped external-runtime scheme that Codex and Claude follow. The result: almost every vendor-scoped rule has a Grok-shaped exception, and those exceptions are scattered across the runtime docs. A sweep or review that applies the vendor-scoped rules to Grok files false defects; this page collects the exceptions in one place.

## The exception map

| Vendor-scoped rule | What Grok does instead | Source |
|---|---|---|
| Model catalogs are fetched live from the vendor's own runtime | Grok rides the **built-in sampler catalog** — there is no external harness to query | `suggested/wiki/runtime-toml-reference.md`; readiness-ladder doc: "the built-in default; existing sampler HTTP SSE" |
| Model picks persist as a `<vendor>_model` key in `~/.agent-tui/runtime.toml` | **No `grok_model` key exists** — "Grok's model pick predates this file's vendor-scoped scheme and rides the built-in sampler catalog." Where the pick actually persists is a **registered open drift** (runtime.toml key vs sampler-owned) — per the frozen-fact rule, assert neither value until that row closes | `suggested/wiki/runtime-toml-reference.md`; `suggested/workflows/reconcile-grok-model-pick-persistence-drift.md` |
| Inference rides a warm vendor runtime (held socket or sticky subprocess session) | Grok uses the **existing `agent-tui-sampler`** — HTTP SSE to cli-chat-proxy | `docs/LOCAL_CLI_AUTH.md`, runtime table |
| Auth reuses the vendor CLI's own login — "no Agent TUI OAuth" | Grok uses the **existing OIDC / API key** flow — it is the one vendor with Agent TUI-native auth | `docs/LOCAL_CLI_AUTH.md`, runtime table |
| Config home is `$AGENT_TUI_HOME` | The **legacy `$GROK_HOME`** is still accepted as a fallback in the resolution chain — a Grok-era name surviving by the renamed-config-surface convention | `suggested/wiki/runtime-toml-reference.md` |
| Vendors occupy a rung of the readiness ladder | Grok holds **all three rungs by construction** — it is the default runtime, not an integrated vendor | `suggested/wiki/vendor-runtime-readiness-ladder.md` |

## How to use this map

- **When sweeping or reviewing:** before flagging a Grok surface for violating a vendor-scoped rule (hardcoded catalog, missing `runtime.toml` key, non-harness auth), check this map — the exception is likely sanctioned legacy, not a defect.
- **When adjudicating drifts:** Grok's pre-scheme status is itself evidence — a doc sentence written under the old scheme (e.g. the superseded "/model is Grok-only" reading) can be a fossil of this history rather than a claim about current behavior.
- **When integrating a new vendor:** none of these exceptions transfer. The vendor-scoped scheme is the template; Grok is grandfathered (see the companion convention).
