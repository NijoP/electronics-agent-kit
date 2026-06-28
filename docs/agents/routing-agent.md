# Routing Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The Routing Agent drives the **[Routing Planning](../state-machines/routing-planning.md)** phase: it physically realizes [Nets](../foundation/engineering-domain-model.md#net) as [Tracks](../foundation/engineering-domain-model.md#track--routing) on the placed [Board](../foundation/engineering-domain-model.md#board--layer-stack) — assigning layers, widths, vias, and differential-pair geometry — in a [DRC](../state-machines/drc-verification.md)-aware way. It exists because routing is where logical connectivity becomes manufacturable copper, and every track must honor electrical and physical [Constraints](../foundation/engineering-domain-model.md#constraint) while remaining traceable.

This doc follows the [Agent family template](../CONVENTIONS.md) and owns the agent's internals; states/transitions/persistence belong to the [routing-planning state machine](../state-machines/routing-planning.md) ([anti-duplication rule](README.md#anti-duplication-rule--agents-vs-state-machines)).

## Purpose

Convert each [Net](../foundation/engineering-domain-model.md#net) into [Track](../foundation/engineering-domain-model.md#track--routing) geometry that electrically realizes exactly that net's [Connections](../foundation/engineering-domain-model.md#connection) — choosing layers, trace widths, via placement, and matched geometry for differential pairs / high-speed nets — such that the result satisfies the [Constraint Engine](../engineering/constraint-engine.md)'s rules and passes [DRC](../state-machines/drc-verification.md).

## Responsibilities

- **Assign layers** and order routing by net class/criticality (a [Reasoning plan](../GLOSSARY.md#the-word-planning-disambiguation) via the [Planning Engine](../engineering/planning-engine.md)).
- **Route tracks** realizing each Net's connectivity, with widths driven by current/impedance Constraints (typed [Physical Quantities](../engineering/units-and-quantities.md)).
- **Handle differential pairs & high-speed nets** — coupling, length matching, spacing per net-class targets.
- **Place vias** and manage layer transitions within stack-up rules.
- **Stay DRC-aware** — check clearance/width/via Constraints continuously so most violations never form.
- **Maintain the invariant** that a Net's Tracks realize exactly its Connections — no more, no less.
- **Not** place Components ([Placement Agent](placement-agent.md)) or render the final [DRC](drc-agent.md) verdict; this agent *creates the copper*.

## Inputs

- The placed [PCB IR](../compiler/ir/pcb-ir.md): [Placements](../foundation/engineering-domain-model.md#placement), [Board](../foundation/engineering-domain-model.md#board--layer-stack)/layer stack, [Nets](../foundation/engineering-domain-model.md#net) and net classes.
- Routing/electrical [Constraints](../foundation/engineering-domain-model.md#constraint) (clearance, width, impedance, length-match) from the [Constraint Engine](../engineering/constraint-engine.md).
- Prior routing strategies from the [Learning Engine](../engineering/learning-engine.md).

## Outputs

- [Track](../foundation/engineering-domain-model.md#track--routing) geometry (segments, arcs, vias, diff-pair partners) committed to [Engineering State](../core/shared-state-model.md), each justified by a [Decision](../foundation/engineering-domain-model.md#decision); enriches the [PCB IR](../compiler/ir/pcb-ir.md).
- Routing-completion status (fully/partially routed) and unroutable-net flags surfaced for human disposition.

## State

The agent's own working state per activation: the active [Reasoning plan](../GLOSSARY.md#the-word-planning-disambiguation) (net ordering, layer assignment strategy), the in-progress track set not yet committed, per-net completion status, running DRC-awareness checks, and budget remaining. Ephemeral; routing lives in [Engineering State](../core/shared-state-model.md), not the agent.

## Events

- **Emits:** `Layer assigned`, `Net routed`, `Via placed`, `Differential pair matched`, `Unroutable net flagged` — [Events](../core/event-bus.md) with justifying [Decisions](../foundation/engineering-domain-model.md#decision).
- **Consumes:** placement-finalized events (phase entry); [DRC](../state-machines/drc-verification.md)- and [EMC](../state-machines/emc-analysis.md)-failure events that loop back to Routing (per the [default workflow plan](../foundation/architecture-views.md)).

> Phase-advancement events belong to the [routing-planning state machine](../state-machines/routing-planning.md).

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md), [Reasoning Engine port](../core/reasoning-engine-interface.md), [State Repository](../core/contracts.md#state-repository), [Knowledge](../knowledge/knowledge-graph.md) & [Vector Memory](../knowledge/vector-memory.md) ports, [Cost-budget](../core/contracts.md#cross-cutting-contracts) & [Security/Policy](../core/contracts.md#cross-cutting-contracts) ports.
- **Engines:** [Constraint Engine](../engineering/constraint-engine.md) (clearance/width/impedance/length rules), [Planning Engine](../engineering/planning-engine.md) (routing order/strategy).
- **Driven by:** the [Execution Engine](../core/execution-engine.md) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md).

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **Unroutable net** (congestion) | Incomplete routing. | Flag the net; propose re-placement loop-back to the [Placement Agent](placement-agent.md) or added layers; never force a DRC-violating track. |
| **Constraint violation** (clearance/width) | Illegal copper. | DRC-aware checks + [Constraint Engine](../engineering/constraint-engine.md) validation at the seam block the commit. |
| **Length-match miss** on diff pair/high-speed | Signal-integrity risk. | Length-match Constraint checked per-net; misses flagged and re-reasoned or escalated. |
| **Reasoning invalid/unavailable** | No strategy proposal. | Validate/repair at the seam; persistent → *failed*; FSM routes recovery (often loop-back). |

## Future improvements

- In-loop signal-integrity estimation (via the [Simulation port](../core/contracts.md#simulation-port)) to score routes before commit.
- Learned routing templates for recurring net classes from the [Learning Engine](../engineering/learning-engine.md).
- Co-optimization with the [Placement Agent](placement-agent.md) to break congestion automatically.

## Two-part split (P8)

| Half | In the Routing Agent |
|------|-----------------------|
| **Deterministic engineering use-case** | Reads placements, stack-up, nets, and Constraints; runs the [Planning Engine](../engineering/planning-engine.md) to order nets and assign layers; applies/validates candidate track geometry against clearance/width/impedance/length-match rules via the [Constraint Engine](../engineering/constraint-engine.md) (DRC-aware); enforces the net-realization invariant; proposes `net routed` / `via placed` capability invocations with justifying [Decisions](../foundation/engineering-domain-model.md#decision). |
| **Reasoning adapter** | Given the board, congestion picture, net classes, and prior art, with a strict output schema ("routing strategy: layer assignment, net order, diff-pair handling + rationale"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) to *propose routing strategies*. Candidates only. |
| **The seam** | The reasoning half proposes strategies; the deterministic half **applies and validates** them as concrete geometry against the Constraint Engine before any track commits — a strategy that yields illegal copper is blocked ([P3](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phase / state machine:** [Routing Planning](../state-machines/routing-planning.md) — owns states/transitions/events/rollback/recovery/persistence; loops back from [DRC Verification](../state-machines/drc-verification.md) and [EMC Analysis](../state-machines/emc-analysis.md) on failure. This agent drives it.
- **Engines used:** [Constraint Engine](../engineering/constraint-engine.md), [Planning Engine](../engineering/planning-engine.md).
- **Primary IR:** enriches the [PCB IR](../compiler/ir/pcb-ir.md).

## Related documents

[`agents/README.md`](README.md) · [`state-machines/routing-planning.md`](../state-machines/routing-planning.md) · [`state-machines/drc-verification.md`](../state-machines/drc-verification.md) · [`engineering/constraint-engine.md`](../engineering/constraint-engine.md) · [`engineering/planning-engine.md`](../engineering/planning-engine.md) · [`compiler/ir/pcb-ir.md`](../compiler/ir/pcb-ir.md) · [`agents/placement-agent.md`](placement-agent.md) · [`agents/drc-agent.md`](drc-agent.md)
