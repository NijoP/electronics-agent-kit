# Vector Store

> **Ring:** Interface adapters (outer). The Vector Store is the **outer-ring [Adapter](../../GLOSSARY.md#adapter) that implements the [Vector Memory port](../../core/contracts.md#vector-memory-port)**. It persists and searches the embeddings that back the [Vector Memory capability](../../knowledge/vector-memory.md) — the "find things like this" recall substrate. It is a **derived, rebuildable index**, not a source of truth. **It names no vector database, embedding model, distance metric, or index type** ([P1](../../foundation/principles.md), Phase-0 rule); those are deferred technology choices.

---

## Why it exists

Much valuable engineering knowledge is *fuzzy* — "we've seen a power tree shaped like this before," "parts like this one" — which exact, structured queries cannot serve ([vector-memory capability](../../knowledge/vector-memory.md)). Semantic similarity retrieval needs a store shaped for **nearest-neighbour search over vectors**, an access model fundamentally different from entity lookup ([State Store](state-store.md)) or relational traversal ([Knowledge-Graph Store](knowledge-graph-store.md)) — which is why it is a distinct store ([storage taxonomy](../storage.md)). Crucially, it is **derived**: every vector is computed from canonical content and can be rebuilt, so the store can be re-platformed or re-embedded without losing knowledge.

## Responsibilities

**Owns:**
- **Durable persistence of embeddings** for indexable engineering content, each with a **back-reference to the canonical [Entity ID](../../core/shared-state-model.md)** so a hit resolves to a real entity.
- **Similarity search** — returning ranked candidates with **similarity scores** so consumers can threshold ([vector-memory capability](../../knowledge/vector-memory.md)).
- **Serving the [Vector Memory port](../../core/contracts.md#vector-memory-port) operations** — index item, similarity query, delete.
- **Rebuildability** — supporting full re-index from canonical sources after content change or technology change.

**Does NOT own:**
- **What "similar" means / what is indexable.** That is the [Vector Memory capability](../../knowledge/vector-memory.md) (inner ring); the store serves it.
- **Truth.** A similarity hit is a *candidate*, never a fact or state ([P3](../../foundation/principles.md), [P10](../../foundation/principles.md)); qualification is the [Knowledge Graph](../../knowledge/knowledge-graph.md)'s job and disposal is the engineer's.
- **Embedding *as judgement*.** Producing an embedding is a mechanical adapter operation; *reasoning* over results happens in an [Agent's](../../agents/README.md) reasoning half ([P3](../../foundation/principles.md)).
- **Canonical knowledge.** That is the [Knowledge-Graph Store](knowledge-graph-store.md) / [State Store](state-store.md); this is a derived index over them.
- **Storage / embedding technology.** Deferred ([P1](../../foundation/principles.md)).

## Conceptual data model

A collection of **indexed items**, each conceptually:

- an **embedding** (an opaque vector representation of some canonical content);
- a **back-reference to the canonical [Entity ID](../../core/shared-state-model.md)** (a design, [Part](../../foundation/engineering-domain-model.md#part), [Functional Block](../../foundation/engineering-domain-model.md#functional-block), [Learning Engine](../../engineering/learning-engine.md) lesson, or past [Violation](../../foundation/engineering-domain-model.md#violation)+fix);
- **scoping metadata** (tenant/project/visibility) so similarity never crosses an unauthorized boundary;
- **provenance of the embedding** (which content/version it was computed from), so staleness and [embedding drift](../../knowledge/vector-memory.md) are detectable.

Per [data-modeling](../data-modeling.md), the vector is *shape*, not *meaning*: the meaning always lives in the canonical entity the back-reference points to.

## Access port

Implements the **[Vector Memory port](../../core/contracts.md#vector-memory-port)** (defined by the [Vector Memory capability](../../knowledge/vector-memory.md)): *index item · similarity query · delete*. No operation names a metric, model, or index type — by design ([contract design rules](../../core/contracts.md)). Query cost is bounded via the [Cost-budget port](../../core/contracts.md) ([P13](../../foundation/principles.md)).

## Consistency

- **Eventually consistent with canonical sources, by design.** Because the index is derived, brief lag between a canonical change and its re-embedding is acceptable — the store is a *recall aid*, not truth, so staleness is recoverable, never corrupting.
- **Determinism is via recording, not via the index.** A similarity result is a boundary output; like a reasoning call it is **recorded** so a run [replays](../../core/determinism-and-reproducibility.md) deterministically ([ADR-0009](../../decisions/0009-determinism-and-replay-strategy.md)) even though similarity search itself is approximate.
- **Scoping is strict.** Visibility/tenant scoping is enforced so retrieval never leaks across boundaries ([Security/Policy port](../../core/contracts.md)).

## Lifecycle & retention

- **Built and maintained by indexing** as canonical content is created/changed (primarily by the [Learning Engine](../../engineering/learning-engine.md) and [Component Library](../../engineering/component-library.md)).
- **Fully rebuildable** from canonical sources — the safe recovery path for corruption, technology change, or **embedding drift** (re-embed and re-index; the capability contract is unaffected, [vector-memory failure modes](../../knowledge/vector-memory.md)).
- **Retention** tracks the lifetime of the content it indexes; deleting a source entity deletes its vector (via the port's *delete*).
- **Versions alongside [design branches](../design-version-control.md)** by the same Entity-ID keying, so a branch's similarity matches reflect that branch.

## Failure modes

- **Store unavailable.** Retrieval degrades to **no-candidates**; agents fall back to first-principles reasoning and structured [Knowledge Graph](../../knowledge/knowledge-graph.md) queries — never fabricated similarity ([failure taxonomy](../../core/failure-taxonomy-and-degraded-modes.md)).
- **Stale index** (source changed). The index is derived and rebuildable; staleness is recoverable, detectable via embedding provenance.
- **Embedding drift** (technology change shifts the vector space). Handled here by **re-indexing**; the inner-ring contract does not change ([vector-memory.md](../../knowledge/vector-memory.md)).
- **Low-score / irrelevant results.** Consumers threshold on score; below-threshold candidates are ignored, never forced into a decision ([P10](../../foundation/principles.md)).
- **Cross-tenant leakage risk.** Prevented by scoping ([Security/Policy port](../../core/contracts.md)); similarity never crosses an unauthorized boundary.

## Open decisions

- [ADR-0009](../../decisions/0009-determinism-and-replay-strategy.md) — recording approximate similarity results so a run replays deterministically.
- [ADR-0002](../../decisions/0002-runtime-owns-knowledge-llm-as-reasoning-engine.md) — similarity hits inform reasoning inputs; they are never truth or state.
- [ADR-0008](../../decisions/0008-design-version-control-model.md) — how vectors version alongside design branches.
- **Open (deferred):** the concrete embedding model, vector database, distance metric, and index type — a later-phase technology ADR ([P1](../../foundation/principles.md)).

## Related documents

[`core/contracts.md`](../../core/contracts.md) (Vector Memory port) · [`knowledge/vector-memory.md`](../../knowledge/vector-memory.md) (the capability it backs) · [`knowledge/knowledge-graph.md`](../../knowledge/knowledge-graph.md) (qualifies its candidates) · [`data/stores/knowledge-graph-store.md`](knowledge-graph-store.md) · [`engineering/learning-engine.md`](../../engineering/learning-engine.md) (primary client) · [`data/data-modeling.md`](../data-modeling.md) · [`data/storage.md`](../storage.md) · [`core/determinism-and-reproducibility.md`](../../core/determinism-and-reproducibility.md)
