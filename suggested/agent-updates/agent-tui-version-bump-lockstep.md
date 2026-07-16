# Convention: Agent TUI versions bump in lockstep across all four manifests — never partially

**Rule.** An Agent TUI version bump touches all four version-bearing manifests in the same change:

1. `crates/codegen/agent-tui-bin/Cargo.toml`
2. `crates/codegen/agent-tui-pager/Cargo.toml`
3. `crates/codegen/agent-tui-update/Cargo.toml`
4. `crates/codegen/agent-tui-pager/npm/agent-tui/package.json` — including its optionalDependency pins

No tag is pushed until all four agree. A partial bump is a defect even if the build is green.

**Grounding.**
- `suggested/skills/release-agent-tui.md`, step 1 is titled "**Bump versions (lockstep)**" and enumerates exactly these four files, with "(+ optionalDependency pins)" called out on the npm package.
- `RELEASING.md`: tags are `v{version}` and "Release asset names (must match installers)" all embed `{version}` (`agent-tui-{version}-macos-aarch64`, …). Three consumers parse those names — `install.sh`, `install.ps1`, and the in-app updater (`agent-tui-update`).
- `suggested/wiki/agent-tui-release-channels-and-assets.md`: stable/alpha channel resolution is pure semver over these tags — a manifest that lags the tag produces a binary that reports the wrong version to its own updater.

**Why:** the version string is a distributed constant. The tag drives `.github/workflows/release.yml`, the workflow names assets from the version, the installers and updater resolve releases by comparing semver against the binary's self-reported version, and the npm package pins platform binaries via optionalDependencies. If any one manifest lags, the failure is not a compile error — it is an updater that perpetually re-offers the "new" version it already has, or an npm install that pins a platform binary from the previous release.

**How to apply:** treat the version bump as one atomic edit — grep the tree for the old version string after bumping and confirm the only remaining hits are historical (changelogs, decision records). Run the verify pair from the release skill (`cargo check -p agent-tui-bin`, `cargo test -p agent-tui-update --lib`) after the bump, and only then tag. When reviewing a release PR, a diff that changes fewer than four manifests is incomplete by definition.
