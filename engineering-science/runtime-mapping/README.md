# Runtime Mapping — the binding crosswalk

This folder is the **binding layer** of the Engineering Science Layer: the crosswalk that proves the science above is load-bearing, not decoration. Every concept in `mathematics/`, `physics/`, `electrical/`, `pcb/`, `manufacturing/`, and `industry/` is traced *down* to the concrete runtime artifact that embodies it — a compiler IR, a state-machine phase, a constraint or verification rule, an engine, and a learning hook — and each link resolves to a real document or a real symbol in the `eak` workspace. If a science concept has a row here it is enforced; if it has none, it is decoration. Each doc below takes one lens (the compiler chain, the constraint engine, the phase machines, verification, knowledge/learning, or the dependency DAG); [`concept-runtime-crosswalk.md`](./concept-runtime-crosswalk.md) is the master page that unifies them.

## Documents

| Document | What it grounds |
|----------|-----------------|
| [concept-runtime-crosswalk.md](./concept-runtime-crosswalk.md) | The master page: every science concept traced to a runtime engine, compiler IR, state-machine phase, constraint/verification rule, and learning hook — the single proof the theory is load-bearing. |
| [compiler-ir-mapping.md](./compiler-ir-mapping.md) | The IR-lowering spine: each engineering invariant is a *type rule* on the Requirement → Engineering → BOM/Schematic → PCB → Manufacturing chain that must hold within an IR and survive each lowering. |
| [constraint-mapping.md](./constraint-mapping.md) | The `⟨X,D,C⟩` of constraint satisfaction made of running code: how Constraint Extraction derives typed bounds and the Constraint Engine stores, resolves, and checks them. |
| [state-machine-mapping.md](./state-machine-mapping.md) | The fourteen phase state machines as enforcement organs: each phase names the dominant science it enforces, the engine it calls, and the IR it produces or checks — with loop-backs and gates as control/decision theory. |
| [verification-mapping.md](./verification-mapping.md) | Each verification rule (ERC/DRC/DFM/EMC) actually implemented in `eak-engines`, traced back to the law of physics, geometry, or manufacturing it defends and forward to the two-tier gate that blocks release. |
| [learning-mapping.md](./learning-mapping.md) | The knowledge-and-learning binding: how durable science plus empirical per-run experience flow sideways across phases through the Knowledge Graph, Vector Memory, and Learning Engine — with the wall that keeps science, experience, and product know-how apart. |
| [dependency-mapping.md](./dependency-mapping.md) | The science layer as a directed acyclic graph: each concept derived from more fundamental ones, in the same direction the clean-architecture Dependency Rule (P1) makes law for the code. |

---

Up: [Engineering Science Layer index](../README.md) · Master crosswalk: [concept-runtime-crosswalk.md](./concept-runtime-crosswalk.md)
