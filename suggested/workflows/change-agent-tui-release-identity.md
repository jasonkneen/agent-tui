# Workflow: change the Agent TUI release identity (repo id, asset names, or release URLs)

The fork's release identity — canonical repo (`jasonkneen/agent-tui`), default branch for raw script URLs (`fork/agent-tui`), asset name pattern (`agent-tui-{version}-{os}-{arch}[.exe]`), and download/API bases — is duplicated across installers, the updater, and docs. `AGENTS.md` is explicit: "When changing release URLs, asset names, or repo id, update **all** of" the surfaces below. A partial update ships installers that fetch from a dead location or an updater that can't find its own releases.

## Trigger

Any change to: the GitHub repo id, the default branch used in raw install-script URLs, the release asset name pattern, tag shape, or download/API base URLs.

## Steps

1. **`version.rs` constants** — `crates/codegen/agent-tui-update/src/version.rs` holds `GH_RELEASE_REPO`, download/API bases, and asset prefixes. This is the release identity's source of truth for the in-app updater; change it first.
2. **Install scripts** — `crates/codegen/agent-tui-pager/scripts/install.sh` and `install.ps1`: update defaults *and comments* (the raw URLs users curl live in doc snippets that embed the branch name).
3. **Updater hints** — `auto_update.rs` reinstall / manual-install hint strings.
4. **Docs** — `RELEASING.md`, `FORK.md`, `README.md`, and the getting-started guide (`crates/codegen/agent-tui-pager/docs/user-guide/01-getting-started.md`), which all embed raw install URLs and the repo id.
5. **Skills** — `suggested/skills/install-agent-tui.md` (and `release-agent-tui.md` if present) quote the install one-liners verbatim.

## Verify

- Grep the tree for the *old* repo id, branch name, and asset prefix — zero operative hits (historical decision records may keep them).
- Asset names in `.github/workflows/release.yml` still match what `install.sh` / `install.ps1` and the updater resolve ("Release asset names (must match installers)", `RELEASING.md`), including the `.sha256` siblings.
- Cut an alpha tag and smoke-test `install.sh` end-to-end before touching stable.

## Hard boundary

Never re-point any of these surfaces at `x.ai/cli` or `@xai-official/grok` — those are official Grok Build channels, not this fork's (`AGENTS.md`, `CONTRIBUTING.md`).