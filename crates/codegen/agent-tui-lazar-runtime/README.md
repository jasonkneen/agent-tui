# agent-tui-lazar-runtime

Spawn-per-turn client for the **lazar** agent kernel.

```text
lazar -p <prompt> --output-format stream-json --model <id> --session <id>
```

The kernel owns providers, models, auth, tools, skills, hooks, and session
logs. This crate only:

1. Spawns `lazar` (cwd = `$LAZAR_HOME`, default `~/lazar`)
2. Parses JSONL events on stdout
3. Returns concatenated `text_delta` text (+ sticky `--session` id)

It never reimplements OAuth, provider routing, or the Go `lazartui` chrome.

## Use from Agent TUI

```
/runtime lazar
```

Requires `lazar` on `PATH` or `$LAZAR_HOME/bin/lazar`, and operator env from:

```sh
source ~/lazar/workspace/lazar-env.sh
```

Design, gaps vs Go TUI, and launchd heartbeats:
[docs/LOCAL_CLI_AUTH.md](../../../docs/LOCAL_CLI_AUTH.md) (Lazar section).

## API sketch

```rust
use agent_tui_lazar_runtime::{LazarRuntimePool, PoolConfig};

let pool = LazarRuntimePool::new(PoolConfig {
    lazar_bin: "lazar".into(),
    cwd: Some(agent_tui_lazar_runtime::lazar_home()),
    ..Default::default()
});
pool.set_session("my-session").await; // optional multi-turn pin
let res = pool.start_text_turn("hello", None).await?;
```

## Tests / parity

```sh
# unit
cargo test -p agent-tui-lazar-runtime

# live integration (needs keys + lazar)
source ~/lazar/workspace/lazar-env.sh
LAZAR_INTEGRATION=1 cargo test -p agent-tui-lazar-runtime -- --ignored

# CLI spawn (Go-TUI-equivalent) vs pool
export LAZAR_NO_SANDBOX=1
bash crates/codegen/agent-tui-lazar-runtime/scripts/parity-eval.sh
```

## Where the old Go TUI is

Not in this repository. Source and binary:

| Path | Role |
|------|------|
| `~/lazar/workspace/tui/` | Go source |
| `~/lazar/workspace/lazartui` | Built binary |
| `~/lazar/workspace/lazartui.sh` | Launcher |
| `/usr/local/bin/lazartui` | Symlink → launcher |
