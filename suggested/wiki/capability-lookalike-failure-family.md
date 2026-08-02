# The capability-lookalike failure family — records mistaken for capability across substrates

A persisted record that describes a *choice* or a *declared state* looks, on disk, exactly like a record that describes a *capability*. Reading the former as the latter is a recurring failure that has now surfaced in at least two independent substrates. This page names the family, catalogs its instances, and states the shared guard so a third substrate does not have to relearn it.

## The shape

Some piece of durable state (a config key, a descriptor field, a status flag) is written to record that *something was selected* or *something reached a preparatory state*. A later reader — a doc author, a support answer, a code reviewer, or a **drift adjudication** — cites that same state as evidence that the thing is **operational** (turns route, work dispatches, the vendor serves). The state was honest about what it recorded; the reader asked it a question it never answered.

The tell: the record is written by the *selection/preparation* step, but is being cited as proof of the *routing/execution* step — two different rungs, coupled only by hope.

## Known instances

| Substrate | The lookalike record | What it does NOT prove | The real proof |
|---|---|---|---|
| Agent TUI runtime | A vendor's runtime selection and its `<vendor>_model` key in `~/.agent-tui/runtime.toml` | That turns route to that vendor — a vendor below the inference rung persists its selection like any other while turns don't route | The **inference rung** of the readiness probe |
| Farm silos | A managed silo's `ready` descriptor | That work routes to it — `ready` is not proof of routability | A **passing smoke dispatch** admits it to rotation |

Both records are user- or machine-written statements of intent/preparation, not capability. In each case the workspace had to write a dedicated convention (`selection ≠ routing`; `ready ≠ routable`) to block the shortcut, and in each case the danger was sharpest in adjudication: a persisted key or descriptor weighed as capability evidence rests a decision on the wrong evidence class.

## The shared guard

1. **Classify the record by its writer.** State written by the selection/preparation step is choice/preparation evidence only. It answers "what was chosen / what stage was reached," never "does it operate."
2. **Establish capability only with the operational probe.** Routability is proven by exercising the operational rung (the inference-rung probe; the smoke dispatch), never by reading state.
3. **Never let a lookalike settle a rung/routability question** in a doc, skill, support answer, code review, or drift adjudication. A drift value resting on a persisted selection key or a `ready` flag is grounded on the wrong evidence class.

## Why it recurs

The lookalike record sits physically next to a genuine capability record — a `claude_model` key beside a routable vendor's `codex_model`, a `ready` descriptor beside a rotation-admitted silo — and proximity invites the inference that both mean the same thing. The guard is cheap once named; the cost is paid only when an unnamed instance quietly seeds a wrong adjudication.

## When a third instance appears

Add its row above and point the substrate's local convention here. Two independent instances already license this general artifact (the convergence ladder); a third is confirmation, not a new discovery. If the new substrate has no operational probe to name in the last column, that gap — not the lookalike — is the finding.
