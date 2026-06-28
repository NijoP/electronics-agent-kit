# Datasheet Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The Datasheet Agent drives the **[Datasheet Intelligence](../state-machines/datasheet-intelligence.md)** phase: it extracts structured engineering facts — parametric limits, pinouts, recommended operating conditions, compliance flags — from component datasheets and asserts them into the [Knowledge Graph](../knowledge/knowledge-graph.md) as queryable [Evidence](../foundation/engineering-domain-model.md#evidence) about [Parts](../foundation/engineering-domain-model.md#part-manufacturer-part). It exists so the runtime *owns* part knowledge ([P2](../foundation/principles.md)) instead of relying on facts buried in PDFs or model memory.

This doc follows the [Agent family template](../CONVENTIONS.md) and owns the agent's internals; the phase's states/transitions/persistence belong to the [datasheet-intelligence state machine](../state-machines/datasheet-intelligence.md) ([anti-duplication rule](README.md#anti-duplication-rule--agents-vs-state-machines)).

## Purpose

Convert unstructured datasheet content into typed, sourced, queryable facts about a [Part](../foundation/engineering-domain-model.md#part-manufacturer-part): absolute-maximum and recommended ratings, pin map and pin electrical types, package/thermal data, and compliance (RoHS/REACH) — each as a [Physical Quantity](../engineering/units-and-quantities.md) where applicable and each carrying its source citation.

## Responsibilities

- **Extract parametric facts** (voltage/current/temperature limits, timing, package thermal resistance) as typed quantities.
- **Extract the pinout** — pin numbers/names mapped to electrical type (input/output/power/passive/bidirectional/no-connect), feeding later [Pin](../foundation/engineering-domain-model.md#pin)/[ERC](../state-machines/erc-verification.md) checks.
- **Extract compliance & lifecycle** flags (RoHS, REACH, active/NRND/EOL).
- **Assert into the Knowledge Graph** with full provenance — every fact cites its datasheet location ([P5](../foundation/principles.md)).
- **Flag low-confidence extractions** for human confirmation rather than asserting uncertain facts as truth.
- **Not** select Parts (that is the [BOM Agent](bom-agent.md)) and **not** check the design against these facts (that is [ERC](erc-agent.md)/[DRC](drc-agent.md)); this agent only *produces verified knowledge*.

## Inputs

- Datasheet documents/references for [Parts](../foundation/engineering-domain-model.md#part-manufacturer-part) under consideration (resolved via the [Parts-data port](../core/contracts.md#parts-data-port)).
- The [Component](../foundation/engineering-domain-model.md#component)/Part context indicating which facts matter.
- Prior extractions and corrections from the [Learning Engine](../engineering/learning-engine.md).

## Outputs

- Structured part facts asserted into the [Knowledge Graph](../knowledge/knowledge-graph.md) (and indexed in [Vector Memory](../knowledge/vector-memory.md) for similarity recall), each as [Evidence](../foundation/engineering-domain-model.md#evidence) with provenance.
- Enrichment of the [Engineering IR](../compiler/ir/engineering-ir.md) with part facts available to downstream phases.
- Low-confidence items surfaced for human confirmation.

## State

The agent's own working state per activation: the extraction work queue (documents/sections to process), candidate facts pending validation, per-fact confidence, and budget remaining. Ephemeral; the durable home of extracted facts is the [Knowledge-Graph Store](../knowledge/knowledge-graph.md) via its port, never the agent.

## Events

- **Emits:** `Part fact asserted`, `Pinout extracted`, `Compliance flag recorded`, `Low-confidence extraction flagged` — [Events](../core/event-bus.md) with provenance to the datasheet source.
- **Consumes:** part-selected/under-consideration events that trigger extraction; correction events from human review.

> Phase-advancement events belong to the [datasheet-intelligence state machine](../state-machines/datasheet-intelligence.md).

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md), [Reasoning Engine port](../core/reasoning-engine-interface.md), [Knowledge port](../knowledge/knowledge-graph.md) (assert/query facts), [Vector Memory port](../knowledge/vector-memory.md), [Parts-data port](../core/contracts.md#parts-data-port) (fetch datasheets), [State Repository](../core/contracts.md#state-repository) (read part context), [Cost-budget](../core/contracts.md#cross-cutting-contracts) & [Security/Policy](../core/contracts.md#cross-cutting-contracts) ports.
- **Engines:** none — this phase *feeds the [Knowledge Graph](../knowledge/knowledge-graph.md)* rather than using an engine (per the [canonical phase map](../foundation/architecture-views.md)).
- **Driven by:** the [Execution Engine](../core/execution-engine.md) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md).

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **Ambiguous/garbled datasheet** | Wrong or missing facts. | Confidence scoring + human confirmation for low-confidence items; never assert uncertain facts as truth. |
| **Hallucinated parameter** | Plausible-but-false fact corrupts knowledge. | Domain validation (unit/range sanity) at the seam; every fact must cite a source location; uncited facts rejected ([P3](../foundation/principles.md)). |
| **Datasheet unavailable** | No extraction. | [Parts-data port](../core/contracts.md#parts-data-port) failure surfaced as recoverable; phase pauses/retries; design proceeds on known facts only. |
| **Unit ambiguity** | Dimensional error. | All numeric facts typed as [Physical Quantities](../engineering/units-and-quantities.md); untyped numbers rejected ([P9](../foundation/principles.md)). |

## Future improvements

- Cross-checking extracted facts against multiple sources (datasheet vs. distributor parametrics) and flagging disagreements.
- Reuse of prior extractions for the same MPN via the [Learning Engine](../engineering/learning-engine.md) to avoid re-processing.
- Confidence calibration learned from human confirmation history.

## Two-part split (P8)

| Half | In the Datasheet Agent |
|------|-------------------------|
| **Deterministic engineering use-case** | Fetches datasheets via the [Parts-data port](../core/contracts.md#parts-data-port); manages the extraction queue; validates candidate facts (unit/range sanity, source citation present) against [domain](../foundation/engineering-domain-model.md) and [unit](../engineering/units-and-quantities.md) rules; asserts validated facts into the [Knowledge Graph](../knowledge/knowledge-graph.md) via the Capability port with provenance. |
| **Reasoning adapter** | Given datasheet text and a strict output schema ("fact: name, typed value, condition, source location, confidence"; "pin: number, name, electrical type"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) to extract structured facts and pinouts. Returns candidates with confidence only. |
| **The seam** | The use-case validates each candidate (schema + unit/range sanity + citation) before asserting; uncited or implausible facts are blocked, and low-confidence ones are escalated for confirmation ([P10](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phase / state machine:** [Datasheet Intelligence](../state-machines/datasheet-intelligence.md) — owns states/transitions/events/rollback/recovery/persistence. This agent drives it.
- **Engines used:** none; **feeds** the [Knowledge Graph](../knowledge/knowledge-graph.md) and [Vector Memory](../knowledge/vector-memory.md) capabilities.
- **Primary IR:** enriches the [Engineering IR](../compiler/ir/engineering-ir.md) with part facts.

## Related documents

[`agents/README.md`](README.md) · [`state-machines/datasheet-intelligence.md`](../state-machines/datasheet-intelligence.md) · [`knowledge/knowledge-graph.md`](../knowledge/knowledge-graph.md) · [`knowledge/vector-memory.md`](../knowledge/vector-memory.md) · [`agents/bom-agent.md`](bom-agent.md) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md#part-manufacturer-part) · [`engineering/units-and-quantities.md`](../engineering/units-and-quantities.md)
