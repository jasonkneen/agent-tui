#!/usr/bin/env bash
# Bring xai-* / shared monorepo surfaces to upstream tip (does not touch agent-tui-*).
# Usage: scripts/upstream-sync/checkout-xai.sh [upstream/main]
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

ensure_upstream
REF="${1:-$UPSTREAM_REF}"

echo "Checking out monorepo surfaces from $REF …"
echo "(delete-then-checkout so stale files from older layouts cannot survive)"

checkout_clean() {
  local path="$1"
  # Remove working tree path first so untracked leftovers (dual modules) die.
  if [[ -e "$path" || -L "$path" ]]; then
    rm -rf "$path"
  fi
  git checkout "$REF" -- "$path"
}

checkout_clean SOURCE_REV
checkout_clean rust-toolchain.toml
checkout_clean clippy.toml
checkout_clean crates/codegen/ptyctl
checkout_clean crates/codegen/ptyctl-cli
checkout_clean prod/mc/cli-chat-proxy-types

for area in crates/codegen crates/common crates/build; do
  while IFS= read -r name; do
    [[ "$name" == xai* ]] || continue
    checkout_clean "$area/$name"
  done < <(git ls-tree --name-only "$REF:$area" 2>/dev/null || true)
done

# Clippy reasons should name agent_tui_* helpers in this fork.
if [[ -f clippy.toml ]]; then
  sed -i.bak \
    -e 's/xai_grok_tools/agent_tui_tools/g' \
    -e 's/xai_tty_utils/agent_tui_tty_utils/g' \
    clippy.toml && rm -f clippy.toml.bak
fi

echo "SOURCE_REV → $(tr -d '[:space:]' < SOURCE_REV)"
echo "rust-toolchain → $(grep '^channel' rust-toolchain.toml || true)"
echo "xai paths ready. Next: scripts/upstream-sync/port-to-agent-tui.py --bulk"
