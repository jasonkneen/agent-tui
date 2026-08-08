#!/usr/bin/env bash
# Shared helpers for Agent TUI ← upstream monorepo sync.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

UPSTREAM_REMOTE="${UPSTREAM_REMOTE:-upstream}"
UPSTREAM_REF="${UPSTREAM_REF:-${UPSTREAM_REMOTE}/main}"
FORK_ONLY_CRATES=(
  agent-tui-bin
  agent-tui-claude-runtime
  agent-tui-codex-runtime
  agent-tui-lazar-runtime
  agent-tui-hermes-runtime
)

# Wire contracts that must NEVER be rewritten during rename ports.
export UPSTREAM_SYNC_PROTECT_PATTERNS=(
  'xai-grok-cli'
  'X-XAI-Token-Auth'
  'xai.api_key'
  'XAI_API_KEY'
)

die() { echo "error: $*" >&2; exit 1; }

require_clean_or_confirm() {
  if ! git diff --quiet || ! git diff --cached --quiet; then
    if [[ "${FORCE:-}" == "1" ]]; then
      echo "warning: dirty tree (FORCE=1); continuing"
      return 0
    fi
    die "working tree is dirty. Commit/stash first, or FORCE=1 to proceed."
  fi
}

ensure_upstream() {
  git remote get-url "$UPSTREAM_REMOTE" >/dev/null 2>&1 \
    || die "remote '$UPSTREAM_REMOTE' missing (expected xai-org/grok-build)"
  git fetch "$UPSTREAM_REMOTE" main --quiet
}

our_source_rev() {
  if [[ -f SOURCE_REV ]]; then
    tr -d '[:space:]' < SOURCE_REV
  else
    die "SOURCE_REV missing"
  fi
}

# Find the upstream commit whose SOURCE_REV matches $1 (or our current).
upstream_commit_for_source_rev() {
  local want="${1:-$(our_source_rev)}"
  local c rev
  for c in $(git rev-list --reverse "$UPSTREAM_REF"); do
    rev=$(git show "$c:SOURCE_REV" 2>/dev/null | tr -d '[:space:]' || true)
    if [[ "$rev" == "$want" ]]; then
      echo "$c"
      return 0
    fi
  done
  return 1
}

pending_upstream_commits() {
  local base
  base=$(upstream_commit_for_source_rev) || die "no upstream commit matches SOURCE_REV=$(our_source_rev)"
  git rev-list --reverse "${base}..${UPSTREAM_REF}"
}
