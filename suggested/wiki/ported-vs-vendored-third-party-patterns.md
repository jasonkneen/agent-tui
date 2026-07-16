# Ported vs vendored third-party code — the two in-repo compliance patterns

The repository carries third-party code in two deliberately different shapes. Choosing the wrong pattern (or applying one pattern's rules to the other) either loses the security audit surface or drops a license-required notice.

## Pattern 1: Vendored — upstream source copied verbatim into `third_party/`

**What it is.** Whole upstream crates copied into the repo as-is: `mermaid-to-svg`, `dagre_rust`, `graphlib_rust`, `ordered_hashmap`. `third_party/README.md` is explicit: this is "**upstream source** vendored into the repository. It is **not** first-party application code."

**When it's used.** When the code sits on a security-sensitive path (here: rendering untrusted model output, diagram source → SVG) and the team wants a full in-tree audit surface, pinned exact source, and immunity to crates.io yanks.

**Notice surfaces.**
- Each crate directory keeps its own full license text (`LICENSE` / `LICENCE`).
- `third_party/NOTICE` — one-page index (names, licenses, upstream links, paths to full text).
- Deeper ancestry files where the crate is itself derived (e.g. `mermaid-to-svg/THIRD_PARTY_NOTICES` for the mermaid.js / dagre.js MIT lineage).

**Maintenance rule.** Local patches and upgrade checklists live in each crate's `Cargo.toml` header comments — "treat those as the source of truth when re-vendoring." Re-vendoring from upstream without replaying those patches silently reverts them.

## Pattern 2: Ported — translated/derived code living inside a first-party crate

**What it is.** Tool implementations translated between languages and adapted to this codebase's traits and runtime: `agent-tui-tools/src/implementations/codex/` (from openai/codex: `apply_patch`, `grep_files`, `list_dir`, `read_file`) and `src/implementations/opencode/` (from sst/opencode: `bash`, `edit`, `glob`, `grep`, `read`, `skill`, `todowrite`, `write`).

**When it's used.** When the upstream code is a starting point, not a dependency — it is rewritten into the crate's `Tool` trait and runtime and then evolves as first-party code.

**Notice surface.** A crate-level `THIRD_PARTY_NOTICES.md` that (a) names each ported file set and its upstream project, (b) reproduces the original license terms, and (c) **constitutes the prominent notice of changes required by Apache License 2.0 §4(b)**: "Ported files have been modified from their originals (translated between languages, adapted to this crate's `Tool` trait and runtime, and extended); this file constitutes the prominent notice of those changes." The same file also covers prebuilt third-party tool binaries embedded in release builds ("Bundled tool binaries").

**Maintenance rule.** Ported code is edited freely like any first-party code — there is no re-vendoring step — but new ports (or newly bundled binaries) must add their section and license reproduction to the crate's `THIRD_PARTY_NOTICES.md`.

## Choosing a pattern for new third-party code

| Question | Vendor into `third_party/` | Port into a first-party crate |
|---|---|---|
| Will you track upstream releases? | Yes — re-vendor with patch replay | No — code diverges permanently |
| Is it on an untrusted-input path needing a pinned audit surface? | Strong signal for vendoring | — |
| Are you translating/adapting rather than consuming? | — | Strong signal for porting |
| Notice obligation | Per-crate license + `third_party/NOTICE` index | Crate `THIRD_PARTY_NOTICES.md` with §4(b) modification notice |

Either way, the license text travels with the code in-tree — never only in an external registry.