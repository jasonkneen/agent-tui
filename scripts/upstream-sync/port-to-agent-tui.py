#!/usr/bin/env python3
"""Port current xai-* working tree into agent-tui-* with rename + optional 3-way fork merge.

Modes:
  --bulk     Overwrite agent-tui crates from current xai trees (skip fork-only crates).
  --remmerge Re-merge pre-sync HEAD fork content into the bulk result (3-way via git merge-file).
  --manifests Rewrite agent-tui Cargo.toml from xai + inject pager runtime deps.

Typical apply order (see apply.sh):
  checkout-xai.sh → port --bulk → port --remmerge → port --manifests
"""
from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import List, Optional

sys.path.insert(0, str(Path(__file__).resolve().parent))
from rename import (  # noqa: E402
    FORK_ONLY_CRATES,
    map_crate_name,
    map_path,
    rename_bytes,
    rename_text,
)

ROOT = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
os.chdir(ROOT)

TEXT_EXT = {
    ".rs",
    ".toml",
    ".md",
    ".json",
    ".txt",
    ".yml",
    ".yaml",
    ".sh",
    ".ps1",
    ".rhai",
    ".html",
    ".css",
    ".js",
    ".ts",
    ".sql",
    ".proto",
}

CONFLICT_RE = re.compile(
    r"<<<<<<<[^\n]*\n(.*?)^=======\n(.*?)^>>>>>>>[^\n]*\n",
    re.DOTALL | re.MULTILINE,
)

RUNTIME_DEPS = [
    "agent-tui-codex-runtime = { workspace = true }",
    "agent-tui-claude-runtime = { workspace = true }",
    "agent-tui-lazar-runtime = { workspace = true }",
    "agent-tui-hermes-runtime = { workspace = true }",
]


def run(cmd, **kw):
    return subprocess.run(cmd, check=False, capture_output=True, text=True, **kw)


def git_show(rev, path):
    # type: (str, str) -> Optional[bytes]
    r = subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True)
    return r.stdout if r.returncode == 0 else None


def bulk_sync() -> None:
    """Copy every xai-* crate → agent-tui-* (skip fork-only)."""
    n = 0
    for area in ("crates/codegen", "crates/common", "crates/build"):
        area_p = ROOT / area
        if not area_p.is_dir():
            continue
        for src in sorted(area_p.iterdir()):
            if not src.is_dir() or not src.name.startswith("xai"):
                continue
            mapped = map_crate_name(src.name)
            if not mapped or mapped in FORK_ONLY_CRATES:
                print(f"SKIP {src.name}")
                continue
            dst = area_p / mapped
            if dst.exists():
                shutil.rmtree(dst)
            for root, dirs, files in os.walk(src):
                dirs[:] = [d for d in dirs if d not in {".git", "target"}]
                rel = os.path.relpath(root, src)
                out_dir = dst if rel == "." else dst / rel
                out_dir.mkdir(parents=True, exist_ok=True)
                for f in files:
                    sp = Path(root) / f
                    dp = out_dir / f
                    if sp.suffix.lower() in TEXT_EXT or sp.name in {
                        "Cargo.toml",
                        "README",
                        "LICENSE",
                        "CHANGELOG",
                    }:
                        try:
                            dp.write_bytes(rename_bytes(sp.read_bytes()))
                        except Exception:
                            shutil.copy2(sp, dp)
                    else:
                        shutil.copy2(sp, dp)
            print(f"SYNC {src.name} → {mapped}")
            n += 1
    print(f"bulk: {n} crates")


def resolve_conflict(path: str, ours: str, theirs: str) -> str:
    if ours.strip() == "":
        return theirs
    if theirs.strip() == "":
        return ours
    fork_keys = (
        "product_profile",
        "runtime_addon",
        "runtime_backend",
        "apply_product",
        "AGENT_TUI_PRODUCT",
        "claude_runtime",
        "codex_runtime",
        "lazar_runtime",
        "hermes_runtime",
    )
    if any(k in path for k in ("product_profile", "runtime_addon", "runtime_backend", "agent-tui-bin")):
        return ours
    if any(k in ours for k in fork_keys):
        return ours
    return theirs


def remmerge(base_rev: str, head_rev: str = "HEAD") -> None:
    """3-way: base=rename(xai@base_rev), ours=head_rev agent-tui, theirs=working tree."""
    out = subprocess.check_output(["git", "ls-tree", "-r", "--name-only", head_rev], text=True)
    agent_files = [p for p in out.splitlines() if "/agent-tui-" in p and not p.endswith(".png")]
    stats = {
        "merged": 0,
        "took_head": 0,
        "took_bulk": 0,
        "conflict": 0,
        "skip": 0,
        "added_head": 0,
    }
    conflicts = []  # type: List[str]
    tmp = Path(tempfile.mkdtemp(prefix="upstream-remmerge-"))

    def reverse_map(agent_path):
        # type: (str) -> Optional[str]
        parts = agent_path.split("/")
        if len(parts) < 3:
            return None
        name = parts[2]
        reverse = {
            "agent-tui-bin": "xai-grok-pager-bin",
            "agent-tui-agent": "xai-grok-agent",
            "agent-tui-compaction": "xai-grok-compaction",
            "agent-tui-proto-build": "xai-proto-build",
        }
        if name in reverse:
            parts[2] = reverse[name]
            return "/".join(parts)
        if not name.startswith("agent-tui-"):
            return None
        rest = name[len("agent-tui-") :]
        for cand in (f"xai-grok-{rest}", f"xai-{rest}"):
            cpath = "/".join(parts[:2] + [cand] + parts[3:])
            if git_show(base_rev, cpath) is not None:
                return cpath
        return "/".join(parts[:2] + [f"xai-grok-{rest}"] + parts[3:])

    try:
        for path in agent_files:
            if any(
                x in path
                for x in (
                    "claude-runtime",
                    "codex-runtime",
                    "lazar-runtime",
                    "hermes-runtime",
                )
            ):
                subprocess.run(["git", "checkout", head_rev, "--", path], capture_output=True)
                stats["took_head"] += 1
                continue

            head_b = git_show(head_rev, path)
            if head_b is None:
                stats["skip"] += 1
                continue
            try:
                head_t = head_b.decode("utf-8")
            except UnicodeDecodeError:
                stats["skip"] += 1
                continue

            wt = Path(path)
            if not wt.is_file():
                wt.parent.mkdir(parents=True, exist_ok=True)
                wt.write_bytes(head_b)
                stats["added_head"] += 1
                continue

            try:
                bulk_t = wt.read_text(encoding="utf-8")
            except Exception:
                stats["skip"] += 1
                continue
            if bulk_t == head_t:
                stats["skip"] += 1
                continue

            xai_path = reverse_map(path)
            base_b = git_show(base_rev, xai_path) if xai_path else None
            if base_b is None:
                if any(
                    k in path
                    for k in (
                        "product_profile",
                        "runtime_addon",
                        "runtime_backend",
                        "install.sh",
                        "install.ps1",
                    )
                ):
                    wt.write_text(head_t, encoding="utf-8")
                    stats["took_head"] += 1
                else:
                    stats["took_bulk"] += 1
                continue

            try:
                base_t = rename_text(base_b.decode("utf-8"))
            except UnicodeDecodeError:
                stats["skip"] += 1
                continue

            if head_t == base_t:
                stats["took_bulk"] += 1
                continue
            if bulk_t == base_t:
                wt.write_text(head_t, encoding="utf-8")
                stats["took_head"] += 1
                continue

            (tmp / "base").write_text(base_t)
            (tmp / "ours").write_text(head_t)
            (tmp / "theirs").write_text(bulk_t)
            r = subprocess.run(
                ["git", "merge-file", "-p", str(tmp / "ours"), str(tmp / "base"), str(tmp / "theirs")],
                capture_output=True,
                text=True,
            )
            merged = r.stdout
            if r.returncode != 0:

                def repl(m, path=path):
                    return resolve_conflict(path, m.group(1), m.group(2))

                merged = CONFLICT_RE.sub(repl, merged)
                if "<<<<<<<" in merged:
                    conflicts.append(path)
                    stats["conflict"] += 1
                else:
                    stats["merged"] += 1
            else:
                stats["merged"] += 1
            wt.write_text(merged, encoding="utf-8")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    print("remmerge stats:", stats)
    if conflicts:
        print("remaining conflicts:", len(conflicts))
        for c in conflicts[:40]:
            print(" ", c)
        sys.exit(2)


def rewrite_manifests() -> None:
    """Rewrite agent-tui Cargo.toml from current xai crates; inject pager runtime deps."""
    n = 0
    for area in ("crates/codegen", "crates/common", "crates/build"):
        area_p = ROOT / area
        for src in sorted(area_p.iterdir()):
            if not src.is_dir() or not src.name.startswith("xai"):
                continue
            mapped = map_crate_name(src.name)
            if not mapped or mapped in FORK_ONLY_CRATES:
                continue
            sp, dp = src / "Cargo.toml", area_p / mapped / "Cargo.toml"
            if not sp.is_file() or not dp.parent.is_dir():
                continue
            text = rename_text(sp.read_text(encoding="utf-8"))
            if mapped == "agent-tui-pager":
                for dep in RUNTIME_DEPS:
                    key = dep.split("=", 1)[0].strip()
                    if key not in text:
                        text = text.replace(
                            "[dependencies]\n",
                            f"[dependencies]\n{dep}\n",
                            1,
                        )
            if mapped == "agent-tui-update":
                head = git_show("HEAD", str(dp.relative_to(ROOT)))
                if head:
                    m = re.search(r'(?m)^version = ".*"', head.decode("utf-8", errors="replace"))
                    if m:
                        text = re.sub(r'(?m)^version = ".*"', m.group(0), text, count=1)
            dp.write_text(text, encoding="utf-8")
            n += 1
            print(f"manifest {mapped}")
    # agent-tui-bin stays fork packaging — restore from HEAD, optional feature inject
    bin_toml = ROOT / "crates/codegen/agent-tui-bin/Cargo.toml"
    head = git_show("HEAD", "crates/codegen/agent-tui-bin/Cargo.toml")
    if head and bin_toml.parent.is_dir():
        text = head.decode("utf-8")
        if "local-workspace" not in text:
            text = text.replace(
                'sandbox-enforce = ["agent-tui-pager/sandbox-enforce"]\n',
                'sandbox-enforce = ["agent-tui-pager/sandbox-enforce"]\n'
                'local-workspace = ["agent-tui-pager/local-workspace"]\n',
            )
        bin_toml.write_text(text, encoding="utf-8")
        print("manifest agent-tui-bin (from HEAD)")
        n += 1
    print(f"manifests: {n}")


def ensure_workspace_members() -> None:
    """Best-effort: add agent-tui-extra-ca / agent-tui-workflow if crates exist."""
    cargo = ROOT / "Cargo.toml"
    text = cargo.read_text(encoding="utf-8")
    adds = []
    for member, path_line in (
        (
            "crates/codegen/agent-tui-extra-ca",
            'agent-tui-extra-ca = { path = "crates/codegen/agent-tui-extra-ca" }',
        ),
        (
            "crates/codegen/agent-tui-workflow",
            'agent-tui-workflow = { path = "crates/codegen/agent-tui-workflow" }',
        ),
    ):
        if (ROOT / member).is_dir() and member not in text:
            # insert into members list after agent-tui-env if possible
            needle = '    "crates/codegen/agent-tui-env",\n'
            if needle in text:
                text = text.replace(needle, needle + f'    "{member}",\n', 1)
            else:
                adds.append(f"WARN: add member {member} manually")
            if path_line.split("=", 1)[0].strip() + " =" not in text:
                anchor = 'agent-tui-env = { path = "crates/codegen/agent-tui-env" }\n'
                if anchor in text:
                    text = text.replace(anchor, anchor + path_line + "\n", 1)
    cargo.write_text(text, encoding="utf-8")
    for a in adds:
        print(a)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bulk", action="store_true")
    ap.add_argument("--remmerge", action="store_true")
    ap.add_argument("--manifests", action="store_true")
    ap.add_argument(
        "--base-rev",
        default=None,
        help="xai tree base for remmerge (default: commit matching SOURCE_REV before apply)",
    )
    ap.add_argument("--head-rev", default="HEAD", help="fork tip for remmerge (default HEAD)")
    args = ap.parse_args()
    if not any((args.bulk, args.remmerge, args.manifests)):
        ap.error("pass at least one of --bulk --remmerge --manifests")

    if args.bulk:
        bulk_sync()
    if args.remmerge:
        base = args.base_rev
        if not base:
            # Prefer ORIG_HEAD or env set by apply.sh
            base = os.environ.get("UPSTREAM_SYNC_BASE_REV")
        if not base:
            print("error: --remmerge needs --base-rev or UPSTREAM_SYNC_BASE_REV", file=sys.stderr)
            sys.exit(1)
        remmerge(base, args.head_rev)
    if args.manifests:
        rewrite_manifests()
        ensure_workspace_members()


if __name__ == "__main__":
    main()
