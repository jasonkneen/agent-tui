# Workflow: audit fork source for hardcoded vendor model IDs and literal catalog embeddings

The vendor-model-catalogs-fetched-live convention forbids hardcoding vendor model IDs into fork source; a literal non-Grok model ID in the diff is a defect. This workflow systematically finds and removes violations before a vendor's catalog rung ships.

## 1. Enumerate known vendor model IDs and API names

From the vendor's own documentation or live runtime query:

| Vendor | Known model IDs (examples) | Catalog query endpoint |
|---|---|---|
| Codex | `gpt-4-1106-preview`, `gpt-4`, `gpt-3.5-turbo`, ... | `model/list` over warm app-server |
| Claude | `claude-3-opus-20240229`, `claude-3-sonnet-20240229`, ... | `claude -p --output-format json` |
| Grok | Built-in sampler catalog (exempt) | N/A |

Run the vendor's own catalog query (if it's live in your environment) and record the current model list as a reference. Note: the list will change as vendors release new models, so this snapshot is evidence for "what was live when we swept".

## 2. Search for hardcoded model IDs

Grep the entire fork source for model ID strings:

```bash
# Search for Claude model patterns
grep -r 'claude-3' src/ tests/ --include='*.rs' --include='*.ts' --include='*.js'
grep -r 'claude-opus\|claude-sonnet' src/ tests/ --include='*.rs' --include='*.ts' --include='*.js'

# Search for Codex model patterns
grep -r 'gpt-4\|gpt-3.5' src/ tests/ --include='*.rs' --include='*.ts' --include='*.js'
grep -r 'text-davinci' src/ tests/ --include='*.rs' --include='*.ts' --include='*.js'
```

**Exclude:** Grok / xAI model IDs (which may legitimately be hardcoded as the sampler default), test fixtures that explicitly validate "reject unknown model" behavior, CLI help text or docs quoting examples, and configuration file examples (these are docs, not operative source).

**Record:** every match with file path, line number, and context.

## 3. Search for embedded catalog lists or constants

Grep for structures that look like model catalogs:

```bash
# Vendored model lists, const arrays, or static tables
grep -rn 'MODEL\|MODELS\|CATALOG' src/ --include='*.rs' --include='*.ts' --include='*.js' | grep -E 'const|static|Vec|\[.*\]'
```

**Record:** any structure that contains multiple model names or IDs, with file and context.

## 4. Verify each match against the vendor's contract

For each match found:

1. **Is it a legitimate test fixture or doc example?** If the code's purpose is to validate input validation (e.g., "reject model names outside the live catalog"), a hardcoded list in the test is acceptable — document that this is test-only.
2. **Is it operative source that reaches shipped binaries?** If yes: it's a defect.
3. **What should it be instead?** Replace with a call to the vendor's live catalog query at runtime (the validate-vendor-warmpool-lifecycle workflow describes how each vendor serves its catalog).

## 5. Remove or replace violations

- **Remove** hardcoded model IDs from operative source; replace any reference with a runtime query.
- **Update** any default-model constant to fetch from the vendor's runtime catalog on first connection, or document the exception and why it's necessary.
- **Add a test** that verifies the vendor's actual model list is queried (via mock or integration test), not a static list.

## 6. Verify no second catalog source was invented

For each vendor with rung 2 or higher, check:

- Codex: only `model/list` over the warm app-server, never a REST endpoint or hardcoded fallback.
- Claude: only the `claude -p --output-format json` harness, never a direct `api.anthropic.com` call or SDK model list.

If a second source exists, remove it or mark it as an emergency fallback with explicit documentation of why it's necessary.
