#!/usr/bin/env bash
# link-product-bins.sh — zero-duplication product skins.
#
# Builds ONE core binary (agent-tui) and creates symlinks for each product name.
# argv[0] selects the product profile (see agent_tui_bin::product_preset_for_argv0).
#
# Usage:
#   cargo build -p agent-tui-bin
#   ./scripts/link-product-bins.sh              # links in target/debug
#   ./scripts/link-product-bins.sh release      # links in target/release
#   DEST=/usr/local/bin ./scripts/link-product-bins.sh release

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE="${1:-debug}"
if [[ "$PROFILE" == "release" ]]; then
  DIR="${DEST:-$ROOT/target/release}"
else
  DIR="${DEST:-$ROOT/target/debug}"
fi

CORE="$DIR/agent-tui"
if [[ ! -x "$CORE" ]]; then
  echo "missing $CORE — build first: cargo build -p agent-tui-bin${PROFILE:+ --release}" >&2
  exit 1
fi

# Product skins → same inode as core
NAMES=(
  agent-multi
  grok
  lazartui
  codex
  claude
  hermes
)

for name in "${NAMES[@]}"; do
  link="$DIR/$name"
  # Replace file or wrong symlink
  rm -f "$link"
  ln -s "agent-tui" "$link"
  echo "linked $link → agent-tui"
done

echo "core: $CORE ($(wc -c <"$CORE" | tr -d ' ') bytes)"
echo "products: ${NAMES[*]} (symlinks — zero core duplication)"
