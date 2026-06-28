# DFM Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The DFM Agent drives the **[DFM Verification](../state-machines/dfm-verification.md)** phase: it checks the layout ([PCB IR](../compiler/ir/pcb-ir.md)) for *manufacturability and assembly* against fabrication/assembly process capabilities, through the [Verification Engine](../engineering/verification-engine.md), recording [Violations](../foundation/engineering-domain-model.md#violation) and proposing fixes. It exists because a layout can be electrically and geometrically legal ([DRC](../state-machines/drc-verification.md)-clean) yet expensive, low-yield, or impossible for a given fab/assembly house.

This doc follows the [Agent family template](../CONVENTIONS.md) and owns the agent's internals; states/transitions/persistence belong to the [dfm-verification state machine](../state-machines/dfm-verification.md) ([anti-duplication rule](README.md#anti-duplication-rule--agents-vs-state-machines)). The generic rule/violation/waiver mechanics belong to the [Verification Engine](../engineering/verification-engine.md); this agent specializes them for the manufacturability domain.

## Purpose

Evaluate the layout against **process-capability** [Rules](../foundation/engineering-domain-model.md#rule) (minimum feature sizes, solder-mask sliver/clearance, silkscreen legibility, panelization, testability/test-point access, component-spacing for pick-and-place, thermal-relief for assembly) so the design is producible at target yield and cost, and surface yield/cost-risk findings the engineer can act on.

## Responsibilities

- **Run DFM rules** via the [Verification Engine](../engineering/verification-engine.md): fabrication limits, solder-mask/silkscreen, panelization, test-point coverage, assembly clearances, thermal relief.
- **Record Violations / risks** with severity, location, and a manufacturability explanation ([P5](../foundation/principles.md)).
- **Propose fixes** routed to the responsible phase ([Placement](../state-machines/component-placement.md) for spacing, [Routing](../state-machines/routing-planning.md) for features).
- **Manage waivers** — justified, human-approved [Waivers](../foundation/engineering-domain-model.md#waiver) where a risk is accepted ([P10](../foundation/principles.md)).
- **Tie to process capability** — checks are parameterized by the target fab/assembly capability profile (via [standards/compliance](../engineering/standards-and-compliance.md) and process data).
- **Not** author the rule framework ([Verification Engine](../engineering/verification-engine.md)) or modify the layout; this agent *checks and explains manufacturability*.

## Inputs

- The [PCB IR](../compiler/ir/pcb-ir.md): [Tracks](../foundation/engineering-domain-model.md#track--routing), [Placements](../foundation/engineering-domain-model.md#placement), [Footprints](../foundation/engineering-domain-model.md#footprint), [Board](../foundation/engineering-domain-model.md#board--layer-stack)/stack-up.
- Target process-capability profile and manufacturing [Constraints](../foundation/engineering-domain-model.md#constraint) (from [standards/compliance](../engineering/standards-and-compliance.md) and the [Constraint Engine](../engineering/constraint-engine.md)).
- Prior DFM findings/fixes from the [Learning Engine](../engineering/learning-engine.md).

## Outputs

- A set of manufacturability [Violations](../foundation/engineering-domain-model.md#violation)/risks (and resolved/[waived](../foundation/engineering-domain-model.md#waiver) statuses) committed to [Engineering State](../core/shared-state-model.md).
- A pass/fail (with risk grading) verdict the [dfm-verification state machine](../state-machines/dfm-verification.md) acts on (pass → EMC; fail → loop to Placement).
- Yield/cost-risk findings and fix proposals surfaced for human disposition.

## State

The agent's own working state per activation: the rule-run work set, the in-progress violation/risk list, candidate fix/waiver proposals pending validation, and budget remaining. Ephemeral; findings persist in [Engineering State](../core/shared-state-model.md), not the agent.

## Events

- **Emits:** `DFM violation recorded`, `Yield risk flagged`, `Fix proposed`, `Waiver proposed/recorded`, `DFM verdict reached` — [Events](../core/event-bus.md), waivers carrying [provenance](../core/provenance-and-traceability.md).
- **Consumes:** DRC-passed / re-checked events that trigger a DFM run.

> Phase-advancement events (and the pass/fail loop-back) belong to the [dfm-verification state machine](../state-machines/dfm-verification.md).

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md), [Reasoning Engine port](../core/reasoning-engine-interface.md), [State Repository](../core/contracts.md#state-repository), [Cost-budget](../core/contracts.md#cross-cutting-contracts) & [Security/Policy](../core/contracts.md#cross-cutting-contracts) ports.
- **Engines:** [Verification Engine](../engineering/verification-engine.md) (rule evaluation, violation/waiver framework); reads [Constraint Engine](../engineering/constraint-engine.md) outputs and process-capability data.
- **Driven by:** the [Execution Engine](../core/execution-engine.md) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md).

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **Wrong process profile** | Checks mismatched to fab. | Profile is explicit input/[Evidence](../foundation/engineering-domain-model.md#evidence); mismatch surfaced; never assume a default silently ([P13](../foundation/principles.md)). |
| **Over-conservative rule** | Spurious risk. | Reasoning-assisted explanation + waiver path; rule tuning fed to the [Learning Engine](../engineering/learning-engine.md). |
| **Missed manufacturability issue** | Low yield / scrap. | Deterministic rule set is the backbone; reasoning only explains/prioritizes ([P3](../foundation/principles.md)). |
| **Reasoning unavailable** | No explanations/fixes. | Rule checking still runs; verdict still produced; explanations degrade gracefully. |

## Future improvements

- Quantitative yield/cost modeling per process profile, surfaced as trade-offs.
- Direct ingestion of fab/assembly capability sheets to parameterize rules.
- Learned DFM fix patterns per footprint family from the [Learning Engine](../engineering/learning-engine.md).

## Two-part split (P8)

| Half | In the DFM Agent |
|------|-------------------|
| **Deterministic engineering use-case** | Invokes the [Verification Engine](../engineering/verification-engine.md) to evaluate manufacturability rules against the process profile deterministically; records [Violations](../foundation/engineering-domain-model.md#violation)/risks via the Capability port; validates any proposed waiver; computes the pass/fail-with-risk verdict. The verdict and rule results are deterministic. |
| **Reasoning adapter** | Given a finding and its context, with a strict output schema ("manufacturability explanation; ranked candidate fixes; cost/yield rationale"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) to *explain* and *propose fixes*. It never decides pass/fail. Candidates only. |
| **The seam** | Reasoning explains and suggests, but the **verdict and findings come from the deterministic engine**; a fix or waiver only commits after validation and (for waivers) human approval ([P3](../foundation/principles.md), [P10](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phase / state machine:** [DFM Verification](../state-machines/dfm-verification.md) — owns states/transitions/events/rollback/recovery/persistence; on pass advances to [EMC Analysis](../state-machines/emc-analysis.md), on fail loops back to [Component Placement](../state-machines/component-placement.md). This agent drives it.
- **Engines used:** [Verification Engine](../engineering/verification-engine.md).
- **Primary IR:** checks the [PCB IR](../compiler/ir/pcb-ir.md).

## Related documents

[`agents/README.md`](README.md) · [`state-machines/dfm-verification.md`](../state-machines/dfm-verification.md) · [`engineering/verification-engine.md`](../engineering/verification-engine.md) · [`engineering/standards-and-compliance.md`](../engineering/standards-and-compliance.md) · [`agents/drc-agent.md`](drc-agent.md) · [`agents/placement-agent.md`](placement-agent.md) · [`compiler/ir/pcb-ir.md`](../compiler/ir/pcb-ir.md) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md#violation)
