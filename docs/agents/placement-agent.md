# Placement Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The Placement Agent is a **dual-phase** agent: it drives both **[PCB Floor Planning](../state-machines/pcb-floor-planning.md)** (allocating [Board](../foundation/engineering-domain-model.md#board--layer-stack) regions to [Functional Blocks](../foundation/engineering-domain-model.md#functional-block) and defining the layer stack-up) and **[Component Placement](../state-machines/component-placement.md)** (positioning each [Component](../foundation/engineering-domain-model.md#component) — X/Y, rotation, side). It exists because coarse regioning and fine placement are one continuous spatial-reasoning effort best kept in a single agent across two adjacent phases.

This doc follows the [Agent family template](../CONVENTIONS.md) and owns the agent's internals; the states/transitions/persistence of each phase belong to their state machines ([anti-duplication rule](README.md#anti-duplication-rule--agents-vs-state-machines)).

## Purpose

Lay out the physical design spatially: first carve the [Board](../foundation/engineering-domain-model.md#board--layer-stack) into regions per Functional Block and fix the layer stack-up (Floor Planning), then place every [Component](../foundation/engineering-domain-model.md#component) within those regions respecting keep-outs, thermal, signal-flow, and mechanical [Constraints](../foundation/engineering-domain-model.md#constraint) (Placement) — producing a routable, manufacturable arrangement.

## Responsibilities

**PCB Floor Planning phase:**
- Allocate board regions to [Functional Blocks](../foundation/engineering-domain-model.md#functional-block); define the layer stack-up (copper/dielectric layers, materials, thicknesses) as typed quantities.
- Reserve keep-outs, mounting, connector edges, and thermal zones.

**Component Placement phase:**
- Position each [Component](../foundation/engineering-domain-model.md#component) (X/Y, rotation, side) producing [Placement](../foundation/engineering-domain-model.md#placement) entities.
- Honor placement Constraints: courtyards/clearances, thermal spreading, decoupling-near-pin, signal-flow locality, mechanical fixtures.
- Sequence work via the [Planning Engine](../engineering/planning-engine.md) (region-by-region, critical-components-first [Reasoning plan](../GLOSSARY.md#the-word-planning-disambiguation)).
- **Not** route nets ([Routing Agent](routing-agent.md)) or run [DRC](drc-agent.md); this agent *decides where parts live*.

## Inputs

- The [Schematic IR](../compiler/ir/schematic-ir.md) (Components, Nets, Functional Blocks) and mechanical/thermal [Constraints](../foundation/engineering-domain-model.md#constraint).
- [Footprint](../foundation/engineering-domain-model.md#footprint) geometry from the [Component Library](../engineering/component-library.md).
- Prior placement strategies from the [Learning Engine](../engineering/learning-engine.md).

## Outputs

- [Board](../foundation/engineering-domain-model.md#board--layer-stack) regions + layer stack-up (Floor Planning) and [Placement](../foundation/engineering-domain-model.md#placement) entities (Placement), committed with justifying [Decisions](../foundation/engineering-domain-model.md#decision).
- The [PCB IR](../compiler/ir/pcb-ir.md): Floor Planning transforms Schematic IR → PCB IR; Placement enriches it.
- Placement-conflict / density warnings surfaced for human disposition.

## State

The agent's own working state per activation: the active [Reasoning plan](../GLOSSARY.md#the-word-planning-disambiguation); the candidate region map / placement arrangement under evaluation; running density and keep-out checks; which phase is active and its goal; budget remaining. Ephemeral; the layout lives in [Engineering State](../core/shared-state-model.md), not the agent.

## Events

- **Emits:** `Board region allocated`, `Layer stack defined`, `Component placed/moved`, `Placement conflict flagged` — [Events](../core/event-bus.md) with justifying [Decisions](../foundation/engineering-domain-model.md#decision).
- **Consumes:** ERC-passed events (entry to Floor Planning); [DFM](../state-machines/dfm-verification.md)-failure events that loop back to Placement (per the [default workflow plan](../foundation/architecture-views.md)).

> Phase-advancement events belong to the [pcb-floor-planning](../state-machines/pcb-floor-planning.md) and [component-placement](../state-machines/component-placement.md) state machines.

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md), [Reasoning Engine port](../core/reasoning-engine-interface.md), [State Repository](../core/contracts.md#state-repository), [Knowledge](../knowledge/knowledge-graph.md) & [Vector Memory](../knowledge/vector-memory.md) ports, [Cost-budget](../core/contracts.md#cross-cutting-contracts) & [Security/Policy](../core/contracts.md#cross-cutting-contracts) ports.
- **Engines:** [Planning Engine](../engineering/planning-engine.md) (sequences floor planning/placement), [Constraint Engine](../engineering/constraint-engine.md) (keep-outs, clearances, thermal).
- **Driven by:** the [Execution Engine](../core/execution-engine.md) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md).

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **Over-dense board** (won't fit) | No legal placement. | Flag density; propose larger board or relax mechanical Constraint via escalation ([P10](../foundation/principles.md)). |
| **Keep-out / clearance violation** | Illegal placement. | [Constraint Engine](../engineering/constraint-engine.md) validation at the seam blocks the commit. |
| **Poor placement → unroutable** | Routing failure later. | Routing-awareness heuristics + loop-back from routing/DFM; placement is revisited, not forced. |
| **Reasoning invalid/unavailable** | No arrangement proposal. | Validate/repair at the seam; persistent → *failed*; FSM routes recovery. |

## Future improvements

- Routability and thermal estimation in-loop to score placements before committing.
- Learned placement templates per Functional Block family from the [Learning Engine](../engineering/learning-engine.md).
- Tighter coupling with the [Routing Agent](routing-agent.md) for placement/route co-optimization.

## Two-part split (P8)

| Half | In the Placement Agent |
|------|-------------------------|
| **Deterministic engineering use-case** | Reads Components/Nets/Footprints and Constraints; runs the [Planning Engine](../engineering/planning-engine.md) to sequence regioning/placement; checks every candidate region/position against keep-outs, courtyards, thermal, and mechanical Constraints via the [Constraint Engine](../engineering/constraint-engine.md); proposes `region allocated` / `component placed` capability invocations with justifying [Decisions](../foundation/engineering-domain-model.md#decision). |
| **Reasoning adapter** | Given the netlist, footprints, region map, and prior strategies, with a strict output schema ("region allocation + rationale"; "component positions: x/y, rotation, side + rationale"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) for floor-plan and placement proposals. Candidates only. |
| **The seam** | The use-case re-checks every proposed position for clearance/keep-out/thermal legality via the [Constraint Engine](../engineering/constraint-engine.md) before committing; an illegal placement is blocked ([P3](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phases / state machines:** [PCB Floor Planning](../state-machines/pcb-floor-planning.md) and [Component Placement](../state-machines/component-placement.md) — each owns its own states/transitions/events/rollback/recovery/persistence; Placement may loop back from [DFM Verification](../state-machines/dfm-verification.md). This agent drives both.
- **Engines used:** [Planning Engine](../engineering/planning-engine.md) (Floor Planning), [Constraint Engine](../engineering/constraint-engine.md) (both phases).
- **Primary IR:** transforms [Schematic IR](../compiler/ir/schematic-ir.md) → [PCB IR](../compiler/ir/pcb-ir.md) (Floor Planning) and enriches it (Placement).

## Related documents

[`agents/README.md`](README.md) · [`state-machines/pcb-floor-planning.md`](../state-machines/pcb-floor-planning.md) · [`state-machines/component-placement.md`](../state-machines/component-placement.md) · [`engineering/planning-engine.md`](../engineering/planning-engine.md) · [`engineering/constraint-engine.md`](../engineering/constraint-engine.md) · [`engineering/component-library.md`](../engineering/component-library.md) · [`compiler/ir/pcb-ir.md`](../compiler/ir/pcb-ir.md) · [`agents/routing-agent.md`](routing-agent.md)
