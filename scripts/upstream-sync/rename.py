#!/usr/bin/env python3
"""Rename helpers: upstream xai-* paths/content → agent-tui-* (fork product names).

Protects wire contracts that must stay upstream-named (see FORK.md / AGENTS.md).
"""
from __future__ import annotations

import re
from typing import Optional

# Crate dir special-cases (not a mechanical xai-grok- → agent-tui- strip).
SPECIAL_CRATE = {
    "xai-grok-pager-bin": "agent-tui-bin",
    "xai-grok-agent": "agent-tui-agent",
    "xai-grok-compaction": "agent-tui-compaction",
    "xai-proto-build": "agent-tui-proto-build",
}

# Content substitutions (longer first).
CONTENT_SUBS = [
    ("xai-grok-pager-bin", "agent-tui-bin"),
    ("xai_grok_pager_bin", "agent_tui_bin"),
    ("xai-grok-agent", "agent-tui-agent"),
    ("xai_grok_agent", "agent_tui_agent"),
    ("xai-grok-compaction", "agent-tui-compaction"),
    ("xai_grok_compaction", "agent_tui_compaction"),
    ("xai-proto-build", "agent-tui-proto-build"),
    ("xai_proto_build", "agent_tui_proto_build"),
    ("xai-grok-", "agent-tui-"),
    ("xai_grok_", "agent_tui_"),
    ("XaiGrok", "AgentTui"),
]

# Leaf xai-* after xai-grok handled.
LEAF_XAI = re.compile(r"\bxai-([a-z0-9-]+)")
LEAF_XAI_US = re.compile(r"\bxai_([a-z0-9_]+)")

PROTECT = [
    r"xai-grok-cli",
    r"X-XAI-Token-Auth",
    r"xai\.api_key",
    r"XAI_API_KEY",
]

# Crates that must never be bulk-overwritten from upstream (fork product).
FORK_ONLY_CRATES = frozenset(
    {
        "agent-tui-bin",
        "agent-tui-claude-runtime",
        "agent-tui-codex-runtime",
        "agent-tui-lazar-runtime",
        "agent-tui-hermes-runtime",
    }
)


def map_crate_name(xai_name: str) -> Optional[str]:
    if xai_name in SPECIAL_CRATE:
        return SPECIAL_CRATE[xai_name]
    if xai_name.startswith("xai-grok-"):
        return "agent-tui-" + xai_name[len("xai-grok-") :]
    if xai_name.startswith("xai-"):
        return "agent-tui-" + xai_name[len("xai-") :]
    return None


def map_path(xai_path: str) -> Optional[str]:
    """Map crates/{codegen,common,build}/xai-… → agent-tui-…; ptyctl stays."""
    if xai_path.startswith("crates/codegen/ptyctl"):
        return xai_path
    parts = xai_path.split("/")
    if len(parts) < 3 or parts[0] != "crates":
        return None
    if parts[1] not in ("codegen", "common", "build"):
        return None
    mapped = map_crate_name(parts[2])
    if not mapped:
        return None
    parts[2] = mapped
    return "/".join(parts)


def rename_text(text: str) -> str:
    store = {}
    for i, pat in enumerate(PROTECT):
        found = []

        def repl(m, found=found):
            found.append(m.group(0))
            return f"__PROTECT_{i}_{len(found)-1}__"

        text = re.sub(pat, repl, text)
        store[i] = found
    for a, b in CONTENT_SUBS:
        text = text.replace(a, b)
    text = LEAF_XAI.sub(r"agent-tui-\1", text)
    text = LEAF_XAI_US.sub(r"agent_tui_\1", text)
    for i, found in store.items():
        for j, v in enumerate(found):
            text = text.replace(f"__PROTECT_{i}_{j}__", v)
    return text


def rename_bytes(data: bytes) -> bytes:
    try:
        return rename_text(data.decode("utf-8")).encode("utf-8")
    except UnicodeDecodeError:
        return data
