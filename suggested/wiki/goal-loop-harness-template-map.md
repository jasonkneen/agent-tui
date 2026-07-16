# Goal-loop harness — template roles from plan to verified summary

The goal harness in `agent-tui-shell` is a multi-role loop assembled from prompt templates under `src/session/templates/`. Each role is a separate template with a distinct contract. This doc maps the loop end to end.

## The loop

```
Goal set → Plan Writer (once) → Implementer (many turns, nudged) → Adversarial Verifier
              │                        ▲                                │
              │                        │  gaps inlined via              │ refuted
              │                        │  continuation directive        ▼
              │                        └──────── Strategist (after N failed rounds)
              └─ plan.md                                                │
                                                     not refuted → Summarizer → user
```

## Roles

### 1. Goal Plan Writer (`goal_planner_prompt.md`)
- Runs **once** at goal creation. Writes `{PLAN_FILE}` (its only permitted write) — the single source of truth for "what was supposed to happen", read by implementer, verifiers, and classifier. The user never sees it.
- For objectives naming an established canon (a named game, algorithm, protocol, "clone of X"), it must **web-research the defining mechanics first**, never plan from memory. Generic archetypes ("a todo app") skip research.
- Criteria are **folded by grouping**, never dropped, to fit the acceptance-criteria cap (a ceiling, not a target).

### 2. Implementer (`goal_rules.md` + `goal_plan_block.md` + `goal_task_discipline.md`)
- Works the plan's `## Task checklist` in order, flipping `- [ ]` → `- [x]` in the plan file — **the harness mines the first unchecked box as the next-step nudge**, so a stale checklist produces stale nudges.
- Deviations go as terse single bullets into the plan's one `## Deviations` section; existing plan items are never edited.
- Discipline rules: tool-call first / narration second; never ask permission to continue in-flight work; don't stop with unblocked work remaining.
- Declares completion via `{GOAL_TOOL}(completed: true)` only after running the plan's `## Verification plan` itself; `blocked_reason` only when truly stuck.

### 3. Continuation directive (`goal_continuation_directive.md`)
- The per-turn `<system-reminder>` nudge: goal state, token/elapsed counters, plan pointer, **outstanding verifier gaps inlined**, optional strategist note, and the mined next step.

### 4. Adversarial Verifier (`goal_verifier_prompt.md`)
- Job is to **refute**; defaults to `refuted: true` when uncertain (a false pass ends the loop wrongly).
- **Audits, doesn't author**: judges the implementer's committed tests and captured scratch evidence rather than rebuilding its own.
- Inputs include `PLAN_CHANGES` — a diff of plan edits during the run; a weakened or self-serving criterion is itself grounds to refute.
- **Anti-ratchet rule**: on re-verification the bar does not rise. New objections are valid only for demonstrable shipped-behavior defects or unmet gating criteria — never fresh nitpicks each round.

### 5. Strategist (`goal_strategist_prompt.md`)
- Runs after several consecutive failed rounds flagging different gaps (whack-a-mole). Investigates the raw traces itself (`chat_history.jsonl`, `events.jsonl`, plan, scratch root) and recommends **one structural change** (refactor for testability, split a monolith, rewrite a subsystem) — never another patch. The implementer sees only a short pointer.

### 6. Summarizer (`goal_summarizer_prompt.md`)
- Runs on verified success. **Read-only** — no edits, no commands. Writes the single closing user message: one sentence naming what was delivered, then exact how-to-run steps. Does not echo the verifier's review.

## Shared substrate
- Session traces: `{SESSION_TRACES_DIR}/chat_history.jsonl` (transcript + inlined gap feedback), `events.jsonl` (verdict history), `goal/plan.md`.
- Scratch: per-goal `{SCRATCH_ROOT}` with `implementer/` and `skeptic-*/` subdirs holding captured test output — the verifier's primary evidence.
