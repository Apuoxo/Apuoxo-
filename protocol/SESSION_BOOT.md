# Virt Session Boot Contract

## Rule

Whenever Virt handles a user request, the persistent memory layer is part of the normal startup path.

### Boot sequence

1. Read `state/BOOT.json`.
2. Read `state/MEMORY.json`.
3. Load `state/MEMORY.md` when human-readable context is needed.
4. Use only entries relevant to the current request.
5. For technical projects, verify current repository/runtime state before treating remembered implementation details as current.
6. After meaningful changes, update the persistent checkpoint with verified facts and decisions.

## Important limitation

GitHub stores and exposes the contract; it cannot force the ChatGPT model to execute a tool call. The assistant must follow this contract whenever the GitHub layer is available.

## Safety

The memory layer is persistent state, not a continuously running agent. It must remain bounded, explicit, and auditable.

### Request-cycle audit

For each handled user request, Virt should create a bounded `request` audit event before substantive work when the GitHub layer is available, then create an `action` and/or `result` event when work is performed or completed. These events are traceability records, not a transcript.

If the exact platform message timestamp is unavailable, the event must not pretend to know it; the worker timestamp is the processing time.

### Memory consolidation

The worker may generate bounded `memory_candidates` from checkpoint/result audit events. Candidates are evidence-backed proposals only; they do not modify `MEMORY.json` automatically. Promotion into durable memory requires an explicit checkpoint/update step.

### Memory promotion

`memory_promote` may add one evidence-backed entry to `state/MEMORY.json`. Promotion is explicit: the task must provide the complete entry, including provenance. Existing IDs cannot be overwritten; a conflicting ID fails instead of silently replacing memory.

### Memory revision

When a verified fact becomes stale, use `memory_revision` instead of overwriting it. The old entry is marked `superseded`, and a new entry records `supersedes` plus provenance. This preserves the history of what was believed and why it changed.
