---
name: smoke-test-agent-tui-release
description: Verify a just-published Agent TUI GitHub Release end-to-end — install.sh resolves the tag, downloads the asset, verifies its .sha256, and the installed binary launches and self-reports the expected version. Use after the Release workflow completes, and always on the alpha channel before any stable tag when the release surface changed.
---

# Smoke-test an Agent TUI release

Release-surface bugs are invisible to CI: the Release workflow (`.github/workflows/release.yml`) can go green while assets carry names the installers can't parse or `version.rs` constants point at a dead location (`suggested/agent-updates/alpha-channel-proves-release-surface-changes.md`). The only test that exercises the real contract is an actual release resolved by an actual installer.

## 1. Pick the channel form of the install command

All three forms use the fork's install script (`crates/codegen/agent-tui-pager/scripts/install.sh`, raw URL on branch `fork/agent-tui` of `jasonkneen/agent-tui`):

```bash
# stable — resolves GitHub releases/latest (excludes prereleases)
curl -fsSL https://raw.githubusercontent.com/jasonkneen/agent-tui/fork/agent-tui/crates/codegen/agent-tui-pager/scripts/install.sh | bash

# alpha — resolves semver-greatest release INCLUDING prereleases
AGENT_TUI_CHANNEL=alpha curl -fsSL …/install.sh | bash

# exact pin — bypasses channel resolution
curl -fsSL …/install.sh | bash -s 0.1.220
```

For a prerelease tag (`vX.Y.Z-alpha.N`) you MUST use the alpha or pinned form — `releases/latest` will never serve it, by design.

Windows counterpart: `irm …/install.ps1 | iex` (same resolution rules).

## 2. What a passing run must show

Per the alpha-channel convention, the smoke test passes only when ALL of:

1. `install.sh` **resolves** the intended release (correct tag picked for the channel).
2. It **downloads** the platform asset — name must match the contract `agent-tui-{version}-{os}-{arch}[.exe]` (`RELEASING.md`, "Release asset names (must match installers)").
3. It **verifies the `.sha256` sibling** — a missing or mismatched checksum file is a release defect, not an installer bug to work around.
4. The installed binary **runs** and **self-reports the expected version** — a binary reporting the previous version means a manifest lagged the tag (see the version-lockstep convention).

Run on at least one supported platform; more if the change touched per-platform asset names.

## 3. When this is mandatory vs advisable

- **Mandatory, alpha-first:** any change to the release surface — install scripts, asset name pattern, tag shape, repo id, download/API bases, `version.rs` constants. Cut `vX.Y.Z-alpha.1`, smoke-test with `AGENT_TUI_CHANNEL=alpha`, and only cut the stable tag after it passes. Stable users must never be the first consumers of a changed release surface.
- **Advisable, every release:** the release skill (`suggested/skills/release-agent-tui.md`) ends with a smoke-test step; run at least the stable-channel form after the workflow finishes.

## 4. On failure

Do not patch around it on the consumer side. Classify: asset-name mismatch or dead URL → run the change-release-identity workflow across all its surfaces; wrong self-reported version → re-check the four lockstep manifests; then cut the next alpha and re-test. Never promote a failed alpha to stable.