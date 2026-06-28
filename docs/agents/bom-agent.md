# BOM Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The BOM Agent drives the **[BOM Planning](../state-machines/bom-planning.md)** phase: it selects orderable [Parts](../foundation/engineering-domain-model.md#part-manufacturer-part) for the design's [Components](../foundation/engineering-domain-model.md#component), resolves sourcing (price, availability, lead time, alternates, lifecycle), and produces the [Bill of Materials](../foundation/engineering-domain-model.md#bom-line-item). It exists because part selection is a constraint-satisfaction and supply-chain problem that must be traceable and re-checkable, not a one-off lookup.

This doc follows the [Agent family template](../CONVENTIONS.md) and owns the agent's internals; states/transitions/persistence belong to the [bom-planning state machine](../state-machines/bom-planning.md) ([anti-duplication rule](README.md#anti-duplication-rule--agents-vs-state-machines)).

## Purpose

For each [Component](../foundation/engineering-domain-model.md#component), choose a [Part](../foundation/engineering-domain-model.md#part-manufacturer-part) (MPN) that satisfies its electrical parameters, the project's [Constraints](../foundation/engineering-domain-model.md#constraint) (cost, compliance), and supply-chain reality (availability, lifecycle, lead time), and assemble [BOM Line Items](../foundation/engineering-domain-model.md#bom-line-item) with alternates. Every selection is a justified [Decision](../foundation/engineering-domain-model.md#decision) ([P5](../foundation/principles.md)).

## Responsibilities

- **Select Parts** whose datasheet facts (from [Datasheet Intelligence](../state-machines/datasheet-intelligence.md)) match each Component's required parameters.
- **Check sourcing** — price, stock, lead time, lifecycle (active/NRND/EOL) via the [Parts-data port](../core/contracts.md#parts-data-port).
- **Enforce constraints** — cost ceilings, RoHS/REACH, preferred-vendor and approved-vendor rules via the [Constraint Engine](../engineering/constraint-engine.md).
- **Propose alternates** for risk (single-source, EOL) and consolidate line items across identical Components.
- **Maintain traceability** — each BOM Line Item links to its Components, the selection Decision, and the part [Evidence](../foundation/engineering-domain-model.md#evidence).
- **Not** extract datasheet facts ([Datasheet Agent](datasheet-agent.md)) or create Components ([Schematic Agent](schematic-agent.md)); this agent *binds Components to real Parts*.

## Inputs

- The [Component](../foundation/engineering-domain-model.md#component) set with required parameters (from the schematic domain).
- Part facts from the [Knowledge Graph](../knowledge/knowledge-graph.md) (via [Datasheet Intelligence](../state-machines/datasheet-intelligence.md)).
- Sourcing data from the [Parts-data port](../core/contracts.md#parts-data-port).
- Cost/compliance/vendor [Constraints](../foundation/engineering-domain-model.md#constraint) from the [Constraint Engine](../engineering/constraint-engine.md).

## Outputs

- [BOM Line Items](../foundation/engineering-domain-model.md#bom-line-item) committed to [Engineering State](../core/shared-state-model.md), each with chosen Part, quantity, using-Components, alternates, and sourcing snapshot.
- The [BOM IR](../compiler/ir/bom-ir.md) at the phase boundary.
- Sourcing-risk flags (single-source, long lead time, EOL) surfaced for human disposition.

## State

The agent's own working state per activation: the Components-to-source work queue, candidate Part rankings per Component, the running cost roll-up against the cost Constraint, unresolved sourcing risks, and budget remaining. Ephemeral; the BOM lives in [Engineering State](../core/shared-state-model.md), not the agent.

## Events

- **Emits:** `Part selected`, `BOM line item created/consolidated`, `Alternate proposed`, `Sourcing risk flagged`, `Cost constraint breached` — [Events](../core/event-bus.md) with justifying [Decisions](../foundation/engineering-domain-model.md#decision).
- **Consumes:** component-set-finalized events; sourcing-data-refreshed events that re-trigger re-selection.

> Phase-advancement events belong to the [bom-planning state machine](../state-machines/bom-planning.md).

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md), [Reasoning Engine port](../core/reasoning-engine-interface.md), [Parts-data port](../core/contracts.md#parts-data-port), [Knowledge port](../knowledge/knowledge-graph.md), [State Repository](../core/contracts.md#state-repository), [Cost-budget](../core/contracts.md#cross-cutting-contracts) & [Security/Policy](../core/contracts.md#cross-cutting-contracts) ports.
- **Engines:** [Constraint Engine](../engineering/constraint-engine.md) (cost/compliance/vendor constraints).
- **Driven by:** the [Execution Engine](../core/execution-engine.md) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md).

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **No part satisfies parameters + constraints** | Component unsourceable. | Relax-constraint proposals or escalate to renegotiate the Constraint/Requirement ([P10](../foundation/principles.md)); never pick a non-conforming Part. |
| **Cost ceiling exceeded** | Budget breach. | [Constraint Engine](../engineering/constraint-engine.md) flags; agent proposes cheaper alternates or escalates; no silent overrun ([P13](../foundation/principles.md)). |
| **Stale sourcing data** | Wrong availability/price. | Sourcing snapshot is timestamped [Evidence](../foundation/engineering-domain-model.md#evidence); refresh re-triggers selection; replay uses recorded data ([P4](../foundation/principles.md)). |
| **EOL / single-source risk** | Supply risk. | Risk flagged; alternates proposed; disposition recorded. |
| **Parts-data unavailable** | No sourcing. | Recoverable failure; phase pauses/retries; selection on cached facts where possible. |

## Future improvements

- Multi-objective optimization across cost/availability/risk surfaced as Pareto options.
- Learned preferred-part lists per part family from the [Learning Engine](../engineering/learning-engine.md).
- Proactive lifecycle monitoring that re-opens BOM Planning when a chosen Part goes NRND/EOL.

## Two-part split (P8)

| Half | In the BOM Agent |
|------|-------------------|
| **Deterministic engineering use-case** | Reads Components + part facts; queries the [Parts-data port](../core/contracts.md#parts-data-port); checks candidates against parameters and [Constraints](../engineering/constraint-engine.md); rolls up cost; validates each selection (parameter match, compliance, sourcing present); proposes `Part selected` / `BOM line item` capability invocations with justifying [Decisions](../foundation/engineering-domain-model.md#decision). |
| **Reasoning adapter** | Given candidate Parts, their facts, and sourcing, with a strict output schema ("ranked part choices with rationale, trade-off, confidence"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) to rank/justify selections and propose alternates. Candidates only. |
| **The seam** | The use-case re-checks every ranked candidate against hard Constraints (parameters, cost, compliance) via the [Constraint Engine](../engineering/constraint-engine.md) before committing — a model preference that violates a Constraint is blocked ([P3](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phase / state machine:** [BOM Planning](../state-machines/bom-planning.md) — owns states/transitions/events/rollback/recovery/persistence. This agent drives it.
- **Engines used:** [Constraint Engine](../engineering/constraint-engine.md).
- **Primary IR produced:** [BOM IR](../compiler/ir/bom-ir.md).

## Related documents

[`agents/README.md`](README.md) · [`state-machines/bom-planning.md`](../state-machines/bom-planning.md) · [`engineering/constraint-engine.md`](../engineering/constraint-engine.md) · [`compiler/ir/bom-ir.md`](../compiler/ir/bom-ir.md) · [`agents/datasheet-agent.md`](datasheet-agent.md) · [`agents/schematic-agent.md`](schematic-agent.md) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md#bom-line-item)
