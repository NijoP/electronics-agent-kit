# Industry — Professional EDA Methodology

This folder captures the **vendor-neutral professional methodology** distilled from mature electronic-design-automation practice — the reusable *process discipline* behind placement, routing, constraint systems, and the manufacturing hand-off, plus the human engineering loop the EAK runtime mechanizes. It carries **no proprietary implementations**: each document names the common-denominator method that every serious tool (Allegro, Altium, KiCad) and every senior engineer converges on, and shows where the EAK runtime silently re-implements it. Where the sibling science folders supply the *physics, mathematics, and algorithms*, this folder supplies the *engineering judgement and workflow* that sequences and governs them.

| Document | What it grounds |
|----------|-----------------|
| [./constraint-systems.md](./constraint-systems.md) | The architectural pattern for encoding design rules as declarative, group-scoped, inheritance-resolved data evaluated by an engine separate from the canvas — grounds EAK's Constraint Engine, Verification Engine, and IR/UI separation. |
| [./placement-philosophy.md](./placement-philosophy.md) | The reusable method that reliably reaches a good placement — rooms/clusters, manual-critical-then-auto, lock-and-iterate, intent capture — grounding the Floor-Planning and Component-Placement state machines and the propose/dispose seam. |
| [./routing-philosophy.md](./routing-philosophy.md) | The five-stage routing loop — plan, route most-constrained first, interactive-vs-auto, tune-then-lock, verify — grounding the runtime's net ordering, autonomy gate, and checkpoint/lock semantics. |
| [./manufacturing-methodology.md](./manufacturing-methodology.md) | The vendor-neutral hand-off discipline — ship a neutral self-describing data set, re-check against the fab's own rules, encode drawing intent, source rules from fab capability — grounding the Manufacturing Generation phase and Manufacturing IR. |
| [./human-workflow.md](./human-workflow.md) | The whole-loop process governance — capture→plan→place→route→verify→revise, design-review gates, accountable sign-off, automate-vs-decide boundary — grounding the Workflow Orchestrator's phase DAG, gates, loop-backs, and autonomy levels. |

---

Up to the layer root index: [../README.md](../README.md) · Across to the concept→runtime crosswalk: [../runtime-mapping/concept-runtime-crosswalk.md](../runtime-mapping/concept-runtime-crosswalk.md).
