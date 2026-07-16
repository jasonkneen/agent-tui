---
name: stand-up-mock-inference-server
description: Set up agent-tui-test-support's MockInferenceServer for an integration test — pick the right constructor, build MockModelEntry models with the chainable capability flags, choose among the three response modes (respecting the scripted > required-auth > mode precedence), and gate settings/user state — as consumed by agent-tui-shell integration tests, the pager PTY harness, and sampler tests.
---

# Stand up a MockInferenceServer for an integration test

`agent-tui-test-support` is the shared test infrastructure for the grok-build crates (`crates/codegen/agent-tui-test-support/README.md`). Its `mock_server` module serves `/v1/chat/completions`, `/v1/responses`, `/v1/messages`, `/v1/models`, `/v1/settings`, and `/v1/user` on `127.0.0.1:0`.

## Step 1: Pick the constructor

All constructors return `anyhow::Result`:
- `start()` — default server.
- `start_with_models(...)` — seed `/v1/models` with your `MockModelEntry` list.
- `start_with_required_auth(...)` — enable the required-auth response mode.

## Step 2: Build the models

`/v1/models` entries are `MockModelEntry` (re-exported as `MockModel` for PTY tests). Start from `new(id)` or `with_agent_type(id, ty)`, then chain capability flags — each maps to a top-level field exactly as `parse_remote_model_value` reads it:
- `with_api_backend(...)`
- `with_supports_backend_search(bool)` → `supportsBackendSearch`
- `with_supports_reasoning_effort(bool)` → `supportsReasoningEffort`
- `with_reasoning_effort(&str)` → `reasoningEffort`
- `with_reasoning_efforts(Vec<Value>)` → `reasoningEfforts` (raw option tables or bare strings)

## Step 3: Choose the response mode — know the precedence

The inference endpoints have three modes with precedence **scripted > required-auth > mode**:
1. **echo** (default) — streams `Echo: <last user message>`, whitespace-collapsing. Good for smoke tests where content doesn't matter.
2. **fixed** — `set_response(text)`; byte-exact delta reconstruction, newlines preserved, so fenced code blocks survive. Use when asserting on rendered output.
3. **scripted** — `enqueue_response(...)`; one-shot responses consumed in order. Use for multi-turn scripts. Scripted always wins over the other modes.

## Step 4: Gate settings and user state if the test needs them

- `/v1/settings` is **404-until-set**: call `set_settings(impl Serialize)`, or `preset_allow_access()` for the common `{"allow_access": true}` gate. Scripted `/v1/settings` one-shots (via `enqueue_response`) take precedence over the steady-state value — that is how stale-snapshot tests are written.
- `/v1/user` serves a minimal `UserInfo`; control `subscriptionTier` with `set_user_subscription_tier(Option<&str>)` (`None` = absent).

## Step 5: Keep the README honest

This crate's README is its API reference under the same-PR freshness rule — if your test needed a new mock capability and you added it to `src/`, the README row lands in the same PR.

## Where to look for full examples

Consumers: `agent-tui-shell` integration tests, `agent-tui-pager-pty-harness` (`ContentController`), and `agent-tui-sampler` tests. How-to-test discovery lives with the pager PTY harness crate.