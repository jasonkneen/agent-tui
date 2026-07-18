# Workflow: reconcile the Grok model-pick persistence drift (runtime.toml key vs sampler-owned)

Two surfaces give two values for one fact — where the **Grok** model pick persists:

- **Value A — runtime.toml.** `suggested/agent-updates/model-command-is-grok-only.md` quotes an earlier `docs/LOCAL_CLI_AUTH.md` revision: "`/model` only switches **Grok** models. Choice is persisted in `~/.agent-tui/runtime.toml`."
- **Value B — outside the vendor-scoped scheme.** `suggested/wiki/runtime-toml-reference.md` lists only `codex_model` and `claude_model`, states the `<vendor>_model` pattern is for "catalog-shipped" vendors, and says "Grok's model pick predates this file's vendor-scoped scheme and rides the built-in sampler catalog" — no `grok_model` key exists.

The values may also be partially reconcilable — Value A's sentence could have meant the *runtime* choice, not the model choice — which is exactly why the fact needs adjudication rather than a guess: a doc's rationalization of an inconsistency is not evidence.

## 1. Register the drift

Open an open-drift-ledger row per the register-doc-drift skill. Disputed fact: *where the Grok model selection persists (a runtime.toml key / sampler-side state / not persisted)*. Competing values A and B with verbatim quotes, source paths, and modification timestamps. Canonical: **TBD**. Per convention, the TBD row still freezes the fact — the runtime.toml reference and any new doc must assert neither value until this closes.

Note the evidentiary weights up front: Value A's quote is lifted from an earlier revision of a doc whose current text no longer contains it (a phantom-grounding signal), and neither current statement sits in a "(shipped)"-marked section covering Grok's pick specifically.

## 2. Establish ground truth on the wire

For live services, on-wire behavior is canonical over any doc:

1. Launch a current build by path from the build output (never Spotlight/Dock/bundle id).
2. With `/runtime grok` active, pick a non-default model via `/model`.
3. Read `~/.agent-tui/runtime.toml` (resolving the home via `$AGENT_TUI_HOME`, legacy `$GROK_HOME` fallback): did any key change?
4. Restart the TUI and check whether the pick survived; if it did but runtime.toml did not change, locate the actual writer's on-disk output — the writer's output is canonical when docs disagree about a machine-written path.

## 3. Adjudicate and close

Write the observed persistence location into the ledger row as canonical, with author and date. Then:

- Update `suggested/wiki/runtime-toml-reference.md` to state Grok's actual behavior (a `grok_model`-style key, or an explicit "persists at <path>, outside runtime.toml" note) — the reference claims to cover "every key, its writer, and when it applies", so a Grok pick living elsewhere must be named, not omitted.
- Scrub operative restatements of the losing value, preserving historical decision records.
- Close the row only when the canonical value is written into the owning convention/reference doc — a reconciliation is finished only then.
