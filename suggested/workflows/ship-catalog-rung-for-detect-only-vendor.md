# Workflow: ship the catalog rung for a detect-only vendor

The vendor readiness ladder has three rungs that ship independently and in order: detect, catalog, inference. The promote-to-routable workflow covers rung 3; this workflow covers rung 2 — making a detected vendor's models browsable and selectable while turns still do not route to it. Claude is the shipped exemplar: "detect-only until Agent SDK bridge" for turn routing, yet "After `/runtime claude`, Agent TUI loads that vendor's model catalog and `/model` switches it" (`docs/LOCAL_CLI_AUTH.md`, "TUI usage (shipped)").

## Preconditions

- The vendor is at rung 1: `auth::local_cli` detection wired, visible in `/runtime` readiness, selectable but not routed.
- The vendor's own local harness can answer a catalog query — rungs never skip, and a catalog is always served by the vendor's runtime, never hardcoded.

## Steps

1. **Pick the catalog transport from the vendor's runtime shape.** Socket-shaped vendors query a listing call over the warm connection (Codex: `model/list` over `codex app-server`). Subprocess-shaped vendors query through the CLI harness (Claude: `claude -p --output-format json` with sticky `--resume`). Do not invent a third shape — reuse whatever connection the eventual inference bridge will use.
2. **Wire `/model` to the vendor's live catalog on selection.** After `/runtime <vendor>`, the catalog loads from the harness and `/model` switches within it. No model IDs land in fork source — the catalog is live-fetched (a literal non-Grok model ID in the diff is a defect).
3. **Persist the pick as a vendor-scoped `runtime.toml` key.** Follow the shipped naming: `<vendor>_model` in `~/.agent-tui/runtime.toml`, applied on the next thread, never mid-thread.
4. **Do NOT route turns.** The detection/inference split stays intact: no one-shot HTTP path built from detected credentials, no `SamplerConfig` reuse. Selecting the vendor remains selectable-but-inert for inference.
5. **Update every surface that labels the vendor's rung.** The `/runtime` help, the `docs/LOCAL_CLI_AUTH.md` usage table, and the `AGENTS.md` runtime table must now say catalog-shipped / inference-pending rather than plain "detect-only" — an unqualified detect-only label alongside a working catalog is exactly the readiness-rung drift already registered for Claude. If a convention doc grounds on the old label, handle it per the supersession rules.
6. **Verify.** Run the fork's standard verify pair, then on-wire: `/runtime <vendor>` → `/model` lists the live catalog → pick persists as `<vendor>_model` → a turn still routes to the previously active runtime (proving rung 3 was not accidentally wired).

## Exit

The vendor sits at rung 2, a sanctioned intermediate state — "a vendor whose models are browsable but whose turns don't route is a sanctioned intermediate state, not a half-broken integration." Rung 3 later goes through the promote-detect-only-runtime-to-routable workflow.