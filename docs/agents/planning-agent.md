# Planning Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The Planning Agent is a **dual-phase** agent: it drives both **[Engineering Analysis](../state-machines/engineering-analysis.md)** (turning [Requirements](../foundation/engineering-domain-model.md#requirement) into a feasible engineering approach — topology, [Functional Blocks](../foundation/engineering-domain-model.md#functional-block), trade-offs) and **[Constraint Extraction](../state-machines/constraint-extraction.md)** (projecting requirements, standards, and parts facts into machine-checkable [Constraints](../foundation/engineering-domain-model.md#constraint)). It exists because the bridge from *intent* to *enforceable engineering* is a single coherent reasoning effort best kept in one agent across two adjacent phases.

This doc follows the [Agent family template](../CONVENTIONS.md) and owns the agent's internals only. The states/transitions/persistence of each phase belong to their respective state machines ([anti-duplication rule](README.md#anti-duplication-rule--agents-vs-state-machines)). "Planning" here always means the [Reasoning plan](../GLOSSARY.md#the-word-planning-disambiguation) and the *Engineering Analysis* phase — never the bare word.

## Purpose

Decide *how* the design will meet its [Requirements](../foundation/engineering-domain-model.md#requirement) (architecture/topology, functional decomposition, key trade-offs), and then make that decision **enforceable** by deriving the [Constraints](../foundation/engineering-domain-model.md#constraint) every later phase must respect. A Requirement is intent; a Constraint is its enforceable projection — this agent produces both halves of that bridge.

## Responsibilities

**Engineering Analysis phase:**
- Propose a system architecture and decompose it into [Functional Blocks](../foundation/engineering-domain-model.md#functional-block) (MCU subsystem, buck regulator, RF front-end…).
- Evaluate topology/approach trade-offs (cost vs. size vs. power vs. risk) and record the chosen approach as a [Decision](../foundation/engineering-domain-model.md#decision) with rationale.
- Assess feasibility against the Requirement set; flag infeasible requirement combinations back for renegotiation.

**Constraint Extraction phase:**
- Derive [Constraints](../foundation/engineering-domain-model.md#constraint) (clearance, voltage/current limits, impedance targets, thermal limits, keep-outs, compliance rules) from Requirements, [standards](../engineering/standards-and-compliance.md), and chosen approach.
- Register and de-conflict them through the [Constraint Engine](../engineering/constraint-engine.md), each as a typed [Physical Quantity](../engineering/units-and-quantities.md) bound where applicable.
- Maintain traceability: every Constraint links to the Requirement/standard it projects ([P5](../foundation/principles.md)).

## Inputs

- The [Requirement IR](../compiler/ir/requirement-ir.md) / [Requirement](../foundation/engineering-domain-model.md#requirement) set from the [Requirement Agent](requirement-agent.md).
- Applicable [standards and compliance](../engineering/standards-and-compliance.md) clauses.
- Prior-art topologies and default Constraint profiles from the [Learning Engine](../engineering/learning-engine.md) (as [Evidence](../foundation/engineering-domain-model.md#evidence)).

## Outputs

- [Functional Blocks](../foundation/engineering-domain-model.md#functional-block) and the recorded architecture [Decision](../foundation/engineering-domain-model.md#decision) (Engineering Analysis).
- A registered, de-conflicted set of [Constraints](../foundation/engineering-domain-model.md#constraint) in the [Constraint Engine](../engineering/constraint-engine.md) (Constraint Extraction).
- The [Engineering IR](../compiler/ir/engineering-ir.md): Engineering Analysis transforms Requirement IR → Engineering IR; Constraint Extraction enriches it.

## State

The agent's own working state per activation: the active [Reasoning plan](../GLOSSARY.md#the-word-planning-disambiguation); the candidate architecture/block decomposition under evaluation; the working set of draft Constraints pending de-confliction; which of the two phases is active and its goal; budget remaining. Ephemeral — durable knowledge lives in [Engineering State](../core/shared-state-model.md) and the [Constraint Engine](../engineering/constraint-engine.md), never in the agent.

## Events

- **Emits:** `Functional block created`, `Architecture decision recorded`, `Constraint derived/registered`, `Constraint conflict detected`, `Feasibility concern raised` — each an [Event](../core/event-bus.md) with its [Decision](../foundation/engineering-domain-model.md#decision).
- **Consumes:** Requirement-set-finalized events (entry to Engineering Analysis) and standards-updated events that re-trigger extraction.

> Phase-advancement events are owned by the [engineering-analysis](../state-machines/engineering-analysis.md) and [constraint-extraction](../state-machines/constraint-extraction.md) state machines.

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md), [Reasoning Engine port](../core/reasoning-engine-interface.md), [State Repository](../core/contracts.md#state-repository), [Knowledge](../knowledge/knowledge-graph.md) & [Vector Memory](../knowledge/vector-memory.md) ports, [Cost-budget](../core/contracts.md#cross-cutting-contracts) & [Security/Policy](../core/contracts.md#cross-cutting-contracts) ports.
- **Engines:** [Planning Engine](../engineering/planning-engine.md) (sequences analysis), [Constraint Engine](../engineering/constraint-engine.md) (stores/resolves/checks Constraints).
- **Driven by:** the [Execution Engine](../core/execution-engine.md) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md).

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **Infeasible requirement set** | No viable architecture. | Raise feasibility concern; return *needs-human* to renegotiate Requirements ([P10](../foundation/principles.md)). |
| **Constraint conflict** (e.g. clearance vs. size) | Over-constrained design. | [Constraint Engine](../engineering/constraint-engine.md) detects; agent re-reasons trade-off or escalates; never silently drops a Constraint. |
| **Reasoning invalid/unavailable** | No usable approach/Constraints. | Validate/repair at the seam; persistent → *failed*; FSM routes recovery. |
| **Under-derivation** (missed Constraint) | Latent violations later. | Coverage check against Requirements + standards; gaps flagged before the phase gate. |

## Future improvements

- A reusable library of topology patterns and standard-derived Constraint packs from the [Learning Engine](../engineering/learning-engine.md).
- Quantitative trade-off scoring (Pareto fronts over cost/size/power) surfaced to the engineer.
- Automatic re-extraction of Constraints when an upstream Requirement changes, with impact diffing.

## Two-part split (P8)

| Half | In the Planning Agent |
|------|------------------------|
| **Deterministic engineering use-case** | Reads the Requirement set; runs the [Planning Engine](../engineering/planning-engine.md) to sequence the analysis; evaluates candidate architectures against feasibility; registers and de-conflicts Constraints via the [Constraint Engine](../engineering/constraint-engine.md); validates every proposed Functional Block and Constraint against [domain invariants](../foundation/engineering-domain-model.md) and unit/quantity typing; proposes capability invocations with justifying [Decisions](../foundation/engineering-domain-model.md#decision). |
| **Reasoning adapter** | Given Requirements, standards, and prior art with strict output schemas ("candidate architecture + functional decomposition + rationale"; "candidate constraints: type, scope, bound, severity, source"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) for topology proposals and Constraint derivations. Candidates only. |
| **The seam** | The use-case validates feasibility and Constraint well-formedness (typed bounds, valid scope, no contradiction) via the [Constraint Engine](../engineering/constraint-engine.md) before committing. A plausible-but-contradictory Constraint is blocked here ([P3](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phases / state machines:** [Engineering Analysis](../state-machines/engineering-analysis.md) and [Constraint Extraction](../state-machines/constraint-extraction.md) — each owns its own states/transitions/events/rollback/recovery/persistence. This agent drives both, sequentially per the [default workflow plan](../foundation/architecture-views.md).
- **Engines used:** [Planning Engine](../engineering/planning-engine.md) (Engineering Analysis), [Constraint Engine](../engineering/constraint-engine.md) (both phases).
- **Primary IR:** transforms [Requirement IR](../compiler/ir/requirement-ir.md) → [Engineering IR](../compiler/ir/engineering-ir.md) (Analysis) and enriches it (Extraction).

## Related documents

[`agents/README.md`](README.md) · [`state-machines/engineering-analysis.md`](../state-machines/engineering-analysis.md) · [`state-machines/constraint-extraction.md`](../state-machines/constraint-extraction.md) · [`engineering/planning-engine.md`](../engineering/planning-engine.md) · [`engineering/constraint-engine.md`](../engineering/constraint-engine.md) · [`compiler/ir/engineering-ir.md`](../compiler/ir/engineering-ir.md) · [`agents/requirement-agent.md`](requirement-agent.md) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md#constraint)
