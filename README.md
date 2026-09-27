# Virt Extension Layer

This repository is the external working layer for Virt: persistent state, queued work, deterministic tools, and asynchronous GitHub Actions execution.

It is intentionally separate from Aether OS. It is not a memory dump. The repository is used as a durable checkpoint and execution surface so work can survive the end of a single chat turn.

## Session boot

The persistent memory layer is part of the normal startup path for Virt.

At the beginning of a new conversation, the assistant should load `state/BOOT.json`, then `state/MEMORY.json`, and use relevant entries before making project-specific assumptions. The human-readable checkpoint is `state/MEMORY.md`.

This does not replace fresh verification. Remembered facts about active technical projects must be checked against current repository or runtime evidence when correctness depends on their current state.

See `protocol/SESSION_BOOT.md` for the full contract.

## Current layers

1. **STATE** — what Virt knows about the current work.
2. **QUEUE** — pending machine-readable tasks.
3. **RESULTS** — evidence produced by completed tasks.
4. **TOOLS** — deterministic allowlisted capabilities exposed to the worker.

The worker does not accept arbitrary shell commands from queue files.

The current design is extended incrementally. Every new capability must solve a concrete limitation of the normal assistant workflow and leave machine-readable evidence behind.

## Current proof

`verify-extension-002` completed through GitHub Actions. `state/index.json` reports completed=2 and failed=0.

## License

MIT
