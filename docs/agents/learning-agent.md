# Learning Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The Learning Agent is the **cross-cutting** agent: it is bound to **no single [Phase](../foundation/architecture-views.md)** and has **no state machine**. It is the driver of the [Learning Engine](../engineering/learning-engine.md) — it *observes all phases*, distills reusable engineering experience from completed, human-approved work, and surfaces that experience back to the other agents as [Evidence](../foundation/engineering-domain-model.md#evidence). It exists because an AI-native engineering tool that does not improve from its own audited history forfeits its main compounding advantage.

This doc follows the [Agent family template](../CONVENTIONS.md) and owns the agent's internals. Because it has no phase, its "FSM cross-link" section explains the *absence* of one and how the agent is instead activated cross-cuttingly. The capture/distill/curate *engine logic* belongs to the [Learning Engine](../engineering/learning-engine.md); this agent is its reasoning-capable driver.

## Purpose

Turn the audited [Event](../core/event-bus.md)/[Decision](../foundation/engineering-domain-model.md#decision) history of real design work into compact, retrievable **lessons** (patterns, good defaults, corrections, recurring violation/fix pairs) with their applicability context, and feed them back to improve future agent proposals and defaults — always as proposals/Evidence, never as silent state changes.

## Responsibilities

- **Observe** completed work across every phase: [Decisions](../foundation/engineering-domain-model.md#decision), the [Violations](../foundation/engineering-domain-model.md#violation) and fixes that resolved them, and — most valuable — engineer **corrections** that overrode AI proposals ([P10](../foundation/principles.md)).
- **Distill** raw history into lessons with applicability context (requirement class, [Constraint](../foundation/engineering-domain-model.md#constraint) profile, part family) via the [Learning Engine](../engineering/learning-engine.md).
- **Index** lessons in [Vector Memory](../knowledge/vector-memory.md) (similarity recall) and assert their relationships in the [Knowledge Graph](../knowledge/knowledge-graph.md) (relational traversal).
- **Surface** relevant experience to other agents' reasoning halves as [Evidence](../foundation/engineering-domain-model.md#evidence) when they face similar situations.
- **Curate** confidence: track how often a lesson held vs. was overridden so weak/stale lessons decay — deterministically, from recorded outcomes ([P4](../foundation/principles.md)).
- **Respect the ECC boundary**: reusable intelligence about *the engineering domain/designs* lives here; reusable intelligence about *building the product* goes to [ECC](../GLOSSARY.md#ecc), never into this agent or the Learning Engine.
- **Not** generate judgement of its own design changes or commit any design change — it informs; the engineer and the deterministic runtime dispose ([P3](../foundation/principles.md), [P10](../foundation/principles.md)).

## Inputs

- The [Event](../core/event-bus.md) history and [Decisions](../foundation/engineering-domain-model.md#decision) of completed work (read-only, via the [Event Source](../core/contracts.md#event-sink-event-source)).
- [Engineering State](../core/shared-state-model.md) context (read-only) to ground a lesson's applicability.
- Existing lessons and their outcome histories from the [Learning Engine](../engineering/learning-engine.md).

## Outputs

- Distilled **lessons** indexed in [Vector Memory](../knowledge/vector-memory.md) and linked in the [Knowledge Graph](../knowledge/knowledge-graph.md), each with provenance to the Decisions/Events it came from.
- Retrieved-experience [Evidence](../foundation/engineering-domain-model.md#evidence) supplied to other agents' reasoning halves (never a committed design change).
- Updated lesson confidence from observed accept/override outcomes.

## State

The agent's own working state per activation: the capture/distillation work queue (history ranges not yet processed), candidate lessons pending validation, and budget remaining. Ephemeral; lessons persist via the [Learning Engine](../engineering/learning-engine.md)'s knowledge/vector stores, never in the agent ([P2](../foundation/principles.md)).

## Events

- **Emits:** `Lesson distilled`, `Lesson indexed/linked`, `Lesson confidence updated`, `Experience surfaced` — [Events](../core/event-bus.md) with provenance to source Decisions/Events.
- **Consumes:** phase-completed, decision-recorded, correction-made, and violation-fixed events from *all* phases — these are its capture triggers.

> The Learning Agent has **no phase state machine**, so there are no phase-advancement events to delegate. It reacts to events emitted by the other 13 agents' phases.

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md) (to index/assert lessons), [Reasoning Engine port](../core/reasoning-engine-interface.md) (to help distill/generalize patterns), [Event Source](../core/contracts.md#event-sink-event-source) (observe history), [Knowledge port](../knowledge/knowledge-graph.md) & [Vector Memory port](../knowledge/vector-memory.md) (index/link/retrieve), [State Repository](../core/contracts.md#state-repository) (read-only grounding), [Security/Policy port](../core/contracts.md#cross-cutting-contracts) (cross-tenant boundaries), [Cost-budget port](../core/contracts.md#cross-cutting-contracts).
- **Engines:** [Learning Engine](../engineering/learning-engine.md) (its host engine — capture/distill/curate/retrieve).
- **Driven by:** the runtime cross-cuttingly (post-phase/idle/scheduled activations) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md) — *not* by a phase [Execution Engine](../core/execution-engine.md) transition.

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **Over-fitting** (narrow lesson applied broadly) | Bad suggestion. | Strict applicability context + confidence decay; the engineer disposes ([P10](../foundation/principles.md)). |
| **Stale lesson** | Outdated advice. | Decay from recorded outcomes; a lesson is never binding state on its own. |
| **Cold start** (sparse history) | Little to offer. | Surfaces less; agents fall back to first-principles reasoning; no fabricated experience ([P13](../foundation/principles.md)). |
| **Retrieval capability down** | No suggestions. | Degrades to no-aid; design work proceeds unaffected. |
| **Privacy/cross-tenant leakage** | Confidential design exposure. | [Security/Policy port](../core/contracts.md#cross-cutting-contracts) gates which experience informs which project; never surfaced without authorization. |
| **ECC-boundary bleed** | Product-build trivia in engineering memory. | Hard boundary: product-build intelligence goes to [ECC](../GLOSSARY.md#ecc), not here. |

## Future improvements

- Active suggestion of which past lessons to revisit when a similar new project starts.
- Confidence calibration tuned against long-run accept/override statistics.
- Configurable distillation thresholds via the [Configuration port](../core/contracts.md#cross-cutting-contracts) (future ADR).

## Two-part split (P8)

| Half | In the Learning Agent |
|------|------------------------|
| **Deterministic engineering use-case** | Observes [Events](../core/event-bus.md)/[Decisions](../foundation/engineering-domain-model.md#decision) (read-only); drives the [Learning Engine](../engineering/learning-engine.md) to distill/curate; validates candidate lessons (applicability context present, provenance intact, within tenant policy); indexes/links them via the Capability port; computes confidence deterministically from recorded outcomes. |
| **Reasoning adapter** | Given raw history excerpts, with a strict output schema ("lesson: pattern, applicability context, generalization, confidence"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) to *help generalize* recurring structures into reusable lessons. Candidates only. |
| **The seam** | The reasoning half proposes generalizations; the deterministic half validates them (grounded in real Decisions, scoped applicability, tenant-authorized) before indexing — and a lesson, even once indexed, is only ever *Evidence* for future proposals, never a committed design change ([P3](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phase / state machine:** **none** — per the [canonical phase map](../foundation/architecture-views.md) the Learning capability is an *engine, not a phase*, so this agent has no state machine. It does not drive a phase; it observes all 14 phases' state machines via the events they emit, and is activated cross-cuttingly (between/after phases, on idle, or on schedule).
- **Engines used:** [Learning Engine](../engineering/learning-engine.md) (its host).
- **Primary IR:** none of its own; it *observes across* all IRs and the [Event](../core/event-bus.md) history.

## Related documents

[`agents/README.md`](README.md) · [`engineering/learning-engine.md`](../engineering/learning-engine.md) · [`knowledge/vector-memory.md`](../knowledge/vector-memory.md) · [`knowledge/knowledge-graph.md`](../knowledge/knowledge-graph.md) · [`core/provenance-and-traceability.md`](../core/provenance-and-traceability.md) · [`foundation/architecture-views.md`](../foundation/architecture-views.md) · [`GLOSSARY.md`](../GLOSSARY.md#ecc) (ECC boundary) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md#decision)
