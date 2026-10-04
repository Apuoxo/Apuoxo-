# Virt Autonomy Implementation Plan

Status: active
Baseline: 091bd13ab5151956314dd3aac042268d6bb14814

## Execution rule

Implementation proceeds as small, evidence-backed increments:

inspect -> design -> implement -> build/test -> verify evidence -> checkpoint -> next increment

Each increment must have one clear purpose. No speculative dependency expansion and no unbounded autonomous execution.

## 1. Structural analysis

Map the existing persistent state, queue, worker, result, audit and verification surfaces. Identify which responsibilities already exist and which are missing before adding new libraries or abstractions.

Exit evidence:
- current state/queue/result protocol understood;
- existing worker boundary identified;
- duplication and unsafe coupling recorded.

## 2. Capability contract

Define a provider-neutral capability model for model calls, GitHub operations, local execution, artifacts, memory and verification.

Core concepts:
- Goal
- Task
- Capability
- Action
- Observation
- Result
- Evidence
- Policy
- Checkpoint

Every capability must expose a bounded input schema, explicit output schema and an auditable result.

## 3. Runtime core

Introduce the smallest reusable Rust core for task execution and capability dispatch. Keep policy and provider-specific code outside the core.

Initial library boundaries:
- virt-core
- virt-tools
- virt-policy
- virt-events

The core must compile and run independently of a particular model provider.

## 4. Model abstraction

Add a provider-neutral model interface and adapters only where a real execution path exists. Separate:
- model selection;
- prompt/context assembly;
- tool-call interpretation;
- execution;
- result validation.

Do not bind the runtime to a retired service or a single vendor API.

## 5. GitHub and MCP integration

Turn existing GitHub interaction into first-class capabilities and add an MCP boundary for external tools. Discovery, schema validation, permission checks and result capture remain explicit.

## 6. Planner and verifier

Implement a bounded plan -> action -> observation -> verification loop. Verification must be able to reject a result, request another diagnostic pass, or stop for human input.

## 7. Persistent memory

Extend the existing checkpoint/memory system so verified project facts, failed hypotheses, decisions and execution evidence can be linked to tasks without silently rewriting history.

## 8. Specialized agents

Add narrowly scoped roles only after the common runtime is stable:
- researcher
- architect
- implementer
- reviewer
- CI/debugger
- hardware bridge
- release/evidence

Roles share the same capability and policy layer.

## 9. Autonomous engineering loop

Compose the existing GitHub workflow discipline into executable runtime behavior:
inspect -> decide -> change -> commit -> verify -> CI -> inspect evidence -> continue.

The runtime must stop on missing evidence, policy boundaries, conflicting state or required human decisions.

## 10. Hardware bridge

Only after the software loop is proven, add a bounded bridge for Aether/AH532 diagnostics and hardware-test evidence. Hardware evidence remains distinct from CI/QEMU evidence.

## 11. Build and distribution

Create reproducible workspace builds, tests, artifacts and versioned capability manifests. Each added dependency must have a concrete reason, a license check and a verification path.

## 12. Readiness gates

A stage is complete only when:
- source is committed;
- relevant tests/builds pass;
- generated evidence is inspected;
- persistent state/checkpoint is updated;
- no unverified claim is promoted to fact.

## Immediate next implementation target

Complete the structural inventory of the existing Virt Extension Layer, then add the minimal provider-neutral core contract. Do not add model SDKs, MCP clients or autonomous scheduling until the core contract is tested.
