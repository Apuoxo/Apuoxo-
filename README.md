# Virt Extension Layer

This repository is the external working layer for Virt: persistent state, queued work, deterministic tools, and asynchronous GitHub Actions execution.

It is intentionally separate from Aether OS. It is not a memory dump. The repository is used as a durable checkpoint and execution surface so work can survive the end of a single chat turn.

The current design has four layers:

1. **STATE** — what Virt knows about the current work.
2. **QUEUE** — pending machine-readable tasks.
3. **RESULTS** — evidence produced by completed tasks.
4. **TOOLS** — deterministic allowlisted capabilities exposed to the worker.

The worker does not accept arbitrary shell commands from queue files.

This layer is being extended incrementally. Every new capability must solve a concrete limitation of the normal assistant workflow and must leave machine-readable evidence behind.

## Current proof

`probe-001` has already completed through GitHub Actions and updated `state/index.json`. This proves the basic queue → worker → result → persistent state loop.

## License

MIT
