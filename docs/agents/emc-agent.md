# EMC Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The EMC Agent drives the **[EMC Analysis](../state-machines/emc-analysis.md)** phase: it analyzes the layout ([PCB IR](../compiler/ir/pcb-ir.md)) for electromagnetic compatibility — emissions, susceptibility, signal/power integrity coupling — producing [Analysis Results](../foundation/engineering-domain-model.md#analysis-result) and improvement proposals. Unlike [ERC](../state-machines/erc-verification.md)/[DRC](../state-machines/drc-verification.md)/[DFM](../state-machines/dfm-verification.md), EMC is **analysis, not pass/fail rule-check**: it interprets simulated/estimated behavior against targets and surfaces risks. It exists because EMC problems are layout-dependent and far cheaper to catch before fabrication than at a compliance lab.

This doc follows the [Agent family template](../CONVENTIONS.md) and owns the agent's internals; states/transitions/persistence belong to the [emc-analysis state machine](../state-machines/emc-analysis.md) ([anti-duplication rule](README.md#anti-duplication-rule--agents-vs-state-machines)). It uses the [Verification Engine](../engineering/verification-engine.md) in its *analysis* mode (interpreting results against [Constraints](../foundation/engineering-domain-model.md#constraint)), not its pass/fail rule mode.

## Purpose

Assess the design's electromagnetic behavior — radiated/conducted emissions, immunity, return-path integrity, crosstalk, decoupling adequacy — by running or estimating analyses via the [Simulation port](../core/contracts.md#simulation-port), comparing results to EMC [Constraints](../foundation/engineering-domain-model.md#constraint)/targets, and proposing layout improvements (mostly routed back to [Routing](../state-machines/routing-planning.md)).

## Responsibilities

- **Run/estimate EMC analyses** via the [Simulation port](../core/contracts.md#simulation-port) (emissions, SI/PI coupling, return paths) and record typed [Analysis Results](../foundation/engineering-domain-model.md#analysis-result).
- **Interpret results** against EMC targets/[Constraints](../foundation/engineering-domain-model.md#constraint) using the [Verification Engine](../engineering/verification-engine.md) in analysis mode; grade risks with confidence.
- **Identify root causes** (split reference plane, long return loop, inadequate decoupling, poor diff-pair coupling) and link them to offending entities ([P5](../foundation/principles.md)).
- **Propose improvements** routed to the responsible phase (usually [Routing Planning](../state-machines/routing-planning.md), sometimes [Placement](../state-machines/component-placement.md)).
- **Record provenance** — every result cites its simulation/analysis source as [Evidence](../foundation/engineering-domain-model.md#evidence) ([P4](../foundation/principles.md)).
- **Not** render a binary fab gate (that is [DRC](drc-agent.md)/[DFM](dfm-agent.md)) or modify the layout; this agent *analyzes and advises*.

## Inputs

- The [PCB IR](../compiler/ir/pcb-ir.md): routed [Tracks](../foundation/engineering-domain-model.md#track--routing), [Board](../foundation/engineering-domain-model.md#board--layer-stack)/stack-up, [Placements](../foundation/engineering-domain-model.md#placement), net classes.
- EMC [Constraints](../foundation/engineering-domain-model.md#constraint)/targets and applicable [standards](../engineering/standards-and-compliance.md).
- [Simulation port](../core/contracts.md#simulation-port) capabilities; prior EMC findings from the [Learning Engine](../engineering/learning-engine.md).

## Outputs

- Typed [Analysis Results](../foundation/engineering-domain-model.md#analysis-result) with interpretation, confidence, and source, committed to [Engineering State](../core/shared-state-model.md) as [Evidence](../foundation/engineering-domain-model.md#evidence)-backed findings.
- A risk-graded verdict the [emc-analysis state machine](../state-machines/emc-analysis.md) acts on (acceptable → Manufacturing; risk → loop to Routing).
- Improvement proposals surfaced for human disposition.

## State

The agent's own working state per activation: the analysis work queue (which analyses to run with which parameters), in-progress results pending interpretation, candidate improvement proposals, and budget remaining. Ephemeral; Analysis Results persist in [Engineering State](../core/shared-state-model.md), not the agent.

## Events

- **Emits:** `Analysis run`, `Analysis result recorded`, `EMC risk flagged`, `Improvement proposed`, `EMC verdict reached` — [Events](../core/event-bus.md), results carrying [provenance](../core/provenance-and-traceability.md) to the simulation source.
- **Consumes:** DFM-passed / re-analyzed events that trigger an EMC run.

> Phase-advancement events (and the loop-back to Routing) belong to the [emc-analysis state machine](../state-machines/emc-analysis.md).

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md), [Reasoning Engine port](../core/reasoning-engine-interface.md), [Simulation port](../core/contracts.md#simulation-port) (the analysis runners), [State Repository](../core/contracts.md#state-repository), [Cost-budget](../core/contracts.md#cross-cutting-contracts) & [Security/Policy](../core/contracts.md#cross-cutting-contracts) ports.
- **Engines:** [Verification Engine](../engineering/verification-engine.md) (analysis-mode interpretation against Constraints); reads [Constraint Engine](../engineering/constraint-engine.md) targets.
- **Driven by:** the [Execution Engine](../core/execution-engine.md) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md).

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **Simulation unavailable/slow** | No analysis. | [Simulation port](../core/contracts.md#simulation-port) failure surfaced as recoverable; phase pauses/retries; coarse estimation fallback where defined, clearly labeled lower-confidence. |
| **Low-confidence result** | Uncertain risk. | Confidence recorded; engineer disposes ([P10](../foundation/principles.md)); never present an estimate as a guarantee ([P13](../foundation/principles.md)). |
| **Non-deterministic simulator output** | Replay divergence. | Inputs and outputs recorded as [Evidence](../foundation/engineering-domain-model.md#evidence); replay reuses recorded results ([P4](../foundation/principles.md)). |
| **Misinterpreted result** | Wrong improvement. | Interpretation validated against typed targets via the [Verification Engine](../engineering/verification-engine.md); reasoning explains, never overrides the measured result ([P3](../foundation/principles.md)). |

## Future improvements

- Pre-layout EMC estimation to steer [Placement](../state-machines/component-placement.md)/[Routing](../state-machines/routing-planning.md) earlier.
- Learned mappings from layout features to EMC outcomes via the [Learning Engine](../engineering/learning-engine.md).
- Standards-aware compliance pre-screening (e.g. CISPR class targets) with margin reporting.

## Two-part split (P8)

| Half | In the EMC Agent |
|------|-------------------|
| **Deterministic engineering use-case** | Selects and parameterizes analyses; invokes them through the [Simulation port](../core/contracts.md#simulation-port); records typed [Analysis Results](../foundation/engineering-domain-model.md#analysis-result) as [Evidence](../foundation/engineering-domain-model.md#evidence); interprets results against EMC targets via the [Verification Engine](../engineering/verification-engine.md); proposes findings/improvements with justifying [Decisions](../foundation/engineering-domain-model.md#decision). Result handling is deterministic and recorded. |
| **Reasoning adapter** | Given results and the layout context, with a strict output schema ("root-cause hypotheses; ranked improvements; rationale; confidence"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) to *diagnose* and *propose improvements*. It never invents measurements. Candidates only. |
| **The seam** | Reasoning interprets and suggests, but the **measured/simulated results and the risk grading against targets are deterministic**; an improvement only commits after validation and human disposition ([P3](../foundation/principles.md), [P10](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phase / state machine:** [EMC Analysis](../state-machines/emc-analysis.md) — owns states/transitions/events/rollback/recovery/persistence; on acceptable advances to [Manufacturing Generation](../state-machines/manufacturing-generation.md), on risk loops back to [Routing Planning](../state-machines/routing-planning.md). This agent drives it.
- **Engines used:** [Verification Engine](../engineering/verification-engine.md) (analysis mode).
- **Primary IR:** analyzes the [PCB IR](../compiler/ir/pcb-ir.md).

## Related documents

[`agents/README.md`](README.md) · [`state-machines/emc-analysis.md`](../state-machines/emc-analysis.md) · [`engineering/verification-engine.md`](../engineering/verification-engine.md) · [`core/contracts.md`](../core/contracts.md#simulation-port) (Simulation port) · [`agents/routing-agent.md`](routing-agent.md) · [`compiler/ir/pcb-ir.md`](../compiler/ir/pcb-ir.md) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md#analysis-result)
