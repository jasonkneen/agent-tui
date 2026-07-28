# Verify vendor model selection persists across vendor runtime switches

The vendor-model-selection-runtime-toml-keys convention requires each vendor's pick to persist in its own `<vendor>_model` key and remain independent when switching runtimes. This skill validates that behavior empirically.

## Setup

- Two vendors with catalog support (e.g., Codex and Claude, both logged in).
- Current build launched by path from build output.
- Text editor for reading `~/.agent-tui/runtime.toml`.

## The test sequence

1. **Switch to vendor A and pick a model.**
   ```
   /runtime codex
   /model
   # Select a non-default model (note which one)
   ```

2. **Verify the pick in runtime.toml.**
   ```
   # Read ~/.agent-tui/runtime.toml
   # Confirm: codex_model key exists and holds the model you selected
   ```

3. **Switch to vendor B and pick a different model.**
   ```
   /runtime claude
   /model
   # Select a different model
   ```

4. **Verify vendor B's independent key.**
   ```
   # Read ~/.agent-tui/runtime.toml again
   # Confirm: claude_model key exists with the new selection
   # Confirm: codex_model key still holds the earlier pick (unchanged)
   ```

5. **Switch back to vendor A.**
   ```
   /runtime codex
   ```

6. **Verify vendor A's pick is restored.**
   ```
   /model
   # Confirm: the picker opens showing the Codex catalog, and the previously-selected model is highlighted/retained
   # (Verify by reading runtime.toml: codex_model unchanged from step 2)
   ```

## Passing criteria

- Each vendor's model key persists independently in `runtime.toml`.
- Switching vendors does not disturb another vendor's stored pick.
- Switching back to a vendor restores its last selection.
- Edits to `runtime.toml` take effect on the next thread, not mid-thread (verify by sending a turn, editing the key, and confirming the pick applies only after restart).
