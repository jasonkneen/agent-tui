# Contributing

## Upstream (SpaceXAI / xAI)

The original Grok Build tree does **not** accept external pull requests or
unsolicited patches. Upstream is published for source transparency under the
Apache License 2.0 (see [`LICENSE`](LICENSE)).

Security for **upstream** Grok Build: see [`SECURITY.md`](SECURITY.md).

## This fork (Agent TUI)

This repository is a packaging fork maintained at
[jasonkneen/agent-tui](https://github.com/jasonkneen/agent-tui).

| Topic | Doc |
|-------|-----|
| What changed vs upstream | [FORK.md](FORK.md) |
| Build & user install | [README.md](README.md) |
| **How to cut a release** | [RELEASING.md](RELEASING.md) |
| Agent / automation notes | [AGENTS.md](AGENTS.md) |

### Working on the fork

```sh
cargo check -p agent-tui-bin
cargo test -p agent-tui-update --lib
```

CI: [`.github/workflows/ci.yml`](.github/workflows/ci.yml)  
Release: [`.github/workflows/release.yml`](.github/workflows/release.yml) (tag `v*`)

### Shipping a version

1. Bump versions (see RELEASING.md checklist).
2. `git tag vX.Y.Z && git push origin vX.Y.Z`
3. Wait for the Release workflow; smoke-test `install.sh`.

Do not reintroduce `x.ai/cli` or `@xai-official/grok` as this fork's default
distribution — those remain official Grok Build channels.

## Licensing

By downloading or using this source, you agree that your use is governed by
the Apache License, Version 2.0.
