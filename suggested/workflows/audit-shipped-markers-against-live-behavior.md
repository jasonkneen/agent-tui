# Workflow: audit '(shipped)' markers against live behavior (bidirectional)

The shipped-marker convention makes the annotation operative: a claim inside a "(shipped)"-marked section outranks unmarked and older statements in drift adjudication. Its own Why-section names the exposure this audit closes: "a future editor could add or drop it casually, silently changing the evidentiary weight of every claim in the section." Because the marker is the doc-side proxy for on-wire evidence, it must itself be audited against the wire. Per the verification convention, the check runs both directions: record → reality and reality → record.

## 1. Enumerate the marked sections from present reality

Grep the workspace for `(shipped)` (and near-variants like `shipped)` in headings). Enumerate from the live tree, not from a remembered list — today's known instance is `docs/LOCAL_CLI_AUTH.md`'s "TUI usage (shipped)" table, but that enumeration is an illustrative snapshot.

## 2. Direction one — marked claims → live behavior

For each marked section, extract its individually checkable claims and verify each against the running product. For vendor-runtime claims, run the probe-vendor-readiness-rung skill on a current build launched by path: does `/runtime` show what the table says, does `/model` load the catalog the section claims, does the persistence key land in `~/.agent-tui/runtime.toml`?

- **Claim holds** → no action.
- **Claim fails on the wire** → the marker is manufacturing false evidence. Register a drift-ledger row (on-wire behavior is canonical for live services) and either fix the claim or strip the marker in the reconciliation — never leave a false claim under a shipped marker.

## 3. Direction two — live behavior → markers

For product behavior that demonstrably exists (probe results, shipped release notes), check whether the doc sections describing it carry the marker. An unmarked live-behavior section is a weaker witness than it should be: in the /model-scope reconciliation the marked section plus timestamps carried the adjudication. Add the marker — with the change attributed and dated, since an unattributed value cannot serve as evidence later.

Also check the inverse mislabel: sections describing planned or aspirational behavior that carry the marker. Strip it; a design intention marked shipped is the convention's named failure.

## 4. Register and record

This sweep is a standing bidirectional record-vs-reality audit: register it in the standing-audit registry in the same change it lands, with scope (all `(shipped)`-marked sections plus live-behavior docs), trigger (each release cut, and any drift adjudication that leaned on a marker), and record each run's outcome in its registry row.
