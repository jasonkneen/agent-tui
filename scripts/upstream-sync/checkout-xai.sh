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

# Paths the monorepo sync bot owns (upstream naming).
paths=(
  SOURCE_REV
  rust-toolchain.toml
  clippy.toml
  crates/codegen/ptyctl
  crates/codegen/ptyctl-cli
  crates/build/xai-proto-build
  crates/common/xai-circuit-breaker
  crates/common/xai-computer-hub-core
  crates/common/xai-computer-hub-mcp-adapter
  crates/common/xai-computer-hub-sdk
  crates/common/xai-grok-compaction
  crates/common/xai-interjection-core
  crates/common/xai-test-utils
  crates/common/xai-tool-protocol
  crates/common/xai-tool-runtime
  crates/common/xai-tool-types
  crates/common/xai-tracing
  prod/mc/cli-chat-proxy-types
)

# All xai-* codegen crates present on REF
while IFS= read -r name; do
  [[ "$name" == xai* ]] || continue
  paths+=("crates/codegen/$name")
done < <(git ls-tree --name-only "$REF:crates/codegen")

git checkout "$REF" -- "${paths[@]}"

echo "SOURCE_REV → $(tr -d '[:space:]' < SOURCE_REV)"
echo "rust-toolchain → $(grep '^channel' rust-toolchain.toml || true)"
echo "Staged/unstaged xai paths ready. Next: scripts/upstream-sync/port-to-agent-tui.py"
