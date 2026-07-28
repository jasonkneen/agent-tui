---
name: probe-model-pick-persistence
description: Empirically establish where (and whether) a vendor's model pick persists in Agent TUI — make a pick, read the vendor-scoped runtime.toml key through the config-home chain, confirm restart survival, and confirm thread-boundary application — so persistence claims in docs and drift rows rest on observed writes, not labels.
---

# Probe where a model pick actually persists

This procedure is currently restated in three places: the probe-vendor-readiness-rung skill's rung-2 check ("pick a model and confirm the write: a `<vendor>_model` key appears in `~/.agent-tui/runtime.toml` and applies on the next thread"), the Grok model-pick drift workflow's ground-truth step ("pick a non-default model via `/model` … did any key change? … Restart the TUI and check whether the pick survives"), and the shipped-marker audit ("does the persistence key land in `~/.agent-tui/runtime.toml`?"). Per the extraction convention, two independent restatements of one procedure are the extraction signal — this skill is the shared procedure; repoint those sites here.

## Setup

Run the prepare-wire-probe-environment skill first: a current build launched **by path** from the build output (never Spotlight/Dock/bundle id), with the vendor's own CLI logged in. A stale binary or logged-out CLI makes every persistence result untrustworthy.

## The probe

1. **Select the vendor and record the baseline.** `/runtime <vendor>`, then read `~/.agent-tui/runtime.toml` — resolving the home via `$AGENT_TUI_HOME` with legacy `$GROK_HOME` fallback, never a hardcoded path — and note which keys exist before the pick.
2. **Make a non-default pick.** Open `/model` and choose a model that is not the current default, so a write is distinguishable from a no-op.
3. **Check the write.** Re-read `runtime.toml`. Did a vendor-scoped `<vendor>_model` key appear or change? Record the exact key name and value — "some key changed" is not evidence; the key identifies which persistence scheme the vendor is on (vendor-scoped file key vs pre-scheme sampler-side state, the disputed fact in the Grok drift).
4. **Check restart survival.** Quit and relaunch the TUI (again by path). Does `/model` show the pick as current? A pick that survives only in-process is not persisted.
5. **Check when it applies.** Confirm the pick takes effect on the **next thread**, not mid-thread — thread-boundary application is part of the persistence contract, and an immediate mid-thread swap is itself a deviation worth recording.

## Recording the result

State exactly: which key (if any) was written, whether it survived restart, and when it applied. A "no key changed but the pick survived restart" outcome means persistence lives somewhere else — that is a finding, not a failure; note the candidate location rather than guessing. Feed the result into the owning drift row or doc section with author and date, since an unattributed value cannot serve as evidence later.
