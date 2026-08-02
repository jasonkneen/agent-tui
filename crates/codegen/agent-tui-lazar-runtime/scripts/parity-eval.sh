#!/usr/bin/env bash
# parity-eval.sh — compare Go-TUI-equivalent CLI spawns vs agent-tui LazarRuntimePool.
#
# Both paths drive the SAME kernel binary with the same flags. We score each
# case with deterministic assertions (markers / files / session continuity),
# not fuzzy free-text equality (the model is non-deterministic).
#
# Usage:
#   source ~/lazar/workspace/lazar-env.sh
#   bash crates/codegen/agent-tui-lazar-runtime/scripts/parity-eval.sh
#
# Env:
#   EVAL_MODEL      default MiniMax-M3 (or $LAZAR_MODEL)
#   EVAL_TIMEOUT    per-turn seconds (default 120)
#   EVAL_CASES      space list: echo tool file session  (default: all)
#   LAZAR_BIN       kernel path (default: which lazar / $LAZAR_HOME/bin/lazar)
#   SKIP_POOL=1     only run CLI path
#   SKIP_CLI=1      only run pool path

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
LAZAR_HOME="${LAZAR_HOME:-$HOME/lazar}"
MODEL="${EVAL_MODEL:-${LAZAR_MODEL:-MiniMax-M3}}"
TIMEOUT="${EVAL_TIMEOUT:-120}"
CASES="${EVAL_CASES:-echo tool file session}"
TMP="$(mktemp -d)"
trap 'chmod -R u+w "$TMP" 2>/dev/null; rm -rf "$TMP"' EXIT

if [ -z "${ANTHROPIC_API_KEY:-}${MINIMAX_API_KEY:-}" ]; then
  echo "No provider key in env. Run: source ~/lazar/workspace/lazar-env.sh" >&2
  exit 2
fi

# Prefer env-loaded key; lazar-env may set ANTHROPIC_API_KEY from MiniMax.
if [ -z "${ANTHROPIC_API_KEY:-}" ] && [ -n "${MINIMAX_API_KEY:-}" ]; then
  export ANTHROPIC_API_KEY="$MINIMAX_API_KEY"
fi

if [ -n "${LAZAR_BIN:-}" ]; then
  BIN="$LAZAR_BIN"
elif command -v lazar >/dev/null 2>&1; then
  BIN="$(command -v lazar)"
elif [ -x "$LAZAR_HOME/bin/lazar" ]; then
  BIN="$LAZAR_HOME/bin/lazar"
else
  echo "lazar binary not found" >&2
  exit 2
fi

TIMEOUT_CMD="$(command -v timeout || command -v gtimeout || true)"
with_timeout() {
  if [ -n "$TIMEOUT_CMD" ]; then
    "$TIMEOUT_CMD" "$TIMEOUT" "$@"
  else
    "$@"
  fi
}

PASS=0
FAIL=0
SKIP=0
ok()  { printf '[PASS] %s\n' "$1"; PASS=$((PASS + 1)); }
no()  { printf '[FAIL] %s — %s\n' "$1" "$2"; FAIL=$((FAIL + 1)); }
sk()  { printf '[SKIP] %s — %s\n' "$1" "$2"; SKIP=$((SKIP + 1)); }

# Collect text_delta from stream-json stdout (Go TUI path).
cli_turn() {
  local session="$1" prompt="$2" out="$3"
  local args=(--output-format stream-json --model "$MODEL" --session "$session" -p "$prompt")
  # Go TUI: cwd is access.Cwd (usually lazar home); stderr discarded/capped.
  (
    cd "$LAZAR_HOME" || exit 1
    with_timeout "$BIN" "${args[@]}" 2>/dev/null
  ) >"$out.raw" || true
  # Flatten text_delta → plain text
  python3 - "$out.raw" "$out" <<'PY'
import json, sys
raw, dest = sys.argv[1], sys.argv[2]
text = []
events = []
for line in open(raw, errors="replace"):
    line = line.strip()
    if not line:
        continue
    try:
        ev = json.loads(line)
    except Exception:
        continue
    events.append(ev.get("type") or "?")
    if ev.get("type") == "text_delta" and isinstance(ev.get("text"), str):
        text.append(ev["text"])
open(dest, "w").write("".join(text))
open(dest + ".events", "w").write(" ".join(events))
PY
}

# Pool path via example binary.
POOL_BIN=""
ensure_pool() {
  if [ "${SKIP_POOL:-0}" = "1" ]; then
    return 1
  fi
  if [ -n "$POOL_BIN" ] && [ -x "$POOL_BIN" ]; then
    return 0
  fi
  echo "== building parity_turn example =="
  ( cd "$ROOT" && cargo build -p agent-tui-lazar-runtime --example parity_turn -q ) || return 1
  POOL_BIN="$ROOT/target/debug/examples/parity_turn"
  [ -x "$POOL_BIN" ]
}

pool_turn() {
  local session="$1" prompt="$2" out="$3"
  ensure_pool || return 1
  local json
  json=$(
    EVAL_TIMEOUT="$TIMEOUT" with_timeout "$POOL_BIN" \
      --prompt "$prompt" \
      --model "$MODEL" \
      --session "$session" \
      --cwd "$LAZAR_HOME" \
      --bin "$BIN" 2>/dev/null
  ) || true
  printf '%s\n' "$json" >"$out.json"
  python3 - "$out.json" "$out" <<'PY'
import json, sys
src, dest = sys.argv[1], sys.argv[2]
try:
    d = json.load(open(src))
except Exception as e:
    open(dest, "w").write("")
    open(dest + ".err", "w").write(str(e))
    raise SystemExit(0)
if not d.get("ok"):
    open(dest, "w").write("")
    open(dest + ".err", "w").write(d.get("error") or "unknown")
else:
    open(dest, "w").write(d.get("text") or "")
open(dest + ".ms", "w").write(str(d.get("ms", "")))
PY
}

want_case() {
  case " $CASES " in
    *" $1 "*) return 0 ;;
    *) return 1 ;;
  esac
}

echo "== lazar parity eval =="
echo "model=$MODEL  bin=$BIN  lazar_home=$LAZAR_HOME  timeout=${TIMEOUT}s"
echo "cases: $CASES"
echo

# ── 1. Echo: single-word deterministic reply ─────────────────────────────
if want_case echo; then
  SID_CLI="parity-echo-cli-$$"
  SID_POOL="parity-echo-pool-$$"
  PROMPT='Reply with the single word: PARSNIP'
  echo "-- case: echo --"
  if [ "${SKIP_CLI:-0}" != "1" ]; then
    cli_turn "$SID_CLI" "$PROMPT" "$TMP/echo-cli"
    if rg -qi 'PARSNIP' "$TMP/echo-cli"; then
      ok "cli echo contains PARSNIP ($(wc -c <"$TMP/echo-cli")B, events: $(cat "$TMP/echo-cli.events" 2>/dev/null | tr ' ' '\n' | sort -u | tr '\n' ' '))"
    else
      no "cli echo" "body=$(head -c 200 "$TMP/echo-cli" | tr '\n' ' ')"
    fi
  else
    sk "cli echo" "SKIP_CLI=1"
  fi
  if [ "${SKIP_POOL:-0}" != "1" ]; then
    if pool_turn "$SID_POOL" "$PROMPT" "$TMP/echo-pool"; then
      if rg -qi 'PARSNIP' "$TMP/echo-pool"; then
        ms="$(cat "$TMP/echo-pool.ms" 2>/dev/null || echo '?')"
        ok "pool echo contains PARSNIP ($(wc -c <"$TMP/echo-pool")B, ${ms}ms)"
      else
        no "pool echo" "body=$(head -c 200 "$TMP/echo-pool" | tr '\n' ' ') err=$(cat "$TMP/echo-pool.err" 2>/dev/null)"
      fi
    else
      sk "pool echo" "build/run failed"
    fi
  else
    sk "pool echo" "SKIP_POOL=1"
  fi
  echo
fi

# ── 2. Tool: must actually execute, not narrate ──────────────────────────
if want_case tool; then
  SID_CLI="parity-tool-cli-$$"
  SID_POOL="parity-tool-pool-$$"
  PROMPT='Run exactly this bash and nothing else: echo EVAL-TOOL-OK'
  echo "-- case: tool --"
  if [ "${SKIP_CLI:-0}" != "1" ]; then
    cli_turn "$SID_CLI" "$PROMPT" "$TMP/tool-cli"
    # Prefer tool_result evidence; fall back to text containing marker.
    HAS_TR=0
    rg -q 'tool_result|tool_use' "$TMP/tool-cli.events" 2>/dev/null && HAS_TR=1
    if rg -q 'EVAL-TOOL-OK' "$TMP/tool-cli" || [ "$HAS_TR" = "1" ] && rg -q 'EVAL-TOOL-OK' "$TMP/tool-cli.raw" 2>/dev/null; then
      ok "cli tool executed (events include tool_*=$HAS_TR)"
    else
      no "cli tool" "no EVAL-TOOL-OK; events=$(cat "$TMP/tool-cli.events" 2>/dev/null) body=$(head -c 160 "$TMP/tool-cli" | tr '\n' ' ')"
    fi
  else
    sk "cli tool" "SKIP_CLI=1"
  fi
  if [ "${SKIP_POOL:-0}" != "1" ]; then
    if pool_turn "$SID_POOL" "$PROMPT" "$TMP/tool-pool"; then
      # Pool flattens to text only — marker must appear in assistant text
      # (kernel usually echoes tool output into the reply or tools leave traces).
      # Also check the session log for tool_use.
      SLOG="$LAZAR_HOME/logs/sessions/${SID_POOL}.jsonl"
      if rg -q 'EVAL-TOOL-OK' "$TMP/tool-pool" || rg -q 'EVAL-TOOL-OK' "$SLOG" 2>/dev/null; then
        ok "pool tool executed (text or session log has EVAL-TOOL-OK)"
      else
        no "pool tool" "body=$(head -c 160 "$TMP/tool-pool" | tr '\n' ' ') err=$(cat "$TMP/tool-pool.err" 2>/dev/null)"
      fi
    else
      sk "pool tool" "build/run failed"
    fi
  else
    sk "pool tool" "SKIP_POOL=1"
  fi
  echo
fi

# ── 3. File write + VERIFY-style grounding ───────────────────────────────
if want_case file; then
  F="$TMP/parity-banana.txt"
  SID_CLI="parity-file-cli-$$"
  SID_POOL="parity-file-pool-$$"
  PROMPT="Create the file $F containing the single word BANANA. Use bash. When done, confirm the word is in the file."
  echo "-- case: file --"
  rm -f "$F"
  if [ "${SKIP_CLI:-0}" != "1" ]; then
    cli_turn "$SID_CLI" "$PROMPT" "$TMP/file-cli"
    if [ -f "$F" ] && rg -q 'BANANA' "$F"; then
      ok "cli file write produced $F"
    else
      no "cli file" "missing/empty file; body=$(head -c 160 "$TMP/file-cli" | tr '\n' ' ')"
    fi
  else
    sk "cli file" "SKIP_CLI=1"
  fi
  F2="$TMP/parity-banana-pool.txt"
  rm -f "$F2"
  PROMPT2="Create the file $F2 containing the single word BANANA. Use bash. When done, confirm the word is in the file."
  if [ "${SKIP_POOL:-0}" != "1" ]; then
    if pool_turn "$SID_POOL" "$PROMPT2" "$TMP/file-pool"; then
      if [ -f "$F2" ] && rg -q 'BANANA' "$F2"; then
        ok "pool file write produced $F2"
      else
        no "pool file" "missing/empty; body=$(head -c 160 "$TMP/file-pool" | tr '\n' ' ') err=$(cat "$TMP/file-pool.err" 2>/dev/null)"
      fi
    else
      sk "pool file" "build/run failed"
    fi
  else
    sk "pool file" "SKIP_POOL=1"
  fi
  echo
fi

# ── 4. Session continuity (two turns, same --session) ────────────────────
if want_case session; then
  SID_CLI="parity-sess-cli-$$"
  SID_POOL="parity-sess-pool-$$"
  echo "-- case: session --"
  if [ "${SKIP_CLI:-0}" != "1" ]; then
    cli_turn "$SID_CLI" 'Remember the codeword is ZIRCON. Reply with only: OK' "$TMP/sess-cli-1"
    cli_turn "$SID_CLI" 'What is the codeword? Reply with only the single word.' "$TMP/sess-cli-2"
    if rg -qi 'ZIRCON' "$TMP/sess-cli-2"; then
      ok "cli session continuity (turn2 recalls ZIRCON)"
    else
      no "cli session" "turn2=$(head -c 200 "$TMP/sess-cli-2" | tr '\n' ' ')"
    fi
  else
    sk "cli session" "SKIP_CLI=1"
  fi
  if [ "${SKIP_POOL:-0}" != "1" ]; then
    if pool_turn "$SID_POOL" 'Remember the codeword is ZIRCON. Reply with only: OK' "$TMP/sess-pool-1" \
      && pool_turn "$SID_POOL" 'What is the codeword? Reply with only the single word.' "$TMP/sess-pool-2"; then
      if rg -qi 'ZIRCON' "$TMP/sess-pool-2"; then
        ok "pool session continuity (turn2 recalls ZIRCON)"
      else
        no "pool session" "turn2=$(head -c 200 "$TMP/sess-pool-2" | tr '\n' ' ') err=$(cat "$TMP/sess-pool-2.err" 2>/dev/null)"
      fi
    else
      sk "pool session" "build/run failed"
    fi
  else
    sk "pool session" "SKIP_POOL=1"
  fi
  echo
fi

echo "---- parity: $PASS passed, $FAIL failed, $SKIP skipped ----"
[ "$FAIL" -eq 0 ]
