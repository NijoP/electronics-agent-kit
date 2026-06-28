# DRC Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The DRC Agent drives the **[DRC Verification](../state-machines/drc-verification.md)** phase: it runs Design Rule Checks over the physical layout ([PCB IR](../compiler/ir/pcb-ir.md)) through the [Verification Engine](../engineering/verification-engine.md), records [Violations](../foundation/engineering-domain-model.md#violation), and proposes explanations and fixes. It exists to guarantee the copper, drills, and spacing are physically legal and fabricable before the design proceeds toward manufacturing.

This doc follows the [Agent family template](../CONVENTIONS.md) and owns the agent's internals; states/transitions/persistence belong to the [drc-verification state machine](../state-machines/drc-verification.md) ([anti-duplication rule](README.md#anti-duplication-rule--agents-vs-state-machines)). The generic rule/violation/waiver mechanics belong to the [Verification Engine](../engineering/verification-engine.md); this agent specializes them for the physical-layout domain.

## Purpose

Evaluate the layout against physical [Rules](../foundation/engineering-domain-model.md#rule) (clearance, trace width, annular ring, drill size, copper-to-edge, courtyard overlap, via rules) derived from [Constraints](../foundation/engineering-domain-model.md#constraint) and the [Board](../foundation/engineering-domain-model.md#board--layer-stack) stack-up, produce a complete, explained set of [Violations](../foundation/engineering-domain-model.md#violation), and help resolve them (fixes routed back to [Routing](../state-machines/routing-planning.md) or [Placement](../state-machines/component-placement.md), or justified [Waivers](../foundation/engineering-domain-model.md#waiver)).

## Responsibilities

- **Run DRC rules** via the [Verification Engine](../engineering/verification-engine.md): clearances, widths, annular rings, drills, copper-to-edge, courtyard overlaps, acid traps, via-in-pad rules.
- **Record Violations** with severity, offending entities, location, and explanation ([P5](../foundation/principles.md)).
- **Propose fixes** and route them to the responsible phase ([Routing](../state-machines/routing-planning.md) for copper, [Placement](../state-machines/component-placement.md) for spacing).
- **Manage waivers** — justified, human-approved [Waivers](../foundation/engineering-domain-model.md#waiver) for accepted Violations ([P10](../foundation/principles.md)).
- **Enforce the gate** — a design with open error-severity Violations cannot advance toward [Manufacturing Generation](../state-machines/manufacturing-generation.md).
- **Not** author the rule framework ([Verification Engine](../engineering/verification-engine.md)) or modify the layout (Routing/Placement agents apply fixes); this agent *checks and explains*.

## Inputs

- The [PCB IR](../compiler/ir/pcb-ir.md) / physical-domain [Engineering State](../core/shared-state-model.md): [Tracks](../foundation/engineering-domain-model.md#track--routing), vias, [Placements](../foundation/engineering-domain-model.md#placement), [Board](../foundation/engineering-domain-model.md#board--layer-stack)/stack-up.
- Physical [Rules](../foundation/engineering-domain-model.md#rule) and [Constraints](../foundation/engineering-domain-model.md#constraint) from the [Constraint Engine](../engineering/constraint-engine.md) / [Verification Engine](../engineering/verification-engine.md).

## Outputs

- A set of [Violations](../foundation/engineering-domain-model.md#violation) (and resolved/[waived](../foundation/engineering-domain-model.md#waiver) statuses) committed to [Engineering State](../core/shared-state-model.md).
- A pass/fail verdict the [drc-verification state machine](../state-machines/drc-verification.md) acts on (pass → DFM; fail → loop to Routing).
- Fix proposals and waiver records surfaced for human disposition.

## State

The agent's own working state per activation: the rule-run work set, the in-progress violation list, candidate fix/waiver proposals pending validation, and budget remaining. Ephemeral; Violations persist in [Engineering State](../core/shared-state-model.md), not the agent.

## Events

- **Emits:** `Violation recorded`, `Violation explained`, `Fix proposed`, `Waiver proposed/recorded`, `DRC verdict reached` — [Events](../core/event-bus.md), waivers carrying [provenance](../core/provenance-and-traceability.md).
- **Consumes:** routing-finalized / re-checked events that trigger a DRC run.

> Phase-advancement events (and the pass/fail loop-back) belong to the [drc-verification state machine](../state-machines/drc-verification.md).

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md), [Reasoning Engine port](../core/reasoning-engine-interface.md), [State Repository](../core/contracts.md#state-repository), [Cost-budget](../core/contracts.md#cross-cutting-contracts) & [Security/Policy](../core/contracts.md#cross-cutting-contracts) ports.
- **Engines:** [Verification Engine](../engineering/verification-engine.md) (rule evaluation, violation/waiver framework); reads [Constraint Engine](../engineering/constraint-engine.md) outputs.
- **Driven by:** the [Execution Engine](../core/execution-engine.md) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md).

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **False positive** | Spurious violation. | Reasoning-assisted explanation; waiver path with rationale; rule tuning fed to the [Learning Engine](../engineering/learning-engine.md). |
| **False negative** (missed defect) | Fabrication fault. | Deterministic rule set is the backbone; reasoning only explains/prioritizes, never replaces a rule ([P3](../foundation/principles.md)). |
| **Unfixable without trade-off** | Blocked gate. | Loop back to [Routing](../state-machines/routing-planning.md)/[Placement](../state-machines/component-placement.md) with a fix proposal, or record a justified waiver ([P10](../foundation/principles.md)). |
| **Reasoning unavailable** | No explanations/fixes. | Rule checking still runs deterministically; verdict still produced; explanations degrade gracefully. |

## Future improvements

- Violation clustering and root-cause grouping to reduce fix churn.
- Auto-fix proposals with one-click hand-off to the [Routing Agent](routing-agent.md).
- Incremental DRC on layout diffs rather than full re-runs.

## Two-part split (P8)

| Half | In the DRC Agent |
|------|-------------------|
| **Deterministic engineering use-case** | Invokes the [Verification Engine](../engineering/verification-engine.md) to evaluate physical rules deterministically; records [Violations](../foundation/engineering-domain-model.md#violation) via the Capability port; validates any proposed waiver; computes the pass/fail verdict and enforces the manufacturing gate. The verdict and rule results are deterministic. |
| **Reasoning adapter** | Given a Violation and its context, with a strict output schema ("explanation; ranked candidate fixes; rationale"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) to *explain* and *propose fixes*. It never decides pass/fail. Candidates only. |
| **The seam** | Reasoning may explain and suggest, but the **verdict and violation set come from the deterministic engine**; a fix or waiver only commits after validation and (for waivers) human approval ([P3](../foundation/principles.md), [P10](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phase / state machine:** [DRC Verification](../state-machines/drc-verification.md) — owns states/transitions/events/rollback/recovery/persistence; on pass advances to [DFM Verification](../state-machines/dfm-verification.md), on fail loops back to [Routing Planning](../state-machines/routing-planning.md). This agent drives it.
- **Engines used:** [Verification Engine](../engineering/verification-engine.md).
- **Primary IR:** checks the [PCB IR](../compiler/ir/pcb-ir.md).

## Related documents

[`agents/README.md`](README.md) · [`state-machines/drc-verification.md`](../state-machines/drc-verification.md) · [`engineering/verification-engine.md`](../engineering/verification-engine.md) · [`agents/routing-agent.md`](routing-agent.md) · [`agents/dfm-agent.md`](dfm-agent.md) · [`agents/erc-agent.md`](erc-agent.md) · [`compiler/ir/pcb-ir.md`](../compiler/ir/pcb-ir.md) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md#violation)
