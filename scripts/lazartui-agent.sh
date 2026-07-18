#!/usr/bin/env bash
# lazartui-agent.sh — run the Lazar product binary (same core as agent-tui).
#
# Prefers the `lazartui` binary (brand + locked lazar addon baked in). Falls
# back to `AGENT_TUI_PRODUCT=lazar agent-tui` if only the platform bin exists.
#
# Usage:
#   ./scripts/lazartui-agent.sh
#   cargo build -p agent-tui-bin --bin lazartui && ./scripts/lazartui-agent.sh

set -euo pipefail

LAZAR_HOME="${LAZAR_HOME:-$HOME/lazar}"
if [[ -f "$LAZAR_HOME/workspace/lazar-env.sh" ]]; then
  # shellcheck disable=SC1091
  source "$LAZAR_HOME/workspace/lazar-env.sh"
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Prefer symlink skin if present (same inode as agent-tui after link-product-bins).
BIN="${AGENT_TUI_BIN:-}"
if [[ -z "$BIN" ]]; then
  for candidate in \
    "$ROOT/target/debug/lazartui" \
    "$ROOT/target/release/lazartui" \
    "$ROOT/target/debug/agent-tui" \
    "$ROOT/target/release/agent-tui"
  do
    if [[ -e "$candidate" ]]; then
      BIN="$candidate"
      break
    fi
  done
fi
if [[ -z "${BIN:-}" ]]; then
  if command -v lazartui >/dev/null 2>&1; then
    BIN="$(command -v lazartui)"
  elif command -v agent-tui >/dev/null 2>&1; then
    BIN="$(command -v agent-tui)"
  else
    echo "agent-tui not found. Build + link:" >&2
    echo "  cargo build -p agent-tui-bin && ./scripts/link-product-bins.sh" >&2
    exit 1
  fi
fi

# If invoking the core name (not a lazartui symlink), force product.
if [[ "$(basename "$BIN")" == "agent-tui" ]]; then
  export AGENT_TUI_PRODUCT="${AGENT_TUI_PRODUCT:-lazar}"
fi

exec "$BIN" "$@"
