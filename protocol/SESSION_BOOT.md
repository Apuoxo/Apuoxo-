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
