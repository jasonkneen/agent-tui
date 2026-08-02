# Vendor integration pre-flight checklist — per-rung validation requirements

The vendor readiness ladder has three rungs that ship independently (detect, catalog, inference), and each rung has specific validation requirements before it is shipped. This checklist provides a single reference for what must pass empirically before a vendor advances.

## Rung 1: Detect

**Goal:** `/runtime` shows the vendor's readiness and detects whether the user's CLI is logged in.

- [ ] **Detection implemented** — `auth::local_cli` helper wired for the vendor's credential store (e.g., `~/.codex/auth.json` for Codex, keychain for Claude).
- [ ] **Readiness shown in `/runtime`** — Run `/runtime` with vendor CLI logged in; vendor appears with ready/detected status.
- [ ] **Readiness hidden when not logged in** — Uninstall or logout the vendor's CLI and confirm `/runtime` no longer shows it as ready.
- [ ] **No inference attempted** — Selecting the vendor with `/runtime <vendor>` does not route turns; turns still go to Grok or the prior runtime (selectable-but-inert).
- [ ] **Three surfaces updated** — `/runtime` help text, `docs/LOCAL_CLI_AUTH.md` usage table, and `AGENTS.md` runtime table all state the vendor as "detected" or "detect-only".

## Rung 2: Catalog

**Goal:** Selecting the vendor loads its live model catalog and `/model` switches within it; the pick persists as a vendor-scoped key in `~/.agent-tui/runtime.toml`.

**Prerequisites:** Rung 1 all pass.

- [ ] **Catalog transport wired** — The vendor's own runtime (socket, subprocess harness, or API) can answer a "list models" query. For Codex: `model/list` over the warm app-server. For Claude: `claude -p --output-format json` via the Agent SDK harness.
- [ ] **Live catalog loads** — `/runtime <vendor>`, then `/model` shows the vendor's actual live catalog (from the harness, not hardcoded).
- [ ] **Model selection persists** — Pick a model via `/model`; verify a `<vendor>_model` key appears in `~/.agent-tui/runtime.toml` with the correct model ID.
- [ ] **Persistence survives restart** — Edit the runtime.toml key to a different model, restart the TUI, and confirm `/model` highlights the new pick on the next thread.
- [ ] **Vendor-scoped independence** — With two catalog-capable vendors: pick a model in vendor A, switch to vendor B, pick a different model, switch back to A, and confirm A's original pick is restored (verify via runtime.toml: both `<vendor_a>_model` and `<vendor_b>_model` keys coexist).
- [ ] **No inference shortcut** — Turns still do not route through the vendor; only turn-serving runtime remains the prior selection or Grok.
- [ ] **Three surfaces updated** — `/runtime` help, `docs/LOCAL_CLI_AUTH.md` table, `AGENTS.md` table now state "catalog-shipped" or similar; no longer plain "detect-only".

## Rung 3: Inference

**Goal:** Turns actually route through the vendor's warm runtime connection; the integration is complete and fully routable.

**Prerequisites:** Rung 2 all pass.

- [ ] **Turns route to vendor** — `/runtime <vendor>`, send a turn, observe it completes through the vendor's runtime (subprocess, socket activity, or logged inference).
- [ ] **Warm pool initialized** — Per the validate-vendor-warmpool-lifecycle workflow: idle timeout works, health probe detects and respawns dead connections, eager warm is implemented or documented as disabled.
- [ ] **Model persistence applies** — Send a turn with model A, edit `<vendor>_model` in runtime.toml to model B, restart, send another turn, and confirm it uses model B (thread-boundary application enforced).
- [ ] **No mid-thread model swap** — Within a thread, editing runtime.toml does not change the in-flight model; changes apply only on the next thread (verify by sending a turn, editing the key mid-turn, and confirming the turn completes with the original model).
- [ ] **Inference latency and concurrency** — Successive turns are faster than the first (warm connection reused); no fresh harness spawned per-request.
- [ ] **Three surfaces updated** — `/runtime` help, `docs/LOCAL_CLI_AUTH.md` table, `AGENTS.md` table now state "fully routable" or similar, with no caveats (no "until bridge lands").
