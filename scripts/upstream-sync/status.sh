#!/usr/bin/env bash
# Report how far fork/agent-tui is behind upstream monorepo syncs.
# Usage: scripts/upstream-sync/status.sh
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

ensure_upstream

OUR=$(our_source_rev)
echo "Our SOURCE_REV:     $OUR"
echo "Upstream tip:       $(git rev-parse --short "$UPSTREAM_REF")  SOURCE_REV=$(git show "$UPSTREAM_REF:SOURCE_REV" | tr -d '[:space:]')"
echo "Histories share merge-base? $(git merge-base HEAD "$UPSTREAM_REF" >/dev/null 2>&1 && echo yes || echo NO — align via SOURCE_REV only)"
echo

if base=$(upstream_commit_for_source_rev); then
  echo "Matching upstream commit: $(git log -1 --oneline "$base")"
else
  die "could not find upstream commit for SOURCE_REV=$OUR"
fi

echo
echo "Pending monorepo syncs (after match → tip):"
n=0
while read -r c; do
  [[ -z "$c" ]] && continue
  n=$((n + 1))
  rev=$(git show "$c:SOURCE_REV" | tr -d '[:space:]')
  subj=$(git log -1 --format='%s' "$c")
  files=$(git diff --name-only "${c}^..${c}" | wc -l | tr -d ' ')
  echo "  $n. $(git rev-parse --short "$c")  SOURCE_REV=${rev:0:12}…  ${files} files  $subj"
  # print bullet summary from commit body if present
  git log -1 --format='%b' "$c" | sed -n 's/^- /     - /p' | head -8
done < <(pending_upstream_commits)

if [[ "$n" -eq 0 ]]; then
  echo "  (none — already at upstream SOURCE_REV)"
else
  echo
  echo "Aggregate delta (match → tip):"
  git diff --stat "$base" "$UPSTREAM_REF" | tail -5
  echo
  echo "Next: scripts/upstream-sync/apply.sh"
fi
