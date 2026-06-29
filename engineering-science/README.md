# Engineering Science Layer

> **Phase 0.5 — Engineering Science.** This layer is documentation only: no code, no technology selection, no implementation. It is the **theoretical "why"** beneath the Electronics Agent Kit — the mathematics, physics, electrical engineering, PCB engineering, and manufacturing science that the runtime, compiler IRs, state machines, constraint engine, verification rules, and learning loop silently assume. It sits beside the [`docs/`](../docs/README.md) architecture (the *what/how*) and the implemented [`eak/`](../eak) workspace (the *built*), and binds them: every doc here ends by mapping its principle to a **concrete runtime artifact**, so the science is load-bearing, not decoration.

Electronics Agent Kit is an **AI-native Engineering Runtime** for PCB and electronics design — a deterministic kernel that orchestrates engineering state machines over a single versioned engineering state, using LLMs only as reasoning engines behind a strict boundary. A runtime that places a component, sizes a trace, checks a clearance, or releases a board is making an **engineering-science claim**. This layer states those claims explicitly, so they can be taught, checked, and (where the audit found gaps) honestly scoped.

## How this layer is organized

The directory layout runs from the most abstract substrate to the most concrete reality, then binds the lot to the runtime and audits the result:

| Folder | Role | Docs |
|--------|------|------|
| [`mathematics/`](mathematics/README.md) | The formal/algorithmic substrate the runtime computes with — graphs, optimization, CSP, geometry, linear algebra, numerics, statistics, search, decision & control theory. | 10 |
| [`physics/`](physics/README.md) | First-principles physics the board obeys — fields, Maxwell, heat transfer, materials, semiconductor devices, RF. | 6 |
| [`electrical/`](electrical/README.md) | Circuit- and signal-level laws bridging physics to the schematic and PCB — circuit theory, Ohm, Kirchhoff, transmission lines, signal & power integrity. | 6 |
| [`pcb/`](pcb/README.md) | Layout engineering principles — placement, routing, planes, return path, differential pairs, stackup, high-speed, analog, EMI/EMC. | 10 |
| [`manufacturing/`](manufacturing/README.md) | Fabrication & assembly reality — manufacturing constraints (DFM), IPC standards, yield-driven design. | 3 |
| [`industry/`](industry/README.md) | Vendor-neutral professional methodology distilled from mature EDA practice (no proprietary implementations). | 5 |
| [`runtime-mapping/`](runtime-mapping/README.md) | **The binding crosswalk** — every concept above traced to a runtime engine, compiler IR, state-machine phase, constraint/verification rule, and learning hook. | 7 |
| [`compliance/`](compliance/README.md) | The architecture-review verdict: does `docs/` + `eak/` honor the science? Report, repairs, improvements. | 3 |

If you read nothing else, read these four:

1. [`runtime-mapping/concept-runtime-crosswalk.md`](runtime-mapping/concept-runtime-crosswalk.md) — the single table that shows how theory becomes runtime.
2. [`runtime-mapping/dependency-mapping.md`](runtime-mapping/dependency-mapping.md) — the DAG of science concepts, mirroring the clean-architecture dependency rule.
3. [`compliance/compliance-report.md`](compliance/compliance-report.md) — the honest verdict on how well the implementation honors all of the above.
4. [`physics/maxwell-equations.md`](physics/maxwell-equations.md) and [`electrical/ohms-law.md`](electrical/ohms-law.md) — the two laws most of the runtime's physical checks reduce to.

## Reading convention

Every document opens with a one-paragraph summary, develops the core principles (with equations and Mermaid diagrams), explains why it matters for electronics/PCB design, then — the point of this layer — a **"Mapping to the runtime"** section linking the principle to specific [`docs/…`](../docs/README.md) and [`eak/…`](../eak) artifacts, followed by failure modes and cross-links. Repo-relative links resolve on disk; the science docs link *inward* to the architecture and code, mirroring the dependency rule.

## What the compliance audit found

The [Architecture Review Swarm](compliance/README.md) audited `docs/` and the implemented `eak/` workspace against this layer, with every finding **adversarially verified against the live repository**. Verdict: **`sound-with-gaps`** — 32 confirmed findings, **0 critical · 7 major · 20 minor · 5 info**.

- **The foundation is the science, correctly realized.** Typed physical quantities (dimension-checked comparisons), the constraint/verification engine as a faithful CSP checker, IR projection enforcing real invariants at each phase boundary, a genuinely **bounded** control loop (no hang, fail-closed), and **fab-sourced** manufacturing floors — all verified, all honoring the principles they implement.
- **The major gaps collapse to two roots.** (1) The board's vertical cross-section is a bare `layers: u32` — no copper weight, dielectric, per-layer thickness, plane/reference role — so impedance, thermal/ampacity, and return-path reasoning are *unrepresentable*, not merely unchecked. (2) Two first-order DRC rules are genuinely missing: copper-to-copper clearance (short detection) and net-connectivity (open detection).
- **Highest-leverage repair:** introduce a typed `LayerStack` entity on `Board`/`PcbIr`; it alone unblocks nine downstream findings. See [`compliance/repair-suggestions.md`](compliance/repair-suggestions.md) for the prioritized backlog.

These are recommendations for later phases; the ABSOLUTE RULES of this effort are research/architecture-first, **no implementation**.

## ECC Learning Summary

Reusable intelligence captured by the ECC Learning Swarm (project-specific specs stay in this repo; durable, reusable patterns go to ECC memory):

- **Honesty contract: a mapping doc must resolve to a real symbol.** The most valuable thing the audit caught was *this layer overstating the runtime* — a hallucinated `high-speed`/`trace-floor`/`load-only` "net class" (they are test-fixture reasoner names; the real `NetClass` is `{Power, Ground, Signal}`), wrong rule-definition sites, and present-tense claims of deferred capability. A binding/crosswalk layer earns its name only if every claim is checked against the code. **Phase-6 reconciliation** repaired all of these and added the discipline: *distinguish what the spec defines from what the implementation carries, and link the gap.*
- **Spec-vs-implementation is a first-class distinction.** The `docs/` Phase-0 spec genuinely defines a rich Board/Layer Stack (copper/dielectric/Dk as typed quantities); the implemented `eak-domain::Board` is a reduced subset. "The IR carries copper weight" is true of the spec and false of the code — both facts must be stated, never conflated.
- **The science layer is itself a dependency DAG**, and it points inward to the runtime exactly as the clean-architecture dependency rule requires of the code — Maxwell → return-path → EMI/EMC; Ohm → power-distribution → power-integrity; transmission-lines → high-speed/differential-pairs; CSP → constraint-engine.
- **Multi-swarm orchestration, staged with gates, scales documentation.** This layer was built by sequential Workflow fan-outs — Foundations (40 agents) → Runtime Mapping (14) → Compliance (find → adversarially-verify → synthesize) → Reconciliation — with the orchestrator merging, link-checking, and normalizing parallel-authoring drift between stages.

## Provenance

Authored 2026-06-29 by multi-swarm agent orchestration (Global Orchestrator + specialized swarms with coordinators), per the Phase-0.5 mandate. 59 markdown documents across 8 folders. ABSOLUTE RULES honored: markdown only; no Rust, no TypeScript, no implementation, no placeholders; research, engineering, mathematics, and physics first.

Related: [`../docs/README.md`](../docs/README.md) (the architecture) · [`../docs/foundation/engineering-domain-model.md`](../docs/foundation/engineering-domain-model.md) (canonical vocabulary) · [`../docs/foundation/architecture-views.md`](../docs/foundation/architecture-views.md) (the phase → state-machine → agent → engine map).
