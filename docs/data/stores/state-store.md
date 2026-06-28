# State Store

> **Ring:** Interface adapters (outer). The State Store is the **outer-ring [Adapter](../../GLOSSARY.md#adapter) that implements the [State Repository port](../../core/contracts.md#state-repository)**. It persists the current [Engineering State](../../core/shared-state-model.md) — the single, versioned, authoritative model of everything known about a design — so the runtime can read and mutate it across restarts. **It names no database or storage technology** ([P1](../../foundation/principles.md), Phase-0 rule); what it persists is a [projection](../data-modeling.md) of the canonical [domain model](../../foundation/engineering-domain-model.md) ([P6](../../foundation/principles.md)). The store holds **no engineering rules** — those live in the runtime ([P1](../../foundation/principles.md), [P2](../../foundation/principles.md)).

---

## Why it exists

The runtime's thesis is that **it owns the engineering knowledge** ([P2](../../foundation/principles.md)). Owned knowledge must survive a process exit, so the materialized "now" of every design needs a durable home that can answer entity and relationship queries efficiently. The State Store is that home. It exists separately from the [Event Store](event-store.md) because the *current materialized state* (mutable, queryable by type/relationship) and the *history that produced it* (immutable, append-only, ordered) have opposite access shapes — collapsing them would force one model to serve both poorly ([storage taxonomy](../storage.md)).

> **Under [ADR-0004](../../decisions/0004-event-sourcing-decision.md):** if the event log is the system of record, this store is a *materialized projection* (a cache) re-derivable by replay; if state is the system of record, this store is *durable truth* with events as a forward audit delta. The store's responsibilities below hold under either resolution.

## Responsibilities

**Owns:**
- **Durable persistence of current Engineering State** — every [entity](../../foundation/engineering-domain-model.md) and first-class relationship, addressed by stable [Entity ID](../../core/shared-state-model.md).
- **Serving the [State Repository](../../core/contracts.md#state-repository) operations** — get entity, query entities by type or relationship, apply a validated mutation, open a transactional unit consistent with the [concurrency model](../../core/concurrency-and-consistency.md).
- **Honouring the version coordinate** — resolving entities at *(Entity ID, [branch](../design-version-control.md), point-in-history)*.

**Does NOT own:**
- **Engineering rules or validation.** The runtime validates mutations *before* they reach the store; the repository invariant — "never accept an unjustified design-significant change" — is enforced by the runtime, not the store ([shared-state-model write discipline](../../core/shared-state-model.md)).
- **Entity definitions.** Canonical in the [domain model](../../foundation/engineering-domain-model.md); the store persists projections ([P6](../../foundation/principles.md)).
- **History / events.** The [Event Store](event-store.md) owns the immutable log; this store holds the materialized result.
- **Snapshots.** Recovery snapshots are the [Checkpoint Store](checkpoint-store.md).
- **Storage technology.** Deferred ([P1](../../foundation/principles.md)).

## Conceptual data model

A [projection](../data-modeling.md) of the [Engineering State graph](../../core/shared-state-model.md): one connected web of identified entities, layered by abstraction. Per the [data-modeling discipline](../data-modeling.md):

- **Entities keyed by opaque, immutable Entity ID**; all references by ID, never by name/position.
- **Relationships are first-class, addressable** ([Connection](../../foundation/engineering-domain-model.md#connection), [Provenance Link](../../foundation/engineering-domain-model.md#provenance-link)), so Events and Decisions can attach to them.
- **Physical values stay [typed](../../engineering/units-and-quantities.md)** (magnitude + unit + tolerance), never flattened to bare numbers ([P9](../../foundation/principles.md)).
- **Three partitions** mirrored from the [shared state model](../../core/shared-state-model.md): *design content* (mutated only via justified Decisions), *reasoning & provenance* (append-mostly), *derived/projected* (recomputable; never authoritative). The store may persist derived data for speed but marks it as rebuildable.

## Access port

Implements the **[State Repository port](../../core/contracts.md#state-repository)** (defined by the runtime core): *get entity · query entities by type/relationship · apply validated mutation · open a transactional unit*. Consumers speak only [domain vocabulary](../../foundation/engineering-domain-model.md) through this port and can never name the store ([contract design rules](../../core/contracts.md)). The store also backs the read-only projections served via the [Presentation/Query port](../../core/contracts.md#presentation--query-port).

## Consistency

- **All writes are validated, justified, and expressed as [Events](../../core/event-bus.md)** by the runtime *before* persistence; the store applies them atomically per the [concurrency model](../../core/concurrency-and-consistency.md) ([ADR-0003](../../decisions/0003-shared-state-consistency-model.md)).
- **Mutations are entity-grained and atomic** — a single engineering act spanning several entities is all-or-nothing ([shared-state granularity](../../core/shared-state-model.md)).
- **State and log agree.** Whichever is the system of record ([ADR-0004](../../decisions/0004-event-sourcing-decision.md)), the materialized state must be reconcilable with the [Event Store](event-store.md) by replay; divergence is a detectable, recoverable fault (re-materialize from the log).
- **Derived data is recomputable**, invalidated on the Events that change its inputs and regenerated deterministically ([P4](../../foundation/principles.md)).

## Lifecycle & retention

- **Created** with a [Project](project-store.md); lives as long as the project does.
- **Versioned**, not overwritten in place: entities are created/enriched/superseded/retired with provenance links ([entity lifecycle](../../foundation/engineering-domain-model.md)), so historical version coordinates remain resolvable.
- **Evolves** under [data-versioning & migration](../data-versioning-and-migration.md) — schema changes are projections of canonical-model changes; old records stay readable.
- **Retention** is the project's lifetime; retired entities leave tombstones (never silent deletion, [P5](../../foundation/principles.md)/[P13](../../foundation/principles.md)).

## Failure modes

- **Store unavailable.** The runtime cannot read/mutate state; it opens **safe/read-only mode** rather than guessing ([failure taxonomy](../../core/failure-taxonomy-and-degraded-modes.md)); on return, reconcile against the [Event Store](event-store.md).
- **State/log divergence** (materialized state disagrees with history). Detected by reconciliation; resolved by **re-materializing from the authoritative log** ([ADR-0004](../../decisions/0004-event-sourcing-decision.md)).
- **Corruption of materialized state.** Recoverable when the log is authoritative (replay/checkpoint-restore); otherwise restored from the nearest valid [Checkpoint](checkpoint-store.md) + tail replay.
- **Dangling reference.** Prevented by immutable IDs + tombstones; resolution degrades to "superseded by X," never a silent null ([shared-state failure modes](../../core/shared-state-model.md)).
- **Attempted out-of-band write.** Architecturally impossible — there is no write path except the runtime via the repository ([P2](../../foundation/principles.md)).

## Open decisions

- [ADR-0004](../../decisions/0004-event-sourcing-decision.md) — system of record: is this store canonical truth or a materialized projection of the event log?
- [ADR-0003](../../decisions/0003-shared-state-consistency-model.md) — consistency/concurrency model the store honours.
- [ADR-0008](../../decisions/0008-design-version-control-model.md) — version-coordinate (branch/history) addressing of entities.
- **Open (deferred):** the concrete storage technology — a later-phase technology ADR ([P1](../../foundation/principles.md)).

## Related documents

[`core/contracts.md`](../../core/contracts.md) (State Repository) · [`core/shared-state-model.md`](../../core/shared-state-model.md) · [`foundation/engineering-domain-model.md`](../../foundation/engineering-domain-model.md) · [`data/stores/event-store.md`](event-store.md) · [`data/stores/checkpoint-store.md`](checkpoint-store.md) · [`data/storage.md`](../storage.md) · [`data/data-modeling.md`](../data-modeling.md) · [`core/concurrency-and-consistency.md`](../../core/concurrency-and-consistency.md) · [`foundation/principles.md`](../../foundation/principles.md)
