# ERC Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The ERC Agent drives the **[ERC Verification](../state-machines/erc-verification.md)** phase: it runs Electrical Rule Checks over the logical design ([Schematic IR](../compiler/ir/schematic-ir.md)) through the [Verification Engine](../engineering/verification-engine.md), records [Violations](../foundation/engineering-domain-model.md#violation), and proposes explanations and fixes. It exists to catch electrical errors — floating inputs, output conflicts, power/ground issues, missing pull-ups — before they propagate into layout.

This doc follows the [Agent family template](../CONVENTIONS.md) and owns the agent's internals; states/transitions/persistence belong to the [erc-verification state machine](../state-machines/erc-verification.md) ([anti-duplication rule](README.md#anti-duplication-rule--agents-vs-state-machines)). The generic rule/violation/waiver mechanics belong to the [Verification Engine](../engineering/verification-engine.md); this agent specializes them for the electrical domain.

## Purpose

Evaluate the schematic against electrical [Rules](../foundation/engineering-domain-model.md#rule) (derived from [Constraints](../foundation/engineering-domain-model.md#constraint), [Pin](../foundation/engineering-domain-model.md#pin) electrical types, and net classes), produce a complete, explained set of [Violations](../foundation/engineering-domain-model.md#violation), and help the engineer resolve them — by proposing fixes (often routed back to the [Schematic Agent](schematic-agent.md)) or by recording justified [Waivers](../foundation/engineering-domain-model.md#waiver).

## Responsibilities

- **Run ERC rules** via the [Verification Engine](../engineering/verification-engine.md): pin-type conflicts (output-to-output), unconnected/floating pins, power/ground integrity, missing pull-ups, net-class mismatches.
- **Record Violations** with severity, offending entities, location, and a human-readable explanation ([P5](../foundation/principles.md)).
- **Propose fixes** and route them to the appropriate phase (most schematic fixes loop back to [Schematic Planning](../state-machines/schematic-planning.md)).
- **Manage waivers** — propose/record [Waivers](../foundation/engineering-domain-model.md#waiver) for accepted Violations, always justified and human-approved ([P10](../foundation/principles.md)).
- **Not** author the rule framework ([Verification Engine](../engineering/verification-engine.md)) or change the schematic itself (the [Schematic Agent](schematic-agent.md) applies fixes); this agent *checks and explains*.

## Inputs

- The [Schematic IR](../compiler/ir/schematic-ir.md) / schematic-domain [Engineering State](../core/shared-state-model.md) (Components, Pins, Nets).
- Electrical [Rules](../foundation/engineering-domain-model.md#rule) and [Constraints](../foundation/engineering-domain-model.md#constraint) from the [Constraint Engine](../engineering/constraint-engine.md) / [Verification Engine](../engineering/verification-engine.md).
- [Pin](../foundation/engineering-domain-model.md#pin) electrical types from [Datasheet Intelligence](../state-machines/datasheet-intelligence.md).

## Outputs

- A set of [Violations](../foundation/engineering-domain-model.md#violation) (and resolved/[waived](../foundation/engineering-domain-model.md#waiver) statuses) committed to [Engineering State](../core/shared-state-model.md).
- A pass/fail verdict that the [erc-verification state machine](../state-machines/erc-verification.md) acts on (pass → Floor Planning; fail → loop to Schematic).
- Fix proposals and waiver records surfaced for human disposition.

## State

The agent's own working state per activation: the rule-run work set, the in-progress violation list, candidate fix/waiver proposals pending validation, and budget remaining. Ephemeral; Violations persist in [Engineering State](../core/shared-state-model.md), not the agent.

## Events

- **Emits:** `Violation recorded`, `Violation explained`, `Fix proposed`, `Waiver proposed/recorded`, `ERC verdict reached` — [Events](../core/event-bus.md), waivers carrying [provenance](../core/provenance-and-traceability.md).
- **Consumes:** schematic-finalized / re-checked events that trigger an ERC run.

> Phase-advancement events (and the pass/fail loop-back) belong to the [erc-verification state machine](../state-machines/erc-verification.md).

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md), [Reasoning Engine port](../core/reasoning-engine-interface.md), [State Repository](../core/contracts.md#state-repository), [Cost-budget](../core/contracts.md#cross-cutting-contracts) & [Security/Policy](../core/contracts.md#cross-cutting-contracts) ports.
- **Engines:** [Verification Engine](../engineering/verification-engine.md) (rule evaluation, violation/waiver framework); reads [Constraint Engine](../engineering/constraint-engine.md) outputs.
- **Driven by:** the [Execution Engine](../core/execution-engine.md) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md).

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **False positive** | Spurious violation. | Reasoning-assisted explanation lets the engineer judge; waiver path with rationale; rule tuning fed to the [Learning Engine](../engineering/learning-engine.md). |
| **False negative** (missed error) | Latent fault into layout. | Deterministic rule set is the backbone; reasoning only *explains/prioritizes*, never replaces a rule ([P3](../foundation/principles.md)). |
| **Unfixable without trade-off** | Blocked phase. | Loop back to [Schematic Planning](../state-machines/schematic-planning.md) with a fix proposal, or record a justified waiver ([P10](../foundation/principles.md)). |
| **Reasoning unavailable** | No explanations/fixes. | Rule checking still runs deterministically; explanations degrade gracefully; verdict still produced. |

## Future improvements

- Severity/priority ranking of violations learned from past fix patterns ([Learning Engine](../engineering/learning-engine.md)).
- Auto-fix proposals with one-click hand-off to the [Schematic Agent](schematic-agent.md).
- Incremental ERC on schematic diffs rather than full re-runs.

## Two-part split (P8)

| Half | In the ERC Agent |
|------|-------------------|
| **Deterministic engineering use-case** | Invokes the [Verification Engine](../engineering/verification-engine.md) to evaluate electrical rules deterministically; records [Violations](../foundation/engineering-domain-model.md#violation) via the Capability port; validates any proposed waiver (justification present, scope valid); computes the pass/fail verdict. The verdict and the rule results are deterministic. |
| **Reasoning adapter** | Given a Violation and its context, with a strict output schema ("plain-language explanation; ranked candidate fixes; rationale"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) to *explain* and *propose fixes*. It never decides pass/fail. Candidates only. |
| **The seam** | Reasoning may explain and suggest, but the **verdict and the violation set come from the deterministic engine**; a fix or waiver only commits after validation and (for waivers) human approval ([P3](../foundation/principles.md), [P10](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phase / state machine:** [ERC Verification](../state-machines/erc-verification.md) — owns states/transitions/events/rollback/recovery/persistence; on pass advances to [PCB Floor Planning](../state-machines/pcb-floor-planning.md), on fail loops back to [Schematic Planning](../state-machines/schematic-planning.md). This agent drives it.
- **Engines used:** [Verification Engine](../engineering/verification-engine.md).
- **Primary IR:** checks the [Schematic IR](../compiler/ir/schematic-ir.md).

## Related documents

[`agents/README.md`](README.md) · [`state-machines/erc-verification.md`](../state-machines/erc-verification.md) · [`engineering/verification-engine.md`](../engineering/verification-engine.md) · [`agents/schematic-agent.md`](schematic-agent.md) · [`agents/drc-agent.md`](drc-agent.md) · [`compiler/ir/schematic-ir.md`](../compiler/ir/schematic-ir.md) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md#violation)
