# runtime.toml reference — every key, its writer, and when it applies

`~/.agent-tui/runtime.toml` is the single file that owns Agent TUI's cross-vendor selection state. Both the vendor choice and every per-vendor model pick live here — by convention no second persistence location may be invented for either. This page is the file-level reference; the rules it reflects live in the owning convention docs.

## Location

Under the config home: `~/.agent-tui`, resolved via `$AGENT_TUI_HOME` with legacy `$GROK_HOME` still accepted as a fallback. Any tooling that reads this file must resolve the home through that chain, never a hardcoded path.

## Keys

| Key | Meaning | Written by | Applies |
|---|---|---|---|
| runtime selection | Which vendor serves turns (Grok / Codex / Claude) | `/runtime <vendor>` (aliases `/provider`, `/rt`) | Persists across restarts |
| `codex_model` | Model pick within the Codex catalog (loaded live via `model/list` over the warm app-server) | `/model` while the Codex runtime is active | **Next thread**, never mid-thread |
| `claude_model` | Model pick within the Claude catalog (loaded live via the `claude -p --output-format json` harness) | `/model` while the Claude runtime is active | **Next thread**, never mid-thread |

Pattern for future vendors: one `<vendor>_model` key per catalog-shipped vendor, co-located with the runtime selection. Grok's model pick predates this file's vendor-scoped scheme and rides the built-in sampler catalog.

## Semantics worth knowing

- **Vendor-scoped, independent picks.** Each vendor's model choice is its own key — switching runtimes does not disturb another vendor's pick, and model IDs are per-vendor wire strings, never entries in one shared catalog.
- **Thread-boundary application.** A thread's turns ride one warm runtime connection with its own handshake and tool index; the pick applies when the next thread starts. Hot-swapping mid-thread is forbidden by convention.
- **Selection ≠ routing.** For a vendor below the inference rung of the readiness ladder, the runtime selection persists here even though turns don't route to it — the file records the user's choice, not the vendor's capability.

## Scripting and reset

To script or reset any selection, edit this file directly rather than hunting for another config surface — it is the documented reset path ("edit that file rather than hunting for another config surface"). It is user-editable state, not machine-written evidence, so hand edits are legitimate; keep edits to the delta you intend.

## Troubleshooting

- Selection not sticking across restarts → confirm the TUI is resolving the same config home you are editing (check `$AGENT_TUI_HOME` / legacy `$GROK_HOME`).
- A model pick "not taking effect" immediately after `/model` → expected; it applies on the next thread.
- A `<vendor>_model` key present but `/model` shows no catalog → the vendor's harness isn't answering the live catalog fetch; probe the vendor's readiness rung rather than editing the file.