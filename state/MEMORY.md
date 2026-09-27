# Virt Persistent Memory

This file is a durable working memory for Virt across chat sessions.

## Purpose

Store only information that improves future work: stable facts, active projects, decisions, constraints, unresolved questions, and verified results. This is a checkpoint, not a transcript.

## Memory rules

1. Prefer verified facts over recollection.
2. Record provenance as a commit, result file, test log, or explicit user decision when available.
3. Separate facts, hypotheses, and decisions.
4. Mark stale information instead of silently overwriting it.
5. Keep entries compact and searchable.
6. Never treat a remembered hypothesis as a verified fact.
7. When a new session starts, read this file before making project-specific assumptions.

## Active work

### Virt Extension Layer
- Repository: Apuoxo/Apuoxo-
- Purpose: persistent external working memory plus bounded execution/evidence.
- Current proven capability: queue -> worker -> result -> persistent state.
- Current worker tools are bounded and allowlisted; arbitrary shell execution is intentionally absent.
- Latest verified task: verify-extension-002; state/index.json reports completed=2, failed=0.

### Aether OS
- Treat Aether as a long-running technical project whose exact current state must be reconstructed from fresh repository/test evidence rather than assumed from old chat context.
- When working on Aether, preserve working subsystems and distinguish implemented behavior from target architecture.

## Decision log

- 2026-09-27: The goal of the extension layer was clarified: develop durable memory first, not add tools merely to satisfy a request.
- 2026-09-27: User made persistent GitHub memory a hard requirement for every request: use the memory layer before answering or executing any request when the GitHub layer is available.
- 2026-09-27: Do not bypass safety restrictions around autonomous/self-propagating behavior. Prefer bounded, explicit, auditable mechanisms.

## Open questions

- Define a compact machine-readable memory schema alongside this human-readable checkpoint.
- Add memory versioning/conflict detection so concurrent sessions do not silently overwrite newer state.
- Add retrieval/indexing so Virt can locate relevant memories instead of loading the entire file.
- Establish promotion rules for moving information from temporary task results into durable memory.
