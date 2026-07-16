# Convention: captured test output and throwaway artifacts go to the per-goal scratch dir — never shared `/tmp/...`

**Rule.** During goal work, all captured test output, temp scripts, and throwaway artifacts are written to the goal's private scratch dir (`{SCRATCH_DIR}`, resolved from the plan's `{SCRATCH}` placeholder) — never to shared `/tmp/...` paths.

**Grounding.**
- `goal_rules.md`: "write captured test output, temp scripts, and throwaway artifacts to your private scratch dir {SCRATCH_DIR} — never to shared `/tmp/...` (skeptics and concurrent goals collide there)."
- `goal_continuation_directive.md`: "Save captured test output and artifacts to your scratch dir {scratch_dir} …, never shared `/tmp/...`; the plan's `{SCRATCH}` placeholder resolves there."
- `goal_strategist_prompt.md`: the scratch root is structured per role — "`{SCRATCH_ROOT}` — per-goal scratch root with the implementer's and each skeptic's captured test output / artifacts (`implementer/`, `skeptic-*/`)".

**Why:** two failure modes. First, collisions — skeptic verifiers and concurrently running goals share `/tmp`, so files get overwritten or read cross-goal. Second, evidence loss — the verifier and strategist **audit saved scratch evidence instead of rebuilding it**; output written to `/tmp` is invisible to that audit, so honest work gets refuted for lack of durable proof.

**How to apply:** resolve the scratch path from the goal rules or the plan's `{SCRATCH}` placeholder before running any capture; pipe test runs and generated artifacts there. When diagnosing a stuck goal, read the scratch root's `implementer/` and `skeptic-*/` subdirs to see what evidence each role actually produced. Treat any goal-work write to a bare `/tmp/...` path as a defect, even when it works locally.
