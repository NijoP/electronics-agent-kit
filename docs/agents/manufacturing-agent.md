# Manufacturing Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The Manufacturing Agent drives the **[Manufacturing Generation](../state-machines/manufacturing-generation.md)** phase — the final phase — turning the verified design ([PCB IR](../compiler/ir/pcb-ir.md) + [BOM](../foundation/engineering-domain-model.md#bom-line-item)) into the complete, validated manufacturing output set (fabrication, assembly, and test data) as the [Manufacturing IR](../compiler/ir/manufacturing-ir.md). It exists because handoff to a [fab/assembly house](../foundation/architecture-views.md) must be correct, complete, and traceable — the point where a design becomes a buildable product.

This doc follows the [Agent family template](../CONVENTIONS.md) and owns the agent's internals; states/transitions/persistence belong to the [manufacturing-generation state machine](../state-machines/manufacturing-generation.md) ([anti-duplication rule](README.md#anti-duplication-rule--agents-vs-state-machines)).

## Purpose

Generate the full manufacturing data package — fabrication geometry, drill data, assembly/pick-and-place data, the production [BOM](../foundation/engineering-domain-model.md#bom-line-item), and test/inspection data — and **validate its self-consistency and completeness** before it is released as [Artifacts](../foundation/architecture-views.md), enforcing that no open error-severity [Violation](../foundation/engineering-domain-model.md#violation) escapes to manufacturing.

## Responsibilities

- **Assemble the output set**: fabrication layers/geometry, drill/aperture data, assembly placement data, production BOM, and test/inspection data — as the [Manufacturing IR](../compiler/ir/manufacturing-ir.md).
- **Validate completeness & consistency** via the [Verification Engine](../engineering/verification-engine.md): outputs agree with the [PCB IR](../compiler/ir/pcb-ir.md)/[BOM IR](../compiler/ir/bom-ir.md), no layer/BOM mismatch, all referenced [Parts](../foundation/engineering-domain-model.md#part-manufacturer-part) resolved.
- **Enforce the release gate**: confirm no open error-severity [Violations](../foundation/engineering-domain-model.md#violation) (or only justified [Waivers](../foundation/engineering-domain-model.md#waiver)) remain — the [domain invariant](../foundation/engineering-domain-model.md#violation) for manufacturing release ([P5](../foundation/principles.md), [P10](../foundation/principles.md)).
- **Record provenance** linking every output back to its source design entities ([P5](../foundation/principles.md)).
- **Not** perform [DRC](drc-agent.md)/[DFM](dfm-agent.md) (those gate entry to this phase) or transmit to a vendor (an [integration](../core/contracts.md) concern); this agent *produces and validates the package*.

## Inputs

- The verified [PCB IR](../compiler/ir/pcb-ir.md) and [BOM IR](../compiler/ir/bom-ir.md), plus [Board](../foundation/engineering-domain-model.md#board--layer-stack)/stack-up and [Placements](../foundation/engineering-domain-model.md#placement).
- The [Violation](../foundation/engineering-domain-model.md#violation)/[Waiver](../foundation/engineering-domain-model.md#waiver) state from upstream verification phases.
- Target output-format/process requirements (from [standards/compliance](../engineering/standards-and-compliance.md)).

## Outputs

- The [Manufacturing IR](../compiler/ir/manufacturing-ir.md) and the released manufacturing [Artifacts](../foundation/architecture-views.md) (fabrication/assembly/test data), committed to [Engineering State](../core/shared-state-model.md) with provenance.
- A release verdict the [manufacturing-generation state machine](../state-machines/manufacturing-generation.md) acts on (complete & gate-clear → released; else blocked).
- Completeness/consistency findings surfaced for human disposition.

## State

The agent's own working state per activation: the output-generation work set, in-progress artifact set pending validation, the gate-check status, and budget remaining. Ephemeral; the released package lives in [Engineering State](../core/shared-state-model.md)/the [Artifact](../foundation/architecture-views.md) store, not the agent.

## Events

- **Emits:** `Manufacturing output generated`, `Output validated`, `Release gate checked`, `Package released`, `Release blocked` — [Events](../core/event-bus.md) with provenance to source entities.
- **Consumes:** EMC-accepted / all-verification-clear events (phase entry); re-generation triggers when an upstream artifact changes.

> Phase-advancement events belong to the [manufacturing-generation state machine](../state-machines/manufacturing-generation.md).

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md), [Reasoning Engine port](../core/reasoning-engine-interface.md), [State Repository](../core/contracts.md#state-repository), [Cost-budget](../core/contracts.md#cross-cutting-contracts) & [Security/Policy](../core/contracts.md#cross-cutting-contracts) ports.
- **Engines:** [Verification Engine](../engineering/verification-engine.md) (completeness/consistency validation, gate enforcement).
- **Driven by:** the [Execution Engine](../core/execution-engine.md) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md).

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **Open error-severity violation** | Unsafe release. | Release gate blocks; returns *blocked* with the offending [Violations](../foundation/engineering-domain-model.md#violation); requires fix or justified waiver ([P10](../foundation/principles.md)). |
| **Output/source mismatch** | Wrong fabrication data. | [Verification Engine](../engineering/verification-engine.md) consistency check against [PCB IR](../compiler/ir/pcb-ir.md)/[BOM IR](../compiler/ir/bom-ir.md) blocks release. |
| **Incomplete package** (missing layer/file) | Unbuildable handoff. | Completeness check enumerates required outputs; gaps block release; no silent omission ([P13](../foundation/principles.md)). |
| **Unresolved part** in production BOM | Procurement failure. | All BOM Line Items must resolve to orderable [Parts](../foundation/engineering-domain-model.md#part-manufacturer-part); unresolved entries block release. |

## Future improvements

- Vendor-/process-specific output profiles registered as [plugin capabilities](../core/capability-registry.md) (new exports without kernel changes, [P7](../foundation/principles.md)).
- Automated round-trip verification (re-import generated data and diff against source).
- Learned output-defect patterns from the [Learning Engine](../engineering/learning-engine.md) to pre-empt rejected fabrication packages.

## Two-part split (P8)

| Half | In the Manufacturing Agent |
|------|-----------------------------|
| **Deterministic engineering use-case** | Generates the manufacturing outputs from the verified design via the Capability port; validates completeness and self-consistency against the [PCB IR](../compiler/ir/pcb-ir.md)/[BOM IR](../compiler/ir/bom-ir.md) through the [Verification Engine](../engineering/verification-engine.md); enforces the release gate (no open error-severity Violations); records release [Decisions](../foundation/engineering-domain-model.md#decision)/provenance. Generation and validation are deterministic. |
| **Reasoning adapter** | Used sparingly: given a completeness/consistency finding, with a strict output schema ("explanation; ranked remediation steps; rationale"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) to *explain gaps* and *propose remediation*. It never authorizes release. Candidates only. |
| **The seam** | The reasoning half may explain/suggest, but **completeness, consistency, and the release gate are decided deterministically**; the package releases only after validation passes and the gate clears ([P3](../foundation/principles.md), [P10](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phase / state machine:** [Manufacturing Generation](../state-machines/manufacturing-generation.md) — owns states/transitions/events/rollback/recovery/persistence; the terminal phase of the [default workflow plan](../foundation/architecture-views.md). This agent drives it.
- **Engines used:** [Verification Engine](../engineering/verification-engine.md).
- **Primary IR produced:** transforms [PCB IR](../compiler/ir/pcb-ir.md) → [Manufacturing IR](../compiler/ir/manufacturing-ir.md).

## Related documents

[`agents/README.md`](README.md) · [`state-machines/manufacturing-generation.md`](../state-machines/manufacturing-generation.md) · [`engineering/verification-engine.md`](../engineering/verification-engine.md) · [`compiler/ir/manufacturing-ir.md`](../compiler/ir/manufacturing-ir.md) · [`compiler/ir/pcb-ir.md`](../compiler/ir/pcb-ir.md) · [`compiler/ir/bom-ir.md`](../compiler/ir/bom-ir.md) · [`agents/dfm-agent.md`](dfm-agent.md) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md#bom-line-item)
