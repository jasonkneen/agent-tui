# The vendor readiness ladder — detect, catalog, inference ship independently

A vendor runtime integration in Agent TUI is not binary. `docs/LOCAL_CLI_AUTH.md` ("TUI usage (shipped)") shows three separately-wired capabilities, and a vendor can legitimately ship with only a prefix of them:

| Stage | What it means | Wiring |
|---|---|---|
| **1. Detect** | `/runtime` shows the vendor's readiness — "is this CLI logged in?" | `auth::local_cli` credential detection; discovery-only, never an inference path |
| **2. Catalog** | Selecting the vendor loads its live model catalog and `/model` switches within it, persisted as a vendor-scoped `runtime.toml` key (`codex_model` / `claude_model`) | Per-vendor catalog fetch: Codex via `model/list` over the warm app-server; Claude via the Agent SDK harness (`claude -p --output-format json` + sticky `--resume`) |
| **3. Inference** | Turns actually route through the vendor's warm runtime | The full runtime bridge: warm pool / long-lived SDK client with idle timeout, health probe, optional eager warm |

## Where each vendor sits (per the shipped table)

- **Grok / xAI** — all three (the built-in default; existing sampler HTTP SSE).
- **Codex** — all three: detected via `~/.codex/auth.json`, catalog over `model/list`, turns route through the warm `codex app-server`.
- **Claude** — stages 1 and 2, not 3: `/runtime claude` is "detect-only until Agent SDK bridge" for *turn routing*, yet selecting it loads the Claude model catalog through the `claude -p` harness and `/model` persists a `claude_model` choice.

The Claude row is the load-bearing example: **catalog can ship before inference**. A vendor whose models are browsable but whose turns don't route is a sanctioned intermediate state, not a half-broken integration.

## Rules the ladder implies

- Stages complete **in order** — there is no supported state with inference but no detection, and a catalog is always served by the vendor's own local harness (live-fetched, never hardcoded).
- Reaching stage 2 never licenses a shortcut to stage 3: inference goes through the warm runtime connection, never a one-shot HTTP call built from detected credentials. The catalog harness existing does not change this.
- Every surface that names a partially-wired vendor labels its stage (as the shipped table does for Claude), so users read "selectable but not routing" as intended.
- Promoting a vendor up the ladder follows the promote-detect-only workflow; the final rung is the full warm-pool lifecycle (idle timeout, health probe, eager warm).

## How to use this doc

When triaging "vendor X is selectable but doesn't answer," first place X on the ladder before filing a defect — for Claude today, that is the documented state. When integrating a new vendor, plan the landing order as detect → catalog → inference, and update the `/runtime` help, `docs/LOCAL_CLI_AUTH.md` table, and `AGENTS.md` runtime table at each rung.