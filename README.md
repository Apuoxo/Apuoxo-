[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Python 3.9+](https://img.shields.io/badge/python-3.9+-blue.svg)](https://www.python.org/downloads/)

# Stanislav Usychenko (Locus)

## Adaptive Collapse Dynamics: A Unified Framework

I develop a unified variational framework where **stability, degradation, and collapse** emerge from a single principle: minimization of generalized free energy under adaptive margin `L(t)`.

### The Core Formula

**V = L² / (t + ε)**

Where:
- **V** — vitality (remaining useful life)
- **L** — adaptive stability margin (L² norm of state vector)
- **t** — time or age
- **ε** — small constant to avoid division by zero

Lower vitality → collapse/eviction.

---

## Research Projects

| Project | Domain | Key Idea |
|---------|--------|----------|
| **[Lt-Adaptive-Collapse](https://github.com/Apuoxo/Lt-Adaptive-Collapse)** | Theory | Unified variational principle for collapse dynamics |
| **[L2-norm-dynamic-prioritization](https://github.com/Apuoxo/L2-norm-dynamic-prioritization)** | Distributed Systems | `L_i(t) = ||x_i(t)||_2` for node ranking |
| **[learned-kv-pruning](https://github.com/Apuoxo/learned-kv-pruning)** | LLM Optimization | MLP-predicted utility + temporal decay for KV cache |
| **[mimo-rls-lqr-governor](https://github.com/Apuoxo/mimo-rls-lqr-governor)** | Control Systems | Adaptive DVFS with online system ID |
| **[FeedbackCoupled-SafeAdaptiveControl](https://github.com/Apuoxo/FeedbackCoupled-SafeAdaptiveControl)** | Safe Control | Supervisory switching with safety guarantees |

---

## The Unifying Idea

All projects share the same mathematical core:

**Lt-Adaptive-Collapse** (General Theory)
    ↓
    V = L² / (t + ε)
    ↓
- **L2-Prioritization**     ← L = ||x||₂
- **KV-Cache Pruning**      ← wⱼ² / (aⱼ + ε)
- **Safe Control**          ← L(t) as stability margin
- **DVFS Governor**         ← LQR with adaptive L(t)

---

## Publications (2026)

- *Scoring-Based KV Cache Pruning with Learned Utility and Temporal Decay*
- *Adaptive Collapse Dynamics: A Unified Variational Principle*
- *Supervisory Multi-Model Safe Adaptive Control*
- *Application of the L² Norm for Dynamic Prioritization of Distributed Information Nodes*
- *MIMO-RLS-LQR Adaptive Governor for Multi-Cluster DVFS*

---

## License

MIT © 2026 Stanislav Usychenko