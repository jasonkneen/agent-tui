#!/usr/bin/env bash
# Full upstream → Agent TUI port pipeline (xai trees + agent-tui rename + fork remmerge).
#
# Usage:
#   scripts/upstream-sync/status.sh          # review first
#   scripts/upstream-sync/apply.sh           # dry-run safety: requires clean tree
#   scripts/upstream-sync/apply.sh --commit  # also create a commit
#
# Env:
#   FORCE=1              allow dirty tree
#   UPSTREAM_REMOTE=…    default upstream
#   SKIP_CHECK=1         skip cargo check -p agent-tui-bin
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

DO_COMMIT=0
for arg in "$@"; do
  case "$arg" in
    --commit) DO_COMMIT=1 ;;
    -h|--help)
      sed -n '2,12p' "$0"
      exit 0
      ;;
  esac
done

require_clean_or_confirm
ensure_upstream

OUR_BEFORE=$(our_source_rev)
BASE_COMMIT=$(upstream_commit_for_source_rev) \
  || die "no upstream commit for SOURCE_REV=$OUR_BEFORE"
export UPSTREAM_SYNC_BASE_REV="$BASE_COMMIT"

TIP_REV=$(git show "$UPSTREAM_REF:SOURCE_REV" | tr -d '[:space:]')
if [[ "$OUR_BEFORE" == "$TIP_REV" ]]; then
  echo "Already at upstream SOURCE_REV=$TIP_REV — nothing to do."
  exit 0
fi

echo "=== Upstream sync plan ==="
echo "  base  SOURCE_REV=$OUR_BEFORE  @ $(git rev-parse --short "$BASE_COMMIT")"
echo "  tip   SOURCE_REV=$TIP_REV     @ $(git rev-parse --short "$UPSTREAM_REF")"
echo "  pending commits:"
git log --oneline "${BASE_COMMIT}..${UPSTREAM_REF}" | sed 's/^/    /'
echo

# Record pre-sync HEAD for remmerge (fork content lives there).
PRE_HEAD=$(git rev-parse HEAD)
export UPSTREAM_SYNC_HEAD_REV="$PRE_HEAD"

echo "=== 1/4 checkout xai-* from $UPSTREAM_REF ==="
bash "$SCRIPT_DIR/checkout-xai.sh" "$UPSTREAM_REF"

echo "=== 2/4 bulk port xai → agent-tui ==="
python3 "$SCRIPT_DIR/port-to-agent-tui.py" --bulk

echo "=== 3/4 remmerge fork ($PRE_HEAD) into bulk (base=$BASE_COMMIT) ==="
python3 "$SCRIPT_DIR/port-to-agent-tui.py" --remmerge \
  --base-rev "$BASE_COMMIT" --head-rev "$PRE_HEAD"

echo "=== 4/4 rewrite manifests + workspace members ==="
python3 "$SCRIPT_DIR/port-to-agent-tui.py" --manifests

# Workspace root deps often need manual review — print a hint from upstream Cargo.toml delta
echo
echo "=== Manual checklist ==="
echo "  [ ] Cargo.toml [workspace.dependencies]: new crates (ctor, rustls, zip, rhai, …)"
echo "  [ ] clippy.toml reasons use agent_tui_* path names (not xai_*)"
echo "  [ ] agent-tui-bin still thin main + product skins (not full upstream pager-bin)"
echo "  [ ] version.rs / install scripts still point at jasonkneen/agent-tui"
echo "  [ ] Wire contracts untouched: xai-grok-cli, X-XAI-Token-Auth, xai.api_key"
echo

if [[ "${SKIP_CHECK:-}" != "1" ]]; then
  echo "=== cargo check -p agent-tui-bin ==="
  if ! cargo check -p agent-tui-bin; then
    echo
    echo "cargo check failed — fix compile errors before committing."
    echo "SOURCE_REV is already $(our_source_rev). Re-run check after fixes."
    exit 1
  fi
fi

if [[ "$DO_COMMIT" -eq 1 ]]; then
  git add -A
  # Build commit body from pending messages
  body=$(
    {
      echo "Synced from monorepo"
      echo
      echo "SOURCE_REV: $OUR_BEFORE → $(our_source_rev)"
      echo
      echo "Upstream commits:"
      git log --oneline "${BASE_COMMIT}..${UPSTREAM_REF}" | sed 's/^/- /'
    }
  )
  git commit -m "Synced from monorepo" -m "$body"
  echo "Committed. Review with: git show --stat"
else
  echo "Working tree updated (not committed). Review, fix Cargo.toml if needed, then:"
  echo "  git add -A && git commit -m 'Synced from monorepo'"
  echo "Or: scripts/upstream-sync/apply.sh --commit"
fi
