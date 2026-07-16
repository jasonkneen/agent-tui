---
name: triage-agent-tui-defect
description: Classify an incoming Agent TUI bug or improvement as fork-surface vs inherited-upstream behavior, route it to the correct tracker and fix location, and avoid drafting patches for a tree that cannot accept them.
---

# Triage an Agent TUI defect (fork surface vs inherited upstream)

Every defect lands in one of two buckets, and the bucket decides where the report and the fix go. Upstream Grok Build "does **not** accept external pull requests or unsolicited patches" — it is published for source transparency under Apache-2.0 (`CONTRIBUTING.md`). All contribution flow targets the fork (`jasonkneen/agent-tui`).

## 1. Classify the surface

**Fork-surface** — anything the fork introduced or renamed (see the FORK.md table):
- Product identity: the `agent-tui` binary name, `agent-tui-*` crate names, `~/.agent-tui` / `$AGENT_TUI_HOME` config home
- Distribution: `install.sh` / `install.ps1`, the in-app updater (`agent-tui-update`), release workflow, asset names, npm package `@agent-tui/agent-tui`
- Fork docs: FORK.md, README.md, RELEASING.md, AGENTS.md, the user guide

**Inherited-upstream** — behavior that predates the fork and lives in shared code:
- Provider wire contracts (auth method IDs like `xai.api_key`, `grok.com`; `XAI_API_KEY`; model IDs; `*.grok.com` endpoints; the `X-XAI-Token-Auth: xai-grok-cli` header)
- Core TUI/agent behavior unchanged from the `dev-reviewed` @ `eb2eca1` baseline

When unsure, check whether the code path involves anything in FORK.md's "What we deliberately did **not** change" list — if yes, lean inherited-upstream.

## 2. Route it

| Bucket | Report to | Fix lands in |
|--------|-----------|--------------|
| Fork-surface | Fork issue tracker (`jasonkneen/agent-tui`) | The fork |
| Inherited-upstream | Fork issue tracker, labeled as inherited | Still the fork — optionally noted for upstream, with **no expectation upstream will take it** |
| Upstream security issue | Upstream's `SECURITY.md` channel | Upstream's process |

## 3. Hard rules

- Never open an upstream PR or draft a patch "to send upstream" — it cannot land, and it delays shipping the fix through the channel users actually install from (the fork's GitHub Releases).
- Never file a fork-introduced bug (renamed crates, installer, updater, release identity) against upstream — it is out of scope there by definition.
- A fix in shared code is still just a fork fix: implement, verify with the standard pair (`cargo check -p agent-tui-bin`, `cargo test -p agent-tui-update --lib`), and release through the fork's channels.
