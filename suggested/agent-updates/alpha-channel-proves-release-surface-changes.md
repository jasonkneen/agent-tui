# Convention: release-surface changes are proven on the alpha channel before stable

**Rule.** Any change to the release surface — install scripts, asset name pattern, tag shape, repo id, download/API bases, updater constants in `version.rs` — is first cut as an alpha tag (`vX.Y.Z-alpha.N`) and smoke-tested end-to-end (`install.sh` resolves, downloads, verifies `.sha256`, and the installed binary runs) before any stable `vX.Y.Z` tag is pushed. Stable users must never be the first consumers of a changed release surface.

**Grounding.**
- `suggested/workflows/change-agent-tui-release-identity.md`, Verify: "Cut an alpha tag and smoke-test `install.sh` end-to-end before touching stable."
- `RELEASING.md` channel table: alpha tags (`v0.1.220-alpha.1`) carry `prerelease = true`; stable/enterprise consumers resolve GitHub `releases/latest`.
- `suggested/wiki/agent-tui-release-channels-and-assets.md`: "a prerelease can never leak to them because GitHub excludes prereleases from `latest`" — the isolation property this convention relies on — and `AGENT_TUI_CHANNEL=alpha` on `install.sh` is the documented way to consume the test release.

**Why:** release-surface bugs are invisible to CI. The Release workflow can succeed — build green, assets uploaded — while the assets carry names the installers can't parse, or the updater's `version.rs` constants point at a dead location. The only test that exercises the real contract is an actual release resolved by an actual installer. The channel design provides a free staging environment: an alpha tag runs the identical `.github/workflows/release.yml` pipeline and is served to `AGENT_TUI_CHANNEL=alpha` consumers, while `releases/latest` continues serving the last-known-good stable to everyone else.

**How to apply:** when a change touches anything in the change-release-identity workflow's surface list, plan the rollout as alpha-first: bump to `vX.Y.Z-alpha.1`, tag, wait for the Release workflow, then run `AGENT_TUI_CHANNEL=alpha curl -fsSL …/install.sh | bash` on at least one supported platform and confirm the binary launches and self-reports the alpha version. Only after that passes, cut the stable tag. If the alpha smoke-test fails, fix and cut `-alpha.2` — never "fix forward" directly on a stable tag.
