# Context-compaction template family

`agent-tui-compaction` ships three distinct compaction modes, each with its own prompt template and output contract. Editing one without knowing the others breaks shared invariants.

## The three modes

### 1. Whole-conversation compaction (`compaction_developer_prompt.txt` / `compaction_user_prompt.txt`)
- Summarizes the full user↔assistant conversation for continuation.
- **Broad file definition**: "files" explicitly includes attachments, uploaded/generated/viewed images, render outputs, and any file-like content blocks — all their IDs, names, and captions must be preserved.
- Scope boundary: only the direct user-assistant history (messages, reasoning, tool calls, tool outputs) — **never** internal team communication or multi-agent interactions.
- Developer and user variants are currently near-identical twins; a change to one almost always belongs in both.

### 2. Intra-turn tool-call compaction (`intra_compaction_system.txt` / `intra_compaction_user.txt`)
- Replaces accumulated tool calls + results **mid-answer**, so the assistant continues the same turn with equal knowledge and less context.
- Required output sections, in order: **1. Task and Intent, 2. Key Findings, 3. Files and Code, 4. Errors and Fixes, 5. Actions Taken, 6. Current Progress.**
- Preserves specifics verbatim: file paths, URLs, IDs, error messages, recent code snippets, function signatures, tool parameters and results.

### 3. Full-replace code compaction (`code_compaction/templates/full_replace_summary_prompt.txt`)
- Successor assistant sees only the original query + this summary; earlier turns are discarded.
- Output is a single `<summary>...</summary>` block with fixed numbered sections (Primary Request and Intent; Key Technical Concepts; Files and Code Sections; Errors and Fixes; Problem Solving; …). Every heading appears even when empty ("None").
- **Economy is explicit**: tight prose over verbatim dumps, at most a few thousand words — "a focused summary that fits is far more useful than an exhaustive one that gets cut off." Exception: the most recent code edits are included in full.

## Shared invariants across all modes
1. **Prior summaries are authoritative and must be carried forward.** Both intra-turn and full-replace templates mark this CRITICAL: if the history already contains a compaction summary (`ConversationCompaction` markers, `<conversation_summary>` tags, a "This session is being continued" preamble), ALL its still-relevant information rolls into the new summary — this is what keeps successive compactions lossless.
2. **Analyze privately first**: templates instruct chronological review in the internal thinking channel before emitting the final summary; no separate analysis block is emitted.
3. **Specifics verbatim, narrative concise**: IDs, paths, error messages, and recent code survive exactly; everything else compresses.

## When touching these templates
A change to a shared invariant (e.g. the carry-forward rule or the verbatim-specifics list) must be applied to every template that states it; a change to one mode's section contract must match whatever code parses that mode's output.
