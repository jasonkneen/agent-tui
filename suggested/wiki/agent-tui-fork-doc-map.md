# Agent TUI fork documentation map — which doc owns which question

The Agent TUI fork governs itself through a small set of root documents, each owning a distinct question. `CONTRIBUTING.md` publishes the core of this table ("What changed vs upstream → FORK.md", "How to cut a release → RELEASING.md", …) and `AGENTS.md` cross-links the rest. This map is the derived index — content lands in the owning docs first.

| Question | Owning doc | What it holds |
|---|---|---|
| What changed vs upstream? What is frozen? | `FORK.md` | Rename table (binary, crates, config home), fork baseline commit, the deliberately-unchanged list (auth method IDs, model IDs, endpoints, `X-XAI-Token-Auth` header), dual-install with official `grok` |
| How do I build or install it? | `README.md` (+ user guide `01-getting-started.md`) | Source-build requirements (pinned Rust toolchain, dotslash protoc), install one-liners, supported hosts |
| How do I cut a release? | `RELEASING.md` | Channel table (stable vs alpha tag shapes), asset-name contract, `version.rs` constants, what ships where (GitHub Releases / npm / never x.ai CDN) |
| What must automation never break? | `AGENTS.md` | Do-not-break list, multi-vendor runtime table with hard boundaries, release-surface pointer table |
| Where does a fix or patch go? | `CONTRIBUTING.md` | Upstream is source-transparency only — all contribution flow targets the fork; the verify command pair; the doc table this map extends |
| What is the architecture? | `docs/CORE_AND_ADDONS.md` | ONE CORE + ADDONS; zero-dup symlink product skins; product profiles |
| How do external vendor runtimes work? | `docs/LOCAL_CLI_AUTH.md` | Addon harness detail: Grok · Codex · Claude · Lazar; gaps vs Go `lazartui` |
| Where is the Go Lazar TUI? | `docs/LOCAL_CLI_AUTH.md` (Lazar section) + `~/lazar/workspace/tui/` | **Not in this repo** — Go source + `lazartui` binary live under lazar home |
| Upstream security issues | upstream `SECURITY.md` | Explicitly not the fork's channel (`CONTRIBUTING.md`) |

## How to use it

- **Before answering a fork question**, route to the owning doc rather than reconstructing from memory — the frozen-contract details (header values, model IDs, asset names) are exact strings where paraphrase introduces drift.
- **Before adding documentation**, check the table: if a question already has an owner, extend that doc. A second doc answering an owned question is a future drift-ledger row.
- **When a new governing doc lands** (e.g. a future `SECURITY.md` for the fork itself), add its row here in the same change, per the same-change coupling law.

## Non-doc sources of truth

Two facts live in code, not docs, and outrank any doc row above when they disagree: release-identity constants in `crates/codegen/agent-tui-update/src/version.rs`, and the asset names produced by `.github/workflows/release.yml` (which `RELEASING.md` mirrors but does not generate).