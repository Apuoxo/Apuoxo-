# Virt Extension Layer

This repository is an external working layer for Virt.

It is not a memory dump and it is not Aether OS. Its purpose is to provide durable state, queued work, deterministic utilities, and an asynchronous GitHub Actions worker that Virt can use across conversations.

## What this adds

- **Persistent state** in `state/`.
- **A machine-readable work queue** in `queue/`.
- **Persistent results** in `results/`.
- **Small deterministic tools** in `tools/`.
- **A long-running worker window** in GitHub Actions.
- Manual, push, scheduled, and `repository_dispatch` entry points.
- No arbitrary shell execution from queued tasks.

GitHub-hosted runners are still ephemeral. The useful property here is that the *work state* survives the runner: a task can be queued, processed, checkpointed as a result, and inspected later. GitHub Actions artifacts can also persist build/test outputs when needed.

## Operating model

```
Virt
  |
  | write task
  v
queue/*.json
  |
  v
GitHub Actions worker
  |
  +--> deterministic tool
  |
  +--> results/<task-id>.json
  |
  v
state/index.json
  |
  v
Virt reads result and continues
```

## Initial tool set

- `inventory` — repository file inventory with SHA-256
- `grep` — bounded text search
- `sha256` — hash a file
- `json_validate` — validate JSON
- `python_compile` — syntax-check Python without executing it
- `system_probe` — record runner facts

The worker deliberately does **not** accept arbitrary commands from queue files.

## Important boundary

This repository does not make Virt itself a background process. It gives Virt an external, persistent and repeatable work surface. That distinction is intentional.

## License

MIT
