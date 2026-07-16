# Convention: a crate README updates in the same PR as its src/ — a src diff without a README diff is incomplete

**Rule.** A crate README that serves as the API reference for its crate's surface is updated in the same PR as any change to that crate's `src/`. The rule is enforced at review time: reviewers treat a `src/` diff without an accompanying README diff as an incomplete change, not a follow-up nicety.

**Grounding.**
- `crates/codegen/agent-tui-test-support/README.md` states it as a named rule in its own header: "**Freshness rule:** update this README in the same PR that changes `src/` — reviewers should treat a `src/` diff without a README diff as incomplete."
- The same README demonstrates the standard it demands: its module map documents the live API at constructor-and-method granularity (`MockInferenceServer` endpoints, `MockModelEntry` chainable builders, the scripted > required-auth > mode response precedence), which is only sustainable if every `src/` change carries its README delta.

**Why:** an API-reference README consumed by other crates' test authors (here: `agent-tui-shell` integration tests, the pager PTY harness, sampler tests) goes stale one merged PR at a time, and staleness in a *reference* doc is worse than absence — readers build against documented behavior that no longer exists. Coupling the doc to the diff at review time is the only point where the drift is cheap to catch; this is the workspace's same-change coupling law applied to in-repo API docs.

**How to apply:** when changing any `src/` file in a crate whose README documents the crate's API surface, write the corresponding README delta in the same change — new exports get rows, changed signatures get updated, removed items get deleted. When reviewing, check for the README diff before reading the code diff. When authoring a new shared-infrastructure crate, put the freshness rule in the README header itself, as agent-tui-test-support does, so the convention travels with the artifact.