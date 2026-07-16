# Vendored Mermaid rendering stack — audit surface and re-vendoring rules

`third_party/` holds upstream Rust crates vendored into the repository — **upstream source, not first-party application code**. This doc collects why the stack is vendored, its exact shape, and the rules for touching it.

## Why these crates are vendored (the security rationale)

The stack sits on the path that renders **untrusted model output**: diagram source → SVG. Vendoring buys three things (`third_party/README.md`):

1. **A full audit surface** — every line that processes untrusted input is in-tree and reviewable.
2. **Pinned exact source** — no silent upstream drift on a security-sensitive path.
3. **Immunity to crates.io yanks.**

## The dependency shape

```text
agent-tui-mermaid
  └── mermaid-to-svg          (MIT, warpdotdev/mermaid-to-svg)
        ├── dagre_rust        (Apache-2.0, 0.0.5, r3alst / Warp re-vendor)
        │     ├── graphlib_rust     (Apache-2.0, 0.0.2)
        │     └── ordered_hashmap   (Apache-2.0, 0.0.3)
        └── graphlib_rust
              └── ordered_hashmap
```

Each crate directory carries its own full license text (`LICENSE` / `LICENCE`).

## Where the maintenance truth lives

**Local patches and upgrade checklists live in each crate's `Cargo.toml` header comments — treat those as the source of truth when re-vendoring** (`third_party/README.md`). Before re-vendoring or upgrading any crate in the stack, read its `Cargo.toml` header first: it records what was changed locally and what the upgrade must re-verify. Re-vendoring from upstream without replaying those patches silently reverts them.

## The notice surfaces

- `third_party/NOTICE` — one-page index: crate names, licenses, upstream links, paths to full license text. Prefer it for an overview.
- `third_party/mermaid-to-svg/THIRD_PARTY_NOTICES` — deeper ancestry for the SVG engine (mermaid.js, dagre.js MIT notices), since the Rust crate is itself derived from JS originals.

## Vendored vs ported — the neighboring pattern

The repo also contains *ported* third-party code, handled differently: `crates/codegen/agent-tui-tools/THIRD_PARTY_NOTICES.md` covers tool implementations translated from openai/codex and sst/opencode into this crate's `Tool` trait, where the notices file itself "constitutes the prominent notice of those changes required by Apache License 2.0 §4(b)". Vendored code keeps upstream source intact with patches ledgered in `Cargo.toml` headers; ported code is rewritten and discharges its modification-notice obligation through the notices file. Know which pattern you are in before editing.

## Rules when touching the stack

- Never edit a vendored crate casually — it is upstream source; a local change is a patch that must be recorded in that crate's `Cargo.toml` header comments.
- Re-vendoring = fetch upstream at the pinned target, replay the header-documented patches, run the header's upgrade checklist.
- License files travel with their crates; the `NOTICE` index and the per-crate license texts stay in sync with the table in `third_party/README.md`.