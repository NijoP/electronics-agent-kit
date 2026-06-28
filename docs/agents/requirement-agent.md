# Requirement Agent

> **Ring:** Use cases / runtime — *instance* ([P7](../foundation/principles.md)). Family: [Agent](README.md). The Requirement Agent drives the **[Requirement Planning](../state-machines/requirement-planning.md)** phase — the first engineering phase, where human [Design Intent](../foundation/engineering-domain-model.md#design-intent) becomes a structured, testable, traceable set of [Requirements](../foundation/engineering-domain-model.md#requirement). It exists because everything downstream ([traceability](../core/provenance-and-traceability.md), [constraints](../engineering/constraint-engine.md), verification) is rooted in well-formed requirements; this agent is where natural-language intent is turned into the root of the engineering-knowledge tree.

This doc follows the [Agent family template](../CONVENTIONS.md). It owns the agent's purpose, I/O, working state, events, dependencies, failures, and the two-part split. It does **not** restate the phase's states/transitions — those belong to the [requirement-planning state machine](../state-machines/requirement-planning.md) ([anti-duplication rule](README.md#anti-duplication-rule--agents-vs-state-machines)).

## Purpose

Transform free-form [Design Intent](../foundation/engineering-domain-model.md#design-intent) ("a USB-C powered IoT sensor node, < 5 W, < 50 × 50 mm") into a complete, deduplicated, prioritized set of testable [Requirements](../foundation/engineering-domain-model.md#requirement), each with an acceptance criterion and a source link back to the intent or an external [standard](../engineering/standards-and-compliance.md). The agent *proposes*; the engineer *disposes* ([P10](../foundation/principles.md)).

## Responsibilities

- **Elicit & structure** intent into discrete Requirements with category (functional / electrical / mechanical / thermal / regulatory / cost / schedule), priority, and acceptance criterion.
- **Detect gaps & ambiguities** (missing power budget, unstated environmental range) and surface clarifying questions to the engineer.
- **Detect conflicts & duplicates** among proposed Requirements (e.g. a size target that contradicts a connector choice).
- **Bind sources** — link each Requirement to its originating Design Intent or external standard for [traceability](../core/provenance-and-traceability.md) ([P5](../foundation/principles.md)).
- **Sequence its own work** using the [Planning Engine](../engineering/planning-engine.md) (a [Reasoning plan](../GLOSSARY.md#the-word-planning-disambiguation) for the elicitation steps).
- **Not** derive enforceable [Constraints](../foundation/engineering-domain-model.md#constraint) — that is the [Planning Agent's](planning-agent.md) Constraint Extraction phase. This agent stops at *intent → testable statement*.

## Inputs

- [Design Intent](../foundation/engineering-domain-model.md#design-intent) (natural language, possibly iterative) — the primary input.
- Engineer answers to clarifying questions (via the [Presentation/Query port](../core/contracts.md#presentation-query-port)).
- Applicable [standards/compliance](../engineering/standards-and-compliance.md) context and prior-art experience from the [Learning Engine](../engineering/learning-engine.md) (as [Evidence](../foundation/engineering-domain-model.md#evidence)).

## Outputs

- A set of [Requirement](../foundation/engineering-domain-model.md#requirement) entities committed to [Engineering State](../core/shared-state-model.md) via the [Capability port](../core/capability-registry.md), each justified by a [Decision](../foundation/engineering-domain-model.md#decision).
- The [Requirement IR](../compiler/ir/requirement-ir.md) projection at the phase boundary.
- Open clarifying questions and detected conflicts surfaced for human disposition.

## State

The agent's **own working state** during an activation (distinct from [Engineering State](../core/shared-state-model.md) and from the [phase FSM's states](../state-machines/requirement-planning.md)): the current [Reasoning plan](../GLOSSARY.md#the-word-planning-disambiguation) of elicitation steps, the working set of draft Requirements not yet committed, the open-questions list, and the per-activation budget remaining. All of this is ephemeral; nothing durable lives in the agent ([P2](../foundation/principles.md)).

## Events

- **Emits** (as the runtime commits its capability invocations): `Requirement created/refined`, `Requirement source linked`, `Conflict detected`, `Clarification requested`. All are [Events](../core/event-bus.md) carrying their justifying [Decision](../foundation/engineering-domain-model.md#decision).
- **Consumes**: engineer-answer events and Design-Intent-updated events that re-trigger elicitation within the phase.

> The *phase-level* events that advance the [state machine](../state-machines/requirement-planning.md) (e.g. phase entered/exited, gate passed) are owned by that FSM, not restated here.

## Dependencies

- **Ports:** [Capability port](../core/capability-registry.md) (the only action surface), [Reasoning Engine port](../core/reasoning-engine-interface.md) (judgement), [State Repository](../core/contracts.md#state-repository) (scoped reads), [Knowledge port](../knowledge/knowledge-graph.md) & [Vector Memory port](../knowledge/vector-memory.md) (prior-art context), [Presentation/Query port](../core/contracts.md#presentation-query-port) (clarifying dialogue), [Cost-budget](../core/contracts.md#cross-cutting-contracts) & [Security/Policy](../core/contracts.md#cross-cutting-contracts) ports.
- **Engines:** [Planning Engine](../engineering/planning-engine.md) (sequences elicitation).
- **Driven by:** the [Execution Engine](../core/execution-engine.md) per the [Agent Runtime Protocol](../core/agent-runtime-protocol.md).

## Failure modes

| Failure | Effect | Mitigation / degradation |
|---------|--------|--------------------------|
| **Vague/contradictory intent** | Cannot form testable Requirements. | Surface clarifying questions; return *needs-human* rather than guess ([P10](../foundation/principles.md)). |
| **Reasoning invalid/unavailable** | No structured proposal. | Validate/repair at the seam; persistent failure → *failed* with diagnostic; FSM routes recovery. |
| **Over-generation** (requirements not grounded in intent) | Scope creep. | Each Requirement must bind to a source; ungrounded proposals are rejected at validation. |
| **Hidden conflict** between requirements | Downstream infeasibility. | Conflict detection flags pairs; unresolved conflicts block the phase gate (FSM-owned) and escalate. |

## Future improvements

- Requirement templates per product archetype (sensor node, motor driver, power supply) seeded from the [Learning Engine](../engineering/learning-engine.md).
- Quantitative completeness scoring of a requirement set before allowing the phase gate.
- Bi-directional sync with external requirements-management tools (deferred to [integration](../core/contracts.md)).

## Two-part split (P8)

| Half | In the Requirement Agent |
|------|---------------------------|
| **Deterministic engineering use-case** | Reads Design Intent and prior-art context; runs the [Planning Engine](../engineering/planning-engine.md) to sequence elicitation; deduplicates and conflict-checks candidate Requirements; validates each against the [domain model](../foundation/engineering-domain-model.md) invariants (testable, sourced, categorized); proposes `Requirement` capability invocations with justifying [Decisions](../foundation/engineering-domain-model.md#decision). |
| **Reasoning adapter** | Given the intent text and a strict **output schema** ("list of candidate requirements: statement, category, priority, acceptance criterion, source"), asks the [Reasoning Engine port](../core/reasoning-engine-interface.md) to extract and phrase testable Requirements and to propose clarifying questions. Returns *candidates only*. |
| **The seam** | The use-case validates each candidate (schema + domain: is it testable? sourced? non-duplicate?) before any commit. Unvalidated text never becomes a Requirement ([P3](../foundation/principles.md)). |

## FSM cross-link (+ engines used)

- **Phase / state machine:** [Requirement Planning](../state-machines/requirement-planning.md) — owns the states, transitions, phase events, rollback, recovery, and persistence for this phase. This agent is its driver.
- **Engines used:** [Planning Engine](../engineering/planning-engine.md).
- **Primary IR produced:** [Requirement IR](../compiler/ir/requirement-ir.md).

## Related documents

[`agents/README.md`](README.md) · [`state-machines/requirement-planning.md`](../state-machines/requirement-planning.md) · [`agents/planning-agent.md`](planning-agent.md) · [`engineering/planning-engine.md`](../engineering/planning-engine.md) · [`compiler/ir/requirement-ir.md`](../compiler/ir/requirement-ir.md) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md#requirement) · [`core/agent-runtime-protocol.md`](../core/agent-runtime-protocol.md)
