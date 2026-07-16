# Workflow: assemble and publish the @agent-tui/agent-tui npm package

Unlike GitHub Releases (tag `v*` → `.github/workflows/release.yml`, fully automated), the fork's npm channel is **manual**: `RELEASING.md`'s "What ships where" table lists `npm (@agent-tui/agent-tui)` with source of truth "Manual assemble + publish". Because nothing automates it, it is the channel most likely to drift from the binary release — this workflow keeps it honest.

## Preconditions

- The corresponding GitHub Release `v{version}` exists and has passed its install smoke-test — npm consumers pull the same platform binaries, so a broken release surface breaks them too.
- The four-manifest lockstep bump is complete, including `crates/codegen/agent-tui-pager/npm/agent-tui/package.json` **and its optionalDependency pins** (`suggested/agent-updates/agent-tui-version-bump-lockstep.md`). A lagging pin silently installs a platform binary from the *previous* release.

## Steps

1. **Confirm the package identity.** The package is `@agent-tui/agent-tui` — never `@xai-official/grok`, which is upstream Grok Build's channel and off-limits for fork distribution (`RELEASING.md`; `suggested/agent-updates/fork-never-ships-through-official-grok-channels.md`).
2. **Verify the manifest.** In `crates/codegen/agent-tui-pager/npm/agent-tui/package.json`, check that `version` equals the just-shipped release version and every optionalDependency pin references that same version's platform packages. Grep for the previous version string in the npm directory — zero operative hits.
3. **Check the package README's install text.** `crates/codegen/agent-tui-pager/npm/agent-tui/README.md` quotes the GitHub Releases installer as preferred and carries the explicit upstream disclaimer ("Official `https://x.ai/cli/install.sh` installs upstream Grok Build (`grok`), not Agent TUI"). Keep both intact — the README ships to npmjs.com where confused users are most likely to land.
4. **Assemble and publish** following the "Manual assemble + publish" section of `RELEASING.md` at the repo root — that document is the authoritative step list; this workflow does not restate it.
5. **Verify from the consumer side.** On a clean machine or container: `npm i -g @agent-tui/agent-tui`, then confirm the installed `agent-tui` launches and self-reports the published version.

## Verify

- `npm view @agent-tui/agent-tui version` matches the GitHub Release tag.
- The globally installed binary's self-reported version matches — a mismatch means an optionalDependency pin lagged (the lockstep convention's failure mode) and requires a corrected publish.

## Hard boundary

Never publish to, depend on, or document `@xai-official/grok` as a fork channel — it belongs to upstream's private pipeline.