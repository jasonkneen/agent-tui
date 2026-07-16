# Convention: `cargo check -p agent-tui-bin` + `cargo test -p agent-tui-update --lib` is the fork's standard verify pair

**Rule.** Every substantive change to the Agent TUI fork — day-to-day work, version bumps, release preparation — is verified with the same two commands before it is committed or tagged:

```sh
cargo check -p agent-tui-bin
cargo test -p agent-tui-update --lib
```

The pair is the floor, not the ceiling: deeper test runs are welcome, but nothing ships having run less.

**Grounding.** Three independent surfaces prescribe exactly this pair:
- `CONTRIBUTING.md`, "Working on the fork": quotes both commands as the way to work on the fork.
- `suggested/skills/release-agent-tui.md`, step 2 "Verify": the same two commands, between the lockstep version bump and tagging.
- `suggested/agent-updates/agent-tui-version-bump-lockstep.md`: "Run the verify pair from the release skill (`cargo check -p agent-tui-bin`, `cargo test -p agent-tui-update --lib`) after the bump, and only then tag."

**Why:** the two commands cover the fork's two failure surfaces. `cargo check -p agent-tui-bin` proves the whole product binary graph still compiles after crate renames or dependency edits. `cargo test -p agent-tui-update --lib` exercises the updater crate — the one component whose bugs are invisible to CI but catastrophic in the field, because it owns the release-identity constants (`version.rs`), asset-name parsing, and channel resolution that installers and self-update depend on. A green build with a broken updater ships a release that cannot update itself.

**How to apply:** run the pair before committing any fork change and always between a version bump and pushing a tag. When writing or reviewing fork automation (release scripts, CI additions, skills), reference this exact pair rather than inventing a variant — if a change genuinely needs a different gate (e.g. it touches the pager crate's tests), add commands on top of the pair, never in place of it. A release PR whose description shows neither command has not been verified.