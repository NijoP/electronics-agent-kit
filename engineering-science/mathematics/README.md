# Mathematics

The formal and algorithmic substrate of the Engineering Science Layer — the mathematics the EAK runtime actually computes with. A netlist is a **graph**, a design rule is a **constraint**, placement and routing are **optimization** and **search**, a board is **geometry**, a circuit is **linear algebra**, a field solve is a **numerical method**, a tolerance is **statistics**, the manufacturing gate is a **decision under uncertainty**, and the verify-and-fix loop is **control**. The runtime never announces which branch of mathematics it is invoking; these documents name it, state the formal object, and pin each one to the runtime concept whose correctness it underwrites.

| Document | What it grounds |
|----------|-----------------|
| [./graph-theory.md](./graph-theory.md) | The netlist and copper topology as graphs — connectivity (components/opens), Steiner/spanning trees for minimal copper, shortest-path routing, coloring/thickness for layer count, and cut-sets for plane splits. |
| [./optimization-theory.md](./optimization-theory.md) | Placement and routing as constrained minimization — objectives (wirelength, congestion, thermal, EMI) versus hard bounds, and the Pareto trade-offs an engineer approves. |
| [./constraint-satisfaction.md](./constraint-satisfaction.md) | The DRC/ERC/DFM rule apparatus as a finite-domain CSP — decidable, terminating checking and search that make the manufacturing gate a deterministic, reproducible function. |
| [./computational-geometry.md](./computational-geometry.md) | The physical-domain verdicts — clearance, containment, intersection, offset, and pour as set operations on planar primitives held on a sub-micron integer grid. |
| [./linear-algebra.md](./linear-algebra.md) | The matrix behind circuit and field analysis — Modified Nodal Analysis `Gv = i` operating points, eigenvalue resonance/ringing, and affine transforms for footprint placement. |
| [./numerical-methods.md](./numerical-methods.md) | Turning continuous physics (fields, heat, device curves) into finite arithmetic — discretize, solve, interpret with a quantified error budget, so a missing or unstable solve is *indeterminate*, never a pass. |
| [./probability-and-statistics.md](./probability-and-statistics.md) | How parameter uncertainty propagates and aggregates into **yield** — tolerance propagation (worst-case vs. statistical), DFM process-capability, and learning-engine confidence. |
| [./search-algorithms.md](./search-algorithms.md) | Placement and routing as state-space search — maze/A\* routing, branch-and-bound/beam placement, rip-up-and-retry backtracking, and the completeness/admissibility guarantees the runtime leans on. |
| [./decision-theory.md](./decision-theory.md) | Gating under uncertainty with asymmetric cost — utility, expected loss, risk thresholds, and autonomy-level human-in-the-loop escalation: *who decides, when, and at what risk*. |
| [./control-theory.md](./control-theory.md) | The verify→fix loop-backs (DRC↺Routing, EMC↺Routing, DFM↺Placement) as closed-loop feedback — convergence, oscillation, damping, and bounded iteration toward a violation-free fixed point. |

---

Up: [../README.md](../README.md) (Engineering Science Layer index) · Across: [../runtime-mapping/concept-runtime-crosswalk.md](../runtime-mapping/concept-runtime-crosswalk.md) (which runtime concept each result of this mathematics grounds).
