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
echo "=== Post-port fork restore (product + identity) ==="
# Pre-sync tip holds product skins / runtimes / bin packaging.
PRE="${UPSTREAM_SYNC_HEAD_REV:-$PRE_HEAD}"
git checkout "$PRE" -- \
  crates/codegen/agent-tui-bin \
  crates/codegen/agent-tui-claude-runtime \
  crates/codegen/agent-tui-codex-runtime \
  crates/codegen/agent-tui-lazar-runtime \
  crates/codegen/agent-tui-hermes-runtime \
  crates/codegen/agent-tui-update/src/version.rs \
  crates/codegen/agent-tui-version \
  2>/dev/null || true

# Product modules live only on the fork; re-copy and wire into pager lib.
for f in \
  crates/codegen/agent-tui-pager/src/product_profile.rs \
  crates/codegen/agent-tui-pager/src/runtime_addon.rs \
  crates/codegen/agent-tui-pager/src/runtime_backend.rs \
  crates/codegen/agent-tui-pager/src/runtime_backend \
  crates/codegen/agent-tui-shell/src/auth/local_cli
do
  git checkout "$PRE" -- "$f" 2>/dev/null || true
done

python3 - <<'PY'
from pathlib import Path
import re
p = Path("crates/codegen/agent-tui-pager/src/lib.rs")
if p.is_file():
    t = p.read_text()
    if "mod product_profile" not in t:
        m = re.search(r"(?m)^(pub )?mod app;", t)
        if m:
            insert = "\npub mod product_profile;\npub mod runtime_addon;\npub mod runtime_backend;\n"
            p.write_text(t[: m.end()] + insert + t[m.end() :])
            print("wired product mods into pager lib.rs")
# Pager needs runtime crates + which
pt = Path("crates/codegen/agent-tui-pager/Cargo.toml")
if pt.is_file():
    t = pt.read_text()
    for dep in [
        "agent-tui-codex-runtime = { workspace = true }",
        "agent-tui-claude-runtime = { workspace = true }",
        "agent-tui-lazar-runtime = { workspace = true }",
        "agent-tui-hermes-runtime = { workspace = true }",
        "which = { workspace = true }",
    ]:
        key = dep.split("=", 1)[0].strip()
        if key not in t:
            t = t.replace("[dependencies]\n", f"[dependencies]\n{dep}\n", 1)
            print("pager dep", key)
    pt.write_text(t)
# Bin: never enable local-workspace by default — OSS lacks gateway_bridge
bt = Path("crates/codegen/agent-tui-bin/Cargo.toml")
if bt.is_file():
    t = bt.read_text()
    t = re.sub(
        r'default = \[\s*"jemalloc",\s*"sandbox-enforce",\s*"local-workspace",\s*\]',
        'default = [\n    "jemalloc",\n    "sandbox-enforce",\n    ]',
        t,
    )
    bt.write_text(t)
print("fork restore done")
PY

# Rebuild bin lib body from upstream pager-bin + product prefix (API lockstep).
python3 - <<'PY'
import re, subprocess
from pathlib import Path
import sys
sys.path.insert(0, "scripts/upstream-sync")
from rename import rename_text

pre = __import__("os").environ.get("UPSTREAM_SYNC_HEAD_REV", "HEAD")
try:
    fork_lib = subprocess.check_output(
        ["git", "show", f"{pre}:crates/codegen/agent-tui-bin/src/lib.rs"], text=True
    )
except subprocess.CalledProcessError:
    fork_lib = Path("crates/codegen/agent-tui-bin/src/lib.rs").read_text()
ups = subprocess.check_output(
    ["git", "show", "upstream/main:crates/codegen/xai-grok-pager-bin/src/main.rs"], text=True
)
body = rename_text(ups)
fl = fork_lib.splitlines(True)
body_start = 0
for i, l in enumerate(fl):
    if ("jemalloc" in l and "feature" in l) or l.startswith("use anyhow"):
        body_start = i
        break
product = "".join(fl[:body_start])
# Drop multi-line #![allow] from body completely
rest = body.splitlines(True)
i = 0
if rest and rest[0].startswith("#!["):
    # skip until line with )]
    while i < len(rest):
        if ")]" in rest[i] and i > 0:
            i += 1
            break
        i += 1
    while i < len(rest) and rest[i].strip() == "":
        i += 1
body_rest = "".join(rest[i:])
body_rest = re.sub(r"(?m)^fn main\s*\(", "pub fn run(", body_rest)
final = product + body_rest
if "pub fn run(" not in final:
    final = re.sub(r"(?m)^fn main\s*\(", "pub fn run(", final)
Path("crates/codegen/agent-tui-bin/src/lib.rs").write_text(final)
Path("crates/codegen/agent-tui-bin/src/main.rs").write_text(
    "//! Single core binary. Product skins are **symlinks** "
    "(`scripts/link-product-bins.sh`).\n\n"
    "fn main() {\n"
    "    agent_tui_bin::apply_product_from_invocation();\n"
    "    agent_tui_bin::run();\n"
    "}\n"
)
print("rebuilt agent-tui-bin lib+main")
PY

echo "=== Manual checklist ==="
echo "  [ ] Cargo.toml [workspace.dependencies]: new crates (ctor, rustls, zip, rhai, …)"
echo "  [ ] members/path deps for agent-tui-extra-ca, agent-tui-workflow when present"
echo "  [ ] agent-tui-bin still thin main + product skins"
echo "  [ ] version.rs / install hints → jasonkneen/agent-tui (not x.ai/cli)"
echo "  [ ] Wire contracts: xai-grok-cli, X-XAI-Token-Auth, xai.api_key"
echo "  [ ] Do NOT enable local-workspace by default (OSS lacks gateway_bridge)"
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
