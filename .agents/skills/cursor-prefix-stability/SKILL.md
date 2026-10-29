---
name: cursor-prefix-stability
description: Implement and review Cursor BYOK conversation projection with append-only provider history and prefix-cache stability. Use when changing runtime prompts, request context, canonical messages, checkpoint hydration, compaction, message identity, or provider history serialization under server.
---

# Cursor prefix stability

Preserve the longest valid provider prefix across turns. Treat provider-visible history as an append-only log unless an explicit compaction operation replaces it.

## Architecture

Keep the invariant in the provider-independent conversation layers:

```text
server/
├── prompt/cursor/*/runtime.md       Per-turn runtime content
├── src/cursor/request/              Request context and runtime compilation
├── src/cursor/projection/           Canonical ↔ Cursor checkpoint projection
├── src/cursor/checkpoint/           Stable roots, turns, and hydration
├── src/run/                         Provider-independent model history
└── src/provider/                    Provider-specific serialization only
```

Do not solve prefix instability independently in each provider adapter. Produce one stable canonical history before dispatching to OpenAI Responses, OpenAI Chat, Anthropic, or another provider.

## Required invariants

- When no compaction occurs, the complete provider-visible history from turn N must be an exact structural prefix of turn N+1. Never edit, remove, merge, reorder, renormalize, or regenerate an earlier message.
- Keep `PromptSpec.instructions` and the stable tool prefix byte-stable when their inputs have not changed. Deterministic ordering is required; do not use unordered iteration in provider-visible output.
- Separate conversation/request context from the per-turn runtime message. The runtime message contains current-turn material such as the user query, selected context, open files, action context, mode reminders, and timestamp.
- Project rules, skills, subagents, environment/Git context, and MCP metadata as a stable `request-context:*` message:
  - append it on the first applicable turn;
  - do not append it again when its compiled content is identical to the latest projected request context;
  - when its content changes, append a new request-context message immediately before the current runtime message;
  - never represent a context change by rewriting the system prompt, replacing an earlier context message, or mutating a checkpoint root.
- Give every appended context update a unique event identity. Compare the latest context by content, not only by identifier, so `A → B → A` appends the final `A` again while retries of the same event remain idempotent.
- Preserve `request-context:*` wire identity through checkpoint encoding and hydration. Deduplication must still work after process restart or conversation resume.
- Automatic compaction is an explicit prefix reset. Compact obsolete history, retain exactly the latest request-context message, then place the summary and current initial messages in deterministic order. Manual compaction may reproject current context on the next user turn.
- Background completions and injected runtime events must not manufacture duplicate request context unless they actually start a user turn whose context changed.

## Change workflow

Before editing, trace the whole path that applies:

```text
AgentRunRequest
→ request context hydration/compilation
→ CanonicalMessage identity and persistence
→ checkpoint encode/decode
→ projected ModelRequest history
→ provider serialization
```

Determine which data is conversation-level and which is turn-level. If a proposed change moves or rewrites an earlier provider-visible value, redesign it as a new append-only event unless the operation is explicitly compaction.

Use TDD for changes in this path. Start with a failing behavioral test, then implement the smallest provider-independent change.

## Verification

Cover the affected behavior with structural assertions, not token-count estimates alone:

- Two turns with identical request context: the first request history is an exact prefix of the second, the system instructions are identical, and only one `request-context:*` message exists.
- Changed context: one new context message appears at the tail before the new runtime query; all earlier messages remain unchanged.
- Context reversion `A → B → A`: three distinct context events are retained in order.
- Retry of one runtime event: no duplicate or conflicting context message is persisted.
- Checkpoint round-trip: request-context identity and content survive encode/hydrate.
- Automatic compaction: only the latest context is retained outside the summary.
- Runtime templates render without embedding conversation-level rules or MCP metadata in every user query.

Run focused tests first, then the relevant server suites:

```bash
cargo test --lib
cargo test --test runtime_modes
cargo test --test prefix_stability
cargo test --test checkpoint_recovery
cargo test --test compaction
cargo clippy --lib -- -D warnings
cargo fmt --all -- --check
```

Do not repair unrelated dirty-worktree failures while validating. Report them separately.
