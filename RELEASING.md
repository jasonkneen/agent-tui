# Releasing Agent TUI

Maintainer guide for shipping binaries, installers, and optional npm packages
for this fork ([jasonkneen/agent-tui](https://github.com/jasonkneen/agent-tui)).

End-user install docs: [README.md](README.md) · [FORK.md](FORK.md) ·
[getting started](crates/codegen/agent-tui-pager/docs/user-guide/01-getting-started.md).

---

## What ships where

| Channel | Who uses it | Source of truth |
|---------|-------------|-----------------|
| **GitHub Releases** | Default install (`install.sh` / `install.ps1`) and in-app auto-update (`internal` + `gh-release`) | Tag `v*` → [`.github/workflows/release.yml`](.github/workflows/release.yml) |
| **npm** (`@agent-tui/agent-tui`) | Optional `npm i -g` | Manual assemble + publish (see below) |
| **x.ai CDN / `@xai-official/grok`** | Official Grok Build only — **not** this fork | Upstream private pipeline |

### Release asset names (must match installers)

```
agent-tui-{version}-macos-aarch64
agent-tui-{version}-macos-x86_64
agent-tui-{version}-linux-x86_64
agent-tui-{version}-windows-x86_64.exe
```

Plus a `.sha256` sibling for each asset. Tags are `v{version}` (e.g. `v0.1.220`).

| Channel | Tag shape | GitHub Release flag |
|---------|-----------|---------------------|
| stable | `v0.1.220` (no hyphen suffix) | latest non-prerelease |
| alpha | `v0.1.220-alpha.1` | prerelease = true |

Installers and the updater resolve:

- **stable / enterprise** → GitHub `releases/latest`
- **alpha** → newest release including prereleases (semver-greatest)

Constants live in `crates/codegen/agent-tui-update/src/version.rs`:

| Constant | Value |
|----------|-------|
| `GH_RELEASE_REPO` | `jasonkneen/agent-tui` |
| `RELEASE_ASSET_PREFIX` | `agent-tui` |
| `MANAGED_BIN_NAME` | `agent-tui` |
| `NPM_PACKAGE` | `@agent-tui/agent-tui` |

---

## Prerequisites (one-time)

1. **GitHub Actions** enabled on the repo; workflows under `.github/workflows/`.
2. Default branch currently: **`fork/agent-tui`**. Install script raw URLs pin that branch.
3. **No extra secrets** for binary releases — `GITHUB_TOKEN` is enough for
   `softprops/action-gh-release`.
4. Optional **npm**: create `NPM_TOKEN` (or `NODE_AUTH_TOKEN`) with publish
   rights for the `@agent-tui` scope; not wired into CI yet.
5. Local tools for a dry run: Rust `1.92.0` (see `rust-toolchain.toml`), `protoc`,
   `gh` authenticated to the fork.

If the default branch is renamed to `main` later, update every
`raw.githubusercontent.com/jasonkneen/agent-tui/fork/agent-tui/...` URL in:

- `README.md`, `FORK.md`, `RELEASING.md`
- `crates/codegen/agent-tui-pager/scripts/install.{sh,ps1}`
- `crates/codegen/agent-tui-update/src/auto_update.rs` (manual install hints)
- `crates/codegen/agent-tui-pager/docs/user-guide/01-getting-started.md`
- `suggested/skills/install-agent-tui.md`

---

## Cut a release (happy path)

### 1. Version bump

Keep these in lockstep when shipping:

| File / package | Field |
|----------------|-------|
| `crates/codegen/agent-tui-bin/Cargo.toml` | `version` |
| `crates/codegen/agent-tui-pager/Cargo.toml` | `version` |
| `crates/codegen/agent-tui-update/Cargo.toml` | `version` |
| `crates/codegen/agent-tui-pager/npm/agent-tui/package.json` | `version` + `optionalDependencies` pins |
| (optional) shell / workspace crates that share the shipping version | |

`agent-tui-version` may stay on a separate `0.2.0-dev`-style line; the **binary**
version comes from `AGENT_TUI_VERSION` / `GROK_VERSION` env at build time (set
by the release workflow) or `CARGO_PKG_VERSION` of `agent-tui-bin`.

### 2. Sanity check

```sh
cargo check -p agent-tui-bin
cargo test -p agent-tui-update --lib
# optional: cargo build --profile release-dist -p agent-tui-bin --features release-dist
```

### 3. Commit, tag, push

```sh
git status
git add -A
git commit -m "release: v0.1.220"

git tag -a v0.1.220 -m "Agent TUI v0.1.220"
git push origin HEAD
git push origin v0.1.220
```

### 4. Watch Actions

1. Open **Actions → Release** for the tag run.
2. Matrix builds: `macos-aarch64`, `macos-x86_64`, `linux-x86_64`, `windows-x86_64`.
3. **Publish GitHub Release** job attaches assets and generates notes.
4. Confirm https://github.com/jasonkneen/agent-tui/releases/latest

### 5. Smoke-test install

```sh
# fresh shell / temp home
curl -fsSL https://raw.githubusercontent.com/jasonkneen/agent-tui/fork/agent-tui/crates/codegen/agent-tui-pager/scripts/install.sh | bash -s 0.1.220
agent-tui --version
```

Windows:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/jasonkneen/agent-tui/fork/agent-tui/crates/codegen/agent-tui-pager/scripts/install.ps1))) -Version 0.1.220
agent-tui --version
```

### 6. Optional: npm publish

After assets exist (or after a local multi-platform build):

```sh
export GROK_DARWIN_ARM64=/path/to/agent-tui   # etc. for each platform env var
# see assemble-platform-packages.js for GROK_DARWIN_X64, GROK_LINUX_*, GROK_WIN32_*

node crates/codegen/agent-tui-pager/npm/agent-tui/scripts/assemble-platform-packages.js

# Publish platform packages first, then the meta package:
for d in \
  crates/codegen/agent-tui-pager/npm/agent-tui-darwin-arm64 \
  crates/codegen/agent-tui-pager/npm/agent-tui-darwin-x64 \
  crates/codegen/agent-tui-pager/npm/agent-tui-linux-x64 \
  crates/codegen/agent-tui-pager/npm/agent-tui-linux-arm64 \
  crates/codegen/agent-tui-pager/npm/agent-tui-win32-x64 \
  crates/codegen/agent-tui-pager/npm/agent-tui-win32-arm64 \
  crates/codegen/agent-tui-pager/npm/agent-tui
do
  (cd "$d" && npm publish --access public)
done

# Dist-tags
npm dist-tag add @agent-tui/agent-tui@0.1.220 latest
# npm dist-tag add @agent-tui/agent-tui@0.1.220-alpha.1 alpha
```

---

## Manual / dispatch release

**Actions → Release → Run workflow**

- Leave **version** empty when dispatching from a tag context, or set e.g.
  `0.1.220` for an untagged dry run (`0.0.0-dev.<run_number>` if empty and not
  on a tag).
- Prefer a real `v*` tag for production so installers that call
  `releases/latest` see a proper release.

---

## Installer & auto-update env vars

| Variable | Purpose | Default |
|----------|---------|---------|
| `AGENT_TUI_GITHUB_REPO` | `owner/repo` for releases | `jasonkneen/agent-tui` |
| `AGENT_TUI_CHANNEL` / `GROK_CHANNEL` | `stable` \| `alpha` \| `enterprise` | `stable` |
| `AGENT_TUI_HOME` / `GROK_HOME` | Config + downloads root | `~/.agent-tui` |
| `AGENT_TUI_BIN_DIR` / `GROK_BIN_DIR` | Binary install directory | `$HOME/bin` under config home |
| `GROK_INSTALLER` | Force updater path: `internal` \| `gh-release` \| `npm` | auto-detected |

In-app updater modes (`crates/codegen/agent-tui-update`):

| `installer` | Version resolve | Download |
|-------------|-----------------|----------|
| `internal` (install.sh default) | GitHub REST API | HTTPS asset URL (no `gh` CLI) |
| `gh-release` | `gh release list` | `gh release download` |
| `npm` | `npm view @agent-tui/agent-tui` | `npm i -g` |

---

## Local dry-run of the release binary

```sh
export AGENT_TUI_VERSION=0.1.220
export GROK_VERSION=0.1.220
cargo build --profile release-dist -p agent-tui-bin --features release-dist
# Unix: target/release-dist/agent-tui
./target/release-dist/agent-tui --version
```

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| Installer: “failed to fetch latest version” | No published release yet, or wrong repo | Tag + wait for Actions; check `AGENT_TUI_GITHUB_REPO` |
| Installer: “not yet available for your system” | Matrix missing that OS/arch | Extend `release.yml` matrix or install from source |
| Auto-update still points at xAI | Old binary built before fork rewires | Reinstall from this repo’s install script |
| `raw.githubusercontent.com` 404 | Wrong branch in URL | Use default branch `fork/agent-tui` (or update URLs after rename) |
| npm install missing binary | Platform package not published / `--no-optional` | Publish all six packages; avoid `--no-optional` |
| Release job fails on protoc | Runner missing protobuf | Workflow installs protoc; re-run job |
| Windows Defender / SmartScreen | Unsigned binary | Expected for unsigned OSS; users may need “More info → Run anyway” |

---

## CI reference

| Workflow | Trigger | Job |
|----------|---------|-----|
| [`.github/workflows/ci.yml`](.github/workflows/ci.yml) | push / PR on `main`, `master`, `fork/**` | `cargo check` bin + update; update lib tests |
| [`.github/workflows/release.yml`](.github/workflows/release.yml) | tag `v*` or `workflow_dispatch` | multi-OS build → GitHub Release |

---

## Checklist (copy into PR / release notes)

```
- [ ] Version bumped in bin / pager / update / npm meta package
- [ ] cargo check -p agent-tui-bin passes
- [ ] cargo test -p agent-tui-update --lib passes (or CI green)
- [ ] Tag vX.Y.Z pushed
- [ ] Actions Release workflow green
- [ ] Assets present on the GitHub Release page
- [ ] install.sh -s X.Y.Z works on at least one host
- [ ] agent-tui --version matches tag
- [ ] (optional) npm packages published + dist-tag set
```
