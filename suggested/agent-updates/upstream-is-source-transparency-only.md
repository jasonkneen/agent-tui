# Convention: upstream Grok Build is source-transparency only — never send patches upstream

**Rule.** The upstream Grok Build tree is published for source transparency under Apache-2.0, not for collaboration: it does **not** accept external pull requests or unsolicited patches. All contribution flow — fixes, features, release work, automation — targets the fork (`jasonkneen/agent-tui`) and its own surfaces (FORK.md, README.md, RELEASING.md, AGENTS.md). Upstream security issues go through upstream's `SECURITY.md`, not the fork's channels.

**Grounding.**
- `CONTRIBUTING.md`, "Upstream (SpaceXAI / xAI)": "The original Grok Build tree does **not** accept external pull requests or unsolicited patches. Upstream is published for source transparency under the Apache License 2.0."
- Same doc: "Security for **upstream** Grok Build: see `SECURITY.md`", while everything under "This fork (Agent TUI)" routes to the fork's own doc table and CI/release workflows.

**Why:** a fix drafted as an upstream PR is wasted work — it cannot land — and worse, it delays shipping the fix in the channel users actually install from (the fork's GitHub Releases). The convention also prevents a subtler error: filing a fork-introduced bug (renamed crates, installer, updater) against upstream, where it is out of scope by definition.

**How to apply:** when a defect or improvement is found in shared code, fix it in the fork and record it there; never open an upstream PR or draft a patch "to send upstream." When triaging a bug, first classify it as fork-surface (renamed crates, install/update/release identity → fork issue tracker) vs inherited-upstream behavior (still fixed in the fork; optionally noted, but with no expectation upstream will take it). When writing contributor-facing docs, keep the upstream/fork split explicit as `CONTRIBUTING.md` does, so new contributors don't route work to a tree that cannot accept it.