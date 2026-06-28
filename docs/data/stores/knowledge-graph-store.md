# Knowledge-Graph Store

> **Ring:** Interface adapters (outer). The Knowledge-Graph Store is the **outer-ring [Adapter](../../GLOSSARY.md#adapter) that implements the [Knowledge port](../../core/contracts.md#knowledge-port)**. It persists and serves the interconnected **engineering facts** that back the [Knowledge Graph capability](../../knowledge/knowledge-graph.md) — parts and parameters, standards/clauses, datasheet-derived facts, and the [Evidence](../../foundation/engineering-domain-model.md#evidence)/[provenance](../../core/provenance-and-traceability.md) web. **It names no graph database or query language** ([P1](../../foundation/principles.md), Phase-0 rule).

---

## Why it exists

Much engineering reasoning is *relational*: "active RoHS parts with this footprint?", "which requirement justifies this constraint?", "from this net, all connected pins → components → parts → thermal limits?" Those are multi-hop, pattern-matching questions a flat or purely entity-keyed store cannot answer efficiently. The Knowledge-Graph Store exists because **relationship traversal is a distinct access pattern** ([storage taxonomy](../storage.md)). It is a **source of truth for engineering *knowledge*** (facts *about* parts, standards, prior art) — distinct from the [State Store](state-store.md), which is the source of truth for *the design itself* ([knowledge-graph capability §4](../../knowledge/knowledge-graph.md)).

## Responsibilities

**Owns:**
- **Durable persistence of the engineering fact web** — typed nodes and typed, directed edges representing facts and their relationships.
- **Relational query** — pattern matching and multi-hop relationship traversal over the [Knowledge port](../../core/contracts.md#knowledge-port).
- **Holding the [provenance](../../core/provenance-and-traceability.md) fabric** — a natural home for [Provenance Links](../../foundation/engineering-domain-model.md#provenance-link) among [Requirements](../../foundation/engineering-domain-model.md#requirement), [Constraints](../../foundation/engineering-domain-model.md#constraint), [Decisions](../../foundation/engineering-domain-model.md#decision), and [Evidence](../../foundation/engineering-domain-model.md#evidence).
- **Enforcing that every fact is sourced** — persisting source/reliability so facts are attributable ([P5](../../foundation/principles.md)).

**Does NOT own:**
- **The fact model / query semantics conceptually.** Those are the [Knowledge Graph capability](../../knowledge/knowledge-graph.md) (inner ring); the store serves it.
- **The canonical design state.** That is the [State Store](state-store.md); the graph holds knowledge that *informs* the design, never a back-door to mutate it ([knowledge-graph §4](../../knowledge/knowledge-graph.md)).
- **Fact *extraction*.** Turning datasheets into facts is [Datasheet Intelligence](../../state-machines/datasheet-intelligence.md); the store is where extracted facts land.
- **Approximate similarity.** That is the [Vector Store](vector-store.md), the sibling adapter.
- **Stochastic reasoning.** The graph is queried deterministically; reasoning over results happens in agents ([P3](../../foundation/principles.md)).
- **Storage / graph technology.** Deferred ([P1](../../foundation/principles.md)).

## Conceptual data model

A **typed property graph** of engineering knowledge (a [projection](../data-modeling.md) of the canonical model's fact/relationship vocabulary):

- **Nodes** — [Parts](../../foundation/engineering-domain-model.md#part) and their parameters, standards/clauses, datasheet facts, lifecycle states, and references to canonical [entities](../../foundation/engineering-domain-model.md) by [Entity ID](../../core/shared-state-model.md).
- **Edges (first-class, typed, directed)** — "has parameter," "complies with," "alternate of," "justifies," "supported by," "derived from" — the [relationship-as-entity](../data-modeling.md) rule realized as graph edges.
- **Typed values** — parametric facts are [Physical Quantities](../../engineering/units-and-quantities.md) ([P9](../../foundation/principles.md)), never bare numbers.
- **Source/reliability on every fact** — for adjudicating conflicts and detecting staleness.

## Access port

Implements the **[Knowledge port](../../core/contracts.md#knowledge-port)** (defined by the [Knowledge Graph capability](../../knowledge/knowledge-graph.md)): *assert fact · query by pattern · traverse relationships*. These are structured, deterministic operations — given the same facts, the same query yields the same answer ([P4](../../foundation/principles.md)). Over-broad traversals are bounded via the [Cost-budget port](../../core/contracts.md) ([P13](../../foundation/principles.md)).

## Consistency

- **Deterministic reads.** Identical facts + identical query ⇒ identical answer ([P4](../../foundation/principles.md)) — essential because graph answers feed reasoning and become [Evidence](../../foundation/engineering-domain-model.md#evidence).
- **Conflicting facts are modeled, not overwritten.** When two sources disagree on a parameter, both are kept with source/reliability so a consumer or engineer can adjudicate ([knowledge-graph failure modes](../../knowledge/knowledge-graph.md)).
- **Provenance integrity.** Every asserted fact carries a source; unsourced assertions are rejected ([P5](../../foundation/principles.md)).
- **Reconciliation with design state.** Design-significant changes go through the [State Repository](state-store.md) and become [Events](../../core/event-bus.md); the graph reflects knowledge, and is updated as facts are asserted, not as a side-channel to state.

## Lifecycle & retention

- **Grows by assertion** from [Datasheet Intelligence](../../state-machines/datasheet-intelligence.md), [Component Library](../../engineering/component-library.md), [Standards & Compliance](../../engineering/standards-and-compliance.md), and the [Learning Engine](../../engineering/learning-engine.md).
- **Facts are refreshed, not silently mutated** — a lifecycle change upstream (e.g. a Part goes EOL) is a new sourced assertion; staleness is detectable via fact provenance.
- **Source of truth → retained**, evolving under [data-versioning & migration](../data-versioning-and-migration.md).
- **Versions alongside [design branches](../design-version-control.md)** by Entity-ID keying, so a branch's facts reflect that branch ([knowledge-graph open decisions](../../knowledge/knowledge-graph.md)).

## Failure modes

- **Store unavailable.** Queries become unanswerable; consumers treat missing facts as **indeterminate**, never fabricated ([failure taxonomy](../../core/failure-taxonomy-and-degraded-modes.md)).
- **Conflicting facts.** Modeled explicitly with source/reliability; surfaced for adjudication, never silently resolved.
- **Stale fact.** Detectable via source/provenance and refreshable; staleness is visible, not hidden.
- **Unsourced assertion.** Rejected at the port — every fact must carry a source ([P5](../../foundation/principles.md)).
- **Over-broad traversal / cost.** Bounded via the [Cost-budget port](../../core/contracts.md); no silent unbounded traversal ([P13](../../foundation/principles.md)).

## Open decisions

- [ADR-0002](../../decisions/0002-runtime-owns-knowledge-llm-as-reasoning-engine.md) — the graph is runtime-owned knowledge feeding reasoning, never a model's private memory.
- [ADR-0005](../../decisions/0005-ir-as-canonical-phase-boundary-representation.md) — relationship between graph facts and [IR](../../compiler/compiler-ir.md) projections.
- [ADR-0008](../../decisions/0008-design-version-control-model.md) — how facts version alongside design branches.
- **Open (deferred):** the concrete graph technology and query language — a later-phase technology ADR ([P1](../../foundation/principles.md)).

## Related documents

[`core/contracts.md`](../../core/contracts.md) (Knowledge port) · [`knowledge/knowledge-graph.md`](../../knowledge/knowledge-graph.md) (the capability it backs) · [`knowledge/vector-memory.md`](../../knowledge/vector-memory.md) (sibling capability) · [`data/stores/vector-store.md`](vector-store.md) · [`core/provenance-and-traceability.md`](../../core/provenance-and-traceability.md) · [`foundation/engineering-domain-model.md`](../../foundation/engineering-domain-model.md) (Evidence, Provenance Link) · [`state-machines/datasheet-intelligence.md`](../../state-machines/datasheet-intelligence.md) · [`data/data-modeling.md`](../data-modeling.md) · [`data/storage.md`](../storage.md)
