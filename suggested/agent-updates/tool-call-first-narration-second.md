# Convention: tool-call first, narration second — a narrated action without its tool call did not happen

**Rule.** During active multi-step agent work, four discipline rules apply:

1. **Tool-call first, narration second.** Any past-tense or present-continuous prose describing an action ("I launched...", "I'm now reading...") MUST be paired with the corresponding tool call in the same assistant response. A turn ending on such a sentence with no tool call means the action did not happen — the launch announcement is written only AFTER the tool call appears in the same response, never on its own.
2. **No permission-seeking on in-flight work.** User-facing questions are for genuine ambiguity that changes the approach (two reasonable architectures, a missing requirement) — never cadence negotiation, confirmation of the obvious next step, or re-affirming an already-authorised plan.
3. **Todo lists are a scratchpad, not a deliverable.** Keep roughly one item `in_progress` and update as you go, but don't over-decompose or spend turns on bookkeeping at the expense of the work.
4. **Don't stop with easy work left undone.** Before ending a turn, check for obvious unblocked remaining work; stopping short only wastes a loop round. Legitimate stops: a live background wait, a real user decision, or a hard external blocker — stated explicitly.

**Grounding.**
- `goal_task_discipline.md` (`<task_completion_discipline>` block): states all four rules verbatim, opening with the failure diagnosis — "Multi-step goal work fails when the model narrates an action without executing it, asks for permission to continue an obviously-in-flight task, or stops with easy work still undone."
- `goal_rules.md` composes the block into every goal turn via `{DISCIPLINE_BLOCK}`, alongside the same tracking rule ("keep ≥1 `in_progress` ... mark each done immediately (do not batch)").

**Why:** narration-without-execution is the failure the harness cannot detect from the transcript alone — the prose reads identically whether the action happened or not. Rule 1 makes the transcript self-verifying: presence of the tool call in the same response is the proof. Rules 2 and 4 exist because the goal loop re-engages the agent anyway, so premature hand-backs and permission questions purely waste rounds.

**How to apply:** when authoring any multi-turn agent prompt (harness template, subagent system prompt, chain phase), include this discipline block or an equivalent — it is a reusable contract, not goal-harness-specific. When debugging a stalled agent run, grep the transcript for action-describing prose in turns with no tool calls: each hit is a narrated-but-unexecuted action, the exact defect rule 1 exists to prevent.