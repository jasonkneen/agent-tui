# Convention: no test theater — a passing test must prove the SHIPPED code works on the real path

**Rule.** A test counts as evidence only when it drives the real shipped entry point. Four moves are banned outright:

1. Hard-coding the expected value.
2. Starting past the thing under test.
3. Re-implementing the code under test inside the test.
4. Reporting success without driving the real entry point.

Where a behavior cannot be driven end-to-end, the sanctioned substitute is a **static/structural check** (assert the artifact exists in source) **plus a unit test of the real shipped function** — not a flaky end-to-end run.

**Grounding (three independent surfaces).**
- `goal_rules.md`: "NO TEST THEATER: a passing test must prove the SHIPPED code works on the real path. Never hard-code the expected value, start past the thing under test, re-implement the code under test inside the test, or report success without driving the real entry point. A test that passes while the program is broken is worse than none."
- `goal_continuation_directive.md`: "Tests must drive the SHIPPED code on the real path — no hard-coded values, no starting past the thing under test, no re-implementing it."
- `goal_verifier_prompt.md`: the verifier audits committed tests as its primary proof and treats prose claims as "NOT evidence" for code-change goals; `goal_strategist_prompt.md` names test theater ("tests that don't drive the real shipped path") as a canonical root cause of unconvergeable goals.

**Why:** the harness's verifier audits committed tests and captured run output instead of rebuilding them — theatrical tests pass local checks, then get the goal refuted (or worse, wrongly passed). A green suite that doesn't exercise shipped code silently converts "broken" into "done".

**How to apply:** when writing any test as goal evidence, trace the call path from the test to the shipped entry point before trusting it; if the path starts inside a re-implementation or a pre-seeded state, replace it with a structural check plus a unit test of the real function. When reviewing, treat each of the four banned moves as an automatic gap.
