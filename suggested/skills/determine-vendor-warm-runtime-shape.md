---
name: determine-vendor-warm-runtime-shape
description: Classify a vendor's warm-runtime shape — held socket vs sticky subprocess session — from the vendor's own harness surface before wiring the catalog or inference rung, so every stage of the integration reuses the correct connection shape and no third shape is invented.
---

# Determine a new vendor's warm-runtime shape before wiring any rung

Every external vendor in Agent TUI follows the warm-connection principle, but the principle has exactly **two concrete shapes** (`suggested/wiki/warm-runtime-shapes-socket-vs-subprocess.md`): the **held socket** (Codex — `codex app-server`, JSON-RPC over stdio/unix socket/ws, turn events as streamed notifications) and the **sticky subprocess session** (Claude — `claude -p --output-format json` with a sticky `--resume` id). The shape decision comes *first*, because both the catalog rung and the inference rung must ride it: the ship-catalog-rung workflow's step 1 says to "pick the catalog transport from the vendor's runtime shape" and "Do not invent a third shape — reuse whatever connection the eventual inference bridge will use."

## 1. Read the vendor's own harness surface

Per `docs/LOCAL_CLI_AUTH.md`, the goal is to "use vendors the way their products do." The shape is dictated by what the vendor's own local harness offers — never by what is convenient to build:

- The vendor ships a **long-lived local server or daemon** (an app-server, a JSON-RPC endpoint over stdio/socket/ws, streamed notifications) → **held socket**. Codex is the exemplar.
- The vendor's harness is a **CLI invocation with a session-continuation mechanism** (a machine-parseable output mode plus a resume/continue flag, like `claude -p --output-format json` + `--resume`) → **sticky subprocess session**. Claude is the exemplar.

If the harness offers both, prefer the long-lived server — it is the stronger warm asset. If it offers neither, there is no sanctioned integration path yet: do not fabricate one from detected credentials (credential detection is discovery-only).

## 2. Record what the shape commits you to

- **What "warm" means:** socket → one long-lived connection per vendor runtime; subprocess → one continuing session carried by the sticky resume id.
- **Lifecycle realization:** the idle-timeout / health-probe / eager-warm obligations apply in the shape's own vocabulary (close vs let-lapse; ping vs resume-id validation; spawn-on-start vs early session).
- **Catalog transport:** socket-shaped vendors serve the catalog over a listing call on the warm connection (Codex: `model/list`); subprocess-shaped vendors serve it through the CLI harness. The catalog never gets its own transport.

## 3. Sanity checks before wiring

- The chosen shape is the one the **eventual inference bridge** will use — if you cannot name that bridge, the shape decision is premature.
- No one-shot HTTP path appears anywhere in the design: neither for catalog nor for inference, and never built from credentials the detector found.
- The shape and its exemplar are named in the design spec / integration doc, so reviewers can check lifecycle behaviors against the right column of the two-shapes table.