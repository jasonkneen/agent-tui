---
name: diagnose-agent-tui-install-update-failure
description: Given a failing install.sh/install.ps1 run or a misbehaving in-app updater (perpetual update re-offer, missing asset, checksum mismatch, wrong channel), identify which release surface drifted and which procedure repairs it.
---

# Diagnose a broken Agent TUI install or self-update

The release surface is a set of distributed constants — version string, tag shape, asset-name pattern, repo id, download/API bases — parsed by three consumers: `install.sh`, `install.ps1`, and the in-app updater (`agent-tui-update`). Most failures are one of these constants drifting out of agreement. Work the symptom table top-down.

## Symptom → drifted surface

| Symptom | Likely cause | Repair |
|---------|--------------|--------|
| Updater perpetually re-offers the version it already has | A manifest lagged the tag — partial version bump, so the binary self-reports the old version | Re-run the lockstep bump: all four manifests (`agent-tui-bin`, `agent-tui-pager`, `agent-tui-update` Cargo.tomls + npm `package.json` with optionalDependency pins) must agree; grep for the old version string |
| `npm i -g @agent-tui/agent-tui` installs a stale platform binary | optionalDependency pins in `package.json` lag the release | Same lockstep bump — the pins are part of manifest #4 |
| `install.sh` 404s on the asset download | Asset name pattern in `.github/workflows/release.yml` no longer matches what installers expect (`agent-tui-{version}-{os}-{arch}[.exe]`), or repo id / download base changed without the full sweep | Run the change-release-identity workflow: `version.rs` constants, both install scripts (defaults *and comments*), `auto_update.rs` hints, docs |
| Checksum verification fails or is skipped | A release asset shipped without its `.sha256` sibling — a release defect by convention | Re-upload the sibling; fix the release workflow if it dropped it |
| Stable user received a prerelease, or alpha user can't see the new alpha | Tag shape wrong: stable is `vX.Y.Z` (no hyphen suffix), alpha is `vX.Y.Z-alpha.N` with `prerelease = true`. Stable resolves `releases/latest` (GitHub excludes prereleases); alpha resolves semver-greatest including prereleases | Check the tag and the release's prerelease flag; re-tag correctly |
| Installed binary is `grok`, config lands in `~/.grok` | The user ran the official `x.ai/cli` installer — that installs upstream Grok Build, never this fork | Point them at the fork one-liner (see `install-agent-tui`); never re-point fork docs at official channels |
| Pinned install (`bash -s 0.1.220`) fails but latest works | That version's assets were renamed or removed after a release-identity change | Verify old-pattern assets still exist for historical versions, or document the floor version |

## Diagnostic steps

1. Reproduce with the exact channel the user used: default, `AGENT_TUI_CHANNEL=alpha`, pinned version, or npm.
2. Compare the binary's self-reported version against the four manifests and the tag.
3. List the GitHub Release's actual asset names and check them against the contract in `RELEASING.md` (including `.sha256` siblings).
4. Check `crates/codegen/agent-tui-update/src/version.rs` constants against the repo id and download/API bases the installers use.

## Prevention

Any fix that touches these surfaces is a release-identity change: prove it on an alpha tag with an end-to-end `install.sh` smoke-test before cutting stable.
