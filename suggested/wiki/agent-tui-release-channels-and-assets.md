# Agent TUI release channels — tag shapes, resolution rules, and the asset-name contract

Agent TUI ships through GitHub Releases with two channels distinguished purely by tag shape, plus a strict asset-naming contract the installers and in-app updater depend on. Constants live in `crates/codegen/agent-tui-update/src/version.rs` (`RELEASING.md`).

## Channels

| Channel | Tag shape | GitHub Release flag | Resolved by |
|---------|-----------|---------------------|-------------|
| stable | `v0.1.220` (no hyphen suffix) | latest non-prerelease | installers & updater via `releases/latest` |
| alpha | `v0.1.220-alpha.1` | `prerelease = true` | newest release **including** prereleases (semver-greatest) |

- Tags are `v{version}`; pushing a `v*` tag triggers `.github/workflows/release.yml`, which builds and uploads all assets.
- **stable / enterprise** consumers resolve GitHub `releases/latest` — a prerelease can never leak to them because GitHub excludes prereleases from `latest`.
- **alpha** consumers (`AGENT_TUI_CHANNEL=alpha` on `install.sh`) resolve the semver-greatest release including prereleases.
- Users can also pin an exact version: `install.sh | bash -s 0.1.220`.

## The asset-name contract

Release asset names **must match what the installers expect** (`RELEASING.md`):

```
agent-tui-{version}-macos-aarch64
agent-tui-{version}-macos-x86_64
agent-tui-{version}-linux-x86_64
agent-tui-{version}-windows-x86_64.exe
```

Every asset ships with a `.sha256` sibling. Three consumers parse these names: `install.sh`, `install.ps1`, and the in-app updater (`agent-tui-update`, modes `internal` + `gh-release`). Changing the pattern is a release-identity change — follow the change-release-identity workflow and update `version.rs`, both install scripts, `auto_update.rs` hints, and the docs together (`AGENTS.md`).

## What is NOT a channel here

The x.ai CDN and `@xai-official/grok` npm package serve upstream Grok Build only — they are not fork channels and installers are never pointed at them. The fork's optional npm package is `@agent-tui/agent-tui`, assembled and published manually (`RELEASING.md`).