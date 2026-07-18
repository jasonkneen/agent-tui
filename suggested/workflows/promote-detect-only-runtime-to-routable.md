# Workflow: promote a detect-only vendor runtime to fully routable

A vendor may legitimately ship **detect-only**: its `auth::local_cli` detection is wired, it appears in `/runtime` readiness, it can even be selected — but no turns route to it until its real runtime bridge lands (`suggested/agent-updates/detect-only-runtime-state.md`). Claude is in this state today: "`/runtime claude` — Select Claude (detect-only until Agent SDK bridge)" (`docs/LOCAL_CLI_AUTH.md`). This workflow is the sanctioned exit from that state. The forbidden exit — constructing a one-shot HTTP inference path from the credentials the detector found — is a convention violation, not a shortcut (`suggested/agent-updates/credential-detection-is-discovery-only.md`).

## Preconditions

- The vendor is currently labeled detect-only in the operative surfaces (`/runtime` help, `docs/LOCAL_CLI_AUTH.md` usage table, `AGENTS.md` runtime table).
- The intended bridge is the vendor's own harness, per the runtime table in `docs/LOCAL_CLI_AUTH.md` (for Claude: the Claude Agent SDK as a long-lived client/subprocess with a streamed message channel, reusing the Claude Code login — no Agent TUI OAuth).

## Steps

1. **Build the runtime crate as a warm pool.** Model it on `agent-tui-codex-runtime` (`CodexRuntimePool`). It must exhibit all three lifecycle behaviors — idle timeout (~5–15 min), health probe before reuse with respawn-if-dead, optional eager warm on TUI start when the CLI is detected and enabled (`suggested/agent-updates/vendor-runtime-warm-pool-lifecycle.md`). A pool missing any one is incomplete.
2. **Keep detection and inference separate.** The existing `auth::local_cli` helper stays discovery-only: it gates whether the runtime is announced/enabled. All turn traffic goes through the new pool's connection — never through credentials lifted from the detector, never through `SamplerConfig` HTTP.
3. **Wire `/runtime <vendor>` selection to actually route turns.** Selection semantics (persisted in `~/.agent-tui/runtime.toml`) already exist from the detect-only phase; the change is that a selected vendor now serves inference instead of being selectable-but-inert.
4. **Scrub the detect-only annotations in the same change.** The detect-only convention required labeling the state in every surface that mentions the vendor; promotion reverses that sweep symmetrically:
   - `docs/LOCAL_CLI_AUTH.md` TUI usage table — remove "(detect-only until … bridge)" and describe the real routing (mirror the `/runtime codex` row's wording).
   - `AGENTS.md` multi-vendor runtime table — add the new pool alongside `CodexRuntimePool`.
   - Any skills or wiki docs that quote the detect-only annotation (e.g. the vendor-runtime switch skill's command table and its "selecting it does not yet route inference" caveat).
   A promotion that wires routing but leaves stale detect-only labels creates the inverse defect: users told selection is inert when it now sends turns.
5. **Verify.**
   - Run the fork's standard verify pair: `cargo check -p agent-tui-bin` and `cargo test -p agent-tui-update --lib`.
   - Live check: `/runtime <vendor>`, send a turn, confirm it is served by the warm connection (first turn may pay the spawn; subsequent turns must not).
   - Grep the tree for the vendor's detect-only wording — remaining hits must be historical records only, not operative docs or skills.

## What this workflow is not

- Not the initial integration of a brand-new vendor — that is the integrate-new-vendor-runtime workflow (detect helper first, runtime crate second). This workflow covers only the case where detection already shipped and the routing half lands later.
- Not a license to bypass the harness boundary under schedule pressure: if the bridge is not ready, the vendor simply stays detect-only — that is a legitimate shipped state, not a gap to be closed cheaply.
