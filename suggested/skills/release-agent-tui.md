---
name: release-agent-tui
description: Cut an Agent TUI GitHub Release — version bump, tag v*, watch Actions, smoke-test install.sh, optional npm publish. Use when shipping a new agent-tui version.
---

# Release Agent TUI

Follow the full checklist in **`RELEASING.md`** at the repo root. Summary:

## 1. Bump versions (lockstep)

- `crates/codegen/agent-tui-bin/Cargo.toml`
- `crates/codegen/agent-tui-pager/Cargo.toml`
- `crates/codegen/agent-tui-update/Cargo.toml`
- `crates/codegen/agent-tui-pager/npm/agent-tui/package.json` (+ optionalDependency pins)

## 2. Verify

```bash
cargo check -p agent-tui-bin
cargo test -p agent-tui-update --lib
```

## 3. Tag and push

```bash
git tag -a vX.Y.Z -m "Agent TUI vX.Y.Z"
git push origin HEAD
git push origin vX.Y.Z
```

Alpha: `vX.Y.Z-alpha.N` (marks GitHub Release as prerelease).

## 4. Confirm Actions

- Workflow: **Release** (`.github/workflows/release.yml`)
- Assets: `agent-tui-{version}-macos-aarch64`, `macos-x86_64`, `linux-x86_64`, `windows-x86_64.exe`
- Repo: `jasonkneen/agent-tui`

## 5. Smoke-test

```bash
curl -fsSL https://raw.githubusercontent.com/jasonkneen/agent-tui/fork/agent-tui/crates/codegen/agent-tui-pager/scripts/install.sh | bash -s X.Y.Z
agent-tui --version
```

## 6. Optional npm

See `RELEASING.md` § npm publish (`assemble-platform-packages.js` then six platform packages + meta).

## Constants (do not re-point at xAI CDN)

`crates/codegen/agent-tui-update/src/version.rs`:

- `GH_RELEASE_REPO = "jasonkneen/agent-tui"`
- `RELEASE_ASSET_PREFIX = "agent-tui"`
- `NPM_PACKAGE = "@agent-tui/agent-tui"`
