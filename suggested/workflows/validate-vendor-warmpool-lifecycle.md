# Workflow: validate vendor runtime warm-pool lifecycle before rung 3 ships

When promoting a detect-only vendor to fully routable (rung 3), the runtime crate must implement three lifecycle obligations: idle timeout, health probe before reuse, and optional eager warm. This workflow verifies each behavior empirically on a running TUI before the vendor ships to inference.

## 1. Verify idle timeout

1. Select the vendor runtime (`/runtime <vendor>`).
2. Send a turn to establish a warm connection.
3. Record the process ID or socket handle of the active connection.
4. Wait beyond the timeout threshold (~5–15 min typical).
5. Observe: does the connection close or remain idle? Verify via process monitoring (lsof, strace, or the runtime's own metrics if exposed).
6. Send another turn: does it establish a fresh connection, or reuse the stale one?

**Pass:** connection closes after idle timeout; next turn establishes fresh.
**Fail:** connection held indefinitely, or fresh connections spawn on every turn (defeating warm-pool amortization).

## 2. Verify health probe and respawn

1. With the vendor selected, send a turn to warm the pool.
2. Force-kill the warm connection's process (or socket).
3. Immediately send another turn.
4. Observe: does the pool detect the dead connection, spawn a fresh one, and continue to completion?
5. Verify in logs/metrics that a health-probe call was made before the respawn.

**Pass:** health probe detects death, respawn completes the turn.
**Fail:** turn stalls, or a second respawn is needed mid-turn.

## 3. Verify optional eager warm

1. Restart the TUI with the vendor CLI logged in and detected.
2. Check whether the warm pool is established before the user sends the first turn (eager warm on startup).
3. If enabled: measure latency of the first turn (should be low, warm-pool cost already paid).
4. If disabled: eager warm is optional; move to step 4.

**Pass:** eager warm (if wired) happens at startup; first turn is fast.
**Fail:** first turn is slow or the pool is not initialized on start despite being enabled.

## 4. Record the results

Document each lifecycle behavior and its status (pass/fail/not-applicable). If any step fails, the runtime is incomplete — it should not advance to rung 3 until all three pass.
