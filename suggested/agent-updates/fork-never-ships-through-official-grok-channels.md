# Convention: the fork never distributes through official Grok channels — x.ai/cli and @xai-official/grok stay upstream-only

**Rule.** Agent TUI's distribution surfaces — install scripts, npm package metadata, updater endpoints, and every doc that quotes an install command — point only at the fork's own channels (GitHub Releases on `jasonkneen/agent-tui`, optional `@agent-tui/agent-tui` on npm). The official channels `https://x.ai/cli/install.sh` and `@xai-official/grok` belong to upstream Grok Build and are never referenced as this fork's distribution path.

**Grounding.**
- `AGENTS.md`: "Do **not** re-point installers at `x.ai/cli` or `@xai-official/grok` for this fork."
- `CONTRIBUTING.md`: "Do not reintroduce `x.ai/cli` or `@xai-official/grok` as this fork's default distribution — those remain official Grok Build channels."
- `RELEASING.md`, "What ships where": the `x.ai CDN / @xai-official/grok` row is marked "Official Grok Build only — **not** this fork", with source of truth "Upstream private pipeline".
- npm `README.md`: "Official `https://x.ai/cli/install.sh` installs upstream Grok Build (`grok`), not Agent TUI."

**Why:** the official channels ship a different binary (`grok`) with a different config home; a fork doc that borrows the upstream one-liner installs the wrong product and silently breaks the dual-install separation (`~/.grok` vs `~/.agent-tui`). It is also the distribution mirror of the rename-boundary rule: wire contracts stay upstream-named, but distribution identity is fully forked — upstream's private pipeline is not under the fork's control and could change or gate at any time.

**How to apply:** when adding or editing any install instruction, updater constant, or package metadata, check the URL against this rule before committing. When vendoring upstream doc text into the fork (getting-started guides, READMEs), rewrite its install sections to the fork channels rather than copying them through. If a legitimate need arises to mention the official channel (e.g. the dual-install guide), label it explicitly as installing upstream Grok Build, as the npm README does.