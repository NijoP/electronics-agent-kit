# Schematic Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The Schematic Agent drives the **[Schematic Planning](../state-machines/schematic-planning.md)** phase: it realizes the [Functional Blocks](../foundation/engineering-domain-model.md#functional-block) and architecture from [Engineering Analysis](../state-machines/engineering-analysis.md) as a concrete logical design — instantiating [Components](../foundation/engineering-domain-model.md#component), their [Pins](../foundation/engineering-domain-model.md#pin), [Connections](../foundation/engineering-domain-model.md#connection), and [Nets](../foundation/engineering-domain-model.md#net). It exists to turn an engineering approach into a connected, [ERC](../state-machines/erc-verification.md)-checkable schematic while keeping every choice traceable.

This doc follows the [Agent family template](../CONVENTIONS.md) and owns the agent's internals; states/transitions/persistence belong to the [schematic-planning state machine](../state-machines/schematic-planning.md) ([anti-duplication rule](README.md#anti-duplication-rule--agents-vs-state-machines)).

## Purpose

Produce the logical design: instantiate the [Components](../foundation/engineering-domain-model.md#component) each [Functional Block](../foundation/engineering-domain-model.md#functional-block) needs (with [Symbols](../foundation/engineering-domain-model.md#symbol)), define their interconnections as [Connections](../foundation/engineering-domain-model.md#connection)/[Nets](../foundation/engineering-domain-model.md#net) with net classes, and apply standard sub-circuit patterns (decoupling, pull-ups, protection) — all consistent with the project [Constraints](../foundation/engineering-domain-model.md#constraint).

## Responsibilities

- **Instantiate Components** for each Functional Block and assign their [Symbols](../foundation/engineering-domain-model.md#symbol) from the [Component Library](../engineering/component-library.md).
- **Capture connectivity** — define [Connections](../foundation/engineering-domain-model.md#connection), aggregate into [Nets](../foundation/engineering-domain-model.md#net), and assign net classes (power/ground/signal/differential/high-speed).
- **Apply known-good patterns** — decoupling caps, pull-ups/downs, ESD/protection, reference circuits — guided by prior art from the [Learning Engine](../engineering/learning-engine.md).
- **Respect constraints** — voltage-domain partitioning, net-class electrical targets via the [Constraint Engine](../engineering/constraint-engine.md).
- **Sequence its work** via the [Planning Engine](../engineering/planning-engine.md) (block-by-block [Reasoning plan](../GLOSSARY.md#the-word-planning-disambiguation)).
- **Not** run [ERC](erc-agent.md) (that is its own phase) or choose MPNs ([BOM Agent](bom-agent.md)); this agent *captures the logical design*.

## Inputs

- [Engineering IR](../compiler/ir/engineering-ir.md): Functional Blocks, architecture Decision, and Constraints.
- [Component Library](../engineering/component-library.md) symbols and known-good sub-circuit patterns.
- Prior reference designs from [Vector Memory](../knowledge/vector-memory.md) / the [Learning Engine](../engineering/learning-engine.md).

## Outputs

- [Components](../foundation/engineering-domain-model.md#component), [Pins](../foundation/engineering-domain-model.md#pin), [Connections](../foundation/engineering-domain-model.md#connection), and [Nets](../foundation/engineering-domain-model.md#net) committed to [Engineering State](../core/shared-state-model.md), each justified by a [Decision](../foundation/engineering-domain-model.md#decision).
- The [Schematic IR](../compiler/ir/schematic-ir.md) at the phase boundary.
- Open design questions (topology variants, unresolved nets) surfaced for human disposition.

## State

The agent's own working state per activation: the active [Reasoning plan](../GLOSSARY.md#the-word-planning-disambiguation) (which block is being captured), the working set of uncommitted Components/Nets, applied-pattern bookkeeping, and budget remaining. Ephemeral; the schematic lives in [Engineering State](../core/shared-state-model.md), not the agent.

## Events

- **Emits:** `Component instantiated`, `Net created/classified`, `Connection made`, `Pattern applied`, `Design question raised` — [Events](../core/event-bus.md) with justifying [Decisions](../foundation/engineering-domain-model.md#decision).
- **Consumes:** engineering-analysis-finalized events (phase entry); [ERC](../state-machines/erc-verification.md)-failure events that loop back to Schematic Planning (per the [default workflow plan](../foundation/architecture-views.md)).

> Phase-advancement events belong to the [schematic-planning state machine](../state-machines/schematic-planning.md).

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md), [Reasoning Engine port](../core/reasoning-engine-interface.md), [State Repository](../core/contracts.md#state-repository), [Knowledge](../knowledge/knowledge-graph.md) & [Vector Memory](../knowledge/vector-memory.md) ports, [Cost-budget](../core/contracts.md#cross-cutting-contracts) & [Security/Policy](../core/contracts.md#cross-cutting-contracts) ports.
- **Engines:** [Planning Engine](../engineering/planning-engine.md) (sequences capture), [Constraint Engine](../engineering/constraint-engine.md) (net-class/voltage-domain constraints).
- **Driven by:** the [Execution Engine](../core/execution-engine.md) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md).

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **Incomplete connectivity** (floating pins, missing decoupling) | ERC failures downstream. | Completeness checks before the gate; flagged for human review; ERC loop catches residuals. |
| **Constraint violation** (wrong voltage domain) | Electrically unsound design. | [Constraint Engine](../engineering/constraint-engine.md) validation at the seam blocks the commit. |
| **Reasoning invalid/unavailable** | No usable topology. | Validate/repair at the seam; persistent → *failed*; FSM routes recovery (often back from an ERC loop). |
| **Pattern misapplication** | Wrong sub-circuit. | Patterns are [Evidence](../foundation/engineering-domain-model.md#evidence)-backed proposals validated against Constraints; engineer disposes ([P10](../foundation/principles.md)). |

## Future improvements

- A growing library of validated reference sub-circuits keyed by Functional Block type from the [Learning Engine](../engineering/learning-engine.md).
- Automatic decoupling/protection insertion with rationale.
- Schematic-diff-aware re-capture when upstream Constraints change.

## Two-part split (P8)

| Half | In the Schematic Agent |
|------|-------------------------|
| **Deterministic engineering use-case** | Reads Functional Blocks + Constraints; runs the [Planning Engine](../engineering/planning-engine.md) to sequence capture; instantiates Components and builds Nets via the Capability port; validates connectivity and net-class/voltage-domain rules via the [Constraint Engine](../engineering/constraint-engine.md); proposes commits with justifying [Decisions](../foundation/engineering-domain-model.md#decision). |
| **Reasoning adapter** | Given a Functional Block, its Constraints, and prior art, with a strict output schema ("components to instantiate; connections; net classes; applied patterns + rationale"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) for a candidate sub-circuit. Candidates only. |
| **The seam** | The use-case validates each candidate sub-circuit (connectivity completeness, constraint conformance) before committing; an unsound topology never reaches state ([P3](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phase / state machine:** [Schematic Planning](../state-machines/schematic-planning.md) — owns states/transitions/events/rollback/recovery/persistence; loops back from [ERC Verification](../state-machines/erc-verification.md) on failure. This agent drives it.
- **Engines used:** [Planning Engine](../engineering/planning-engine.md), [Constraint Engine](../engineering/constraint-engine.md).
- **Primary IR produced:** [Schematic IR](../compiler/ir/schematic-ir.md).

## Related documents

[`agents/README.md`](README.md) · [`state-machines/schematic-planning.md`](../state-machines/schematic-planning.md) · [`state-machines/erc-verification.md`](../state-machines/erc-verification.md) · [`engineering/planning-engine.md`](../engineering/planning-engine.md) · [`engineering/constraint-engine.md`](../engineering/constraint-engine.md) · [`engineering/component-library.md`](../engineering/component-library.md) · [`compiler/ir/schematic-ir.md`](../compiler/ir/schematic-ir.md) · [`agents/erc-agent.md`](erc-agent.md)
