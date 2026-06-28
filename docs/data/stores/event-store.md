# Event Store

> **Ring:** Interface adapters (outer). The Event Store is the **outer-ring [Adapter](../../GLOSSARY.md#adapter) that implements the persistence half of the [Event Sink/Source port](../../core/contracts.md#event-sink--event-source)** (the [Event Bus](../../core/event-bus.md) implements the transport half). It persists the **ordered, immutable log of [Events](../../core/event-bus.md)** — every state change, decision, agent action, and reasoning call — and is the **candidate system of record** ([ADR-0004](../../decisions/0004-event-sourcing-decision.md)) that makes [deterministic replay](../../core/determinism-and-reproducibility.md) and full [provenance](../../core/provenance-and-traceability.md) possible. **It names no log/database technology** ([P1](../../foundation/principles.md), Phase-0 rule).

---

## Why it exists

[P4 (determinism)](../../foundation/principles.md) and [P5 (everything traceable)](../../foundation/principles.md) are only achievable if *what happened* is recorded as an ordered, immutable history that can be replayed and queried. The Event Store is that history. It exists separately from the [State Store](state-store.md) because history is **append-only and immutable** while state is **mutable and materialized** — opposite access shapes ([storage taxonomy](../storage.md)). It is the foundation on which checkpoints, design branches, and provenance all rest: every one of them is, ultimately, a view over this log.

> **System-of-record candidate.** Whether this log is *the* system of record (full event sourcing, current state derived from it) or a forward audit/replay trail alongside a state-of-record is **the** central persistence decision, recorded in [ADR-0004](../../decisions/0004-event-sourcing-decision.md). This document is written so the store is correct under either resolution; under event sourcing it is canonical truth, under state-of-record it is the authoritative *audit* trail.

## Responsibilities

**Owns:**
- **Durable, append-only persistence of the [Event](../../core/event-bus.md) log** in a **total order** (a stable sequence position per event).
- **The [Event Source](../../core/contracts.md#event-sink--event-source) read operations** — read an ordered range, replay from a sequence point — that drive [replay](../../core/determinism-and-reproducibility.md), [checkpoint](checkpoint-store.md) tail-replay, and [branch](../design-version-control.md) materialization.
- **Immutability guarantee** — once appended, an event is never edited or deleted (corrections are *new compensating* events, [P5](../../foundation/principles.md)).
- **Recording boundary non-determinism** — captured [Reasoning Engine](../../core/reasoning-engine-interface.md) outputs, time, randomness, and external data are recorded as events so a run is reproducible ([P4](../../foundation/principles.md)).

**Does NOT own:**
- **Event transport / subscription delivery.** That is the [Event Bus](../../core/event-bus.md) (in-process transport); this store is persistence.
- **Event *meaning* / schema definitions.** Events describe changes to [domain-model](../../foundation/engineering-domain-model.md) entities; the store persists, it does not define.
- **Materialized current state.** The [State Store](state-store.md).
- **Snapshots.** The [Checkpoint Store](checkpoint-store.md) (a derived optimization over this log).
- **Storage technology.** Deferred ([P1](../../foundation/principles.md)).

## Conceptual data model

An **ordered, immutable sequence of event records**. Each event (a [projection](../data-modeling.md) of a change to the canonical model) conceptually carries:

- a **monotonic sequence position** establishing total order;
- the **affected [Entity ID(s)](../../core/shared-state-model.md)** and the change, by reference (enabling per-entity provenance lookup);
- the justifying **[Decision](../../foundation/engineering-domain-model.md#decision)** reference for design-significant changes ([P5](../../foundation/principles.md));
- recorded **boundary outputs** where the event captures non-determinism ([reasoning call results, etc.](../../core/determinism-and-reproducibility.md));
- **version/provenance metadata** so historical events stay interpretable ([data-versioning](../data-versioning-and-migration.md)).

The model is **strictly append-only**: the log grows; it never mutates. Per-branch ordering keys on the same Entity-ID/version coordinate as the rest of the system ([design version control](../design-version-control.md)).

## Access port

Implements the persistence side of the **[Event Sink/Source port](../../core/contracts.md#event-sink--event-source)** (defined by the runtime core): *append · read range · replay from sequence point* (with *subscribe* served by the [Event Bus](../../core/event-bus.md) transport). Consumers speak domain vocabulary and never name the store ([contract design rules](../../core/contracts.md)).

## Consistency

- **Append is atomic and ordered.** Each event receives a unique, monotonic sequence position; the total order is the backbone of [deterministic](../../core/determinism-and-reproducibility.md) replay ([P4](../../foundation/principles.md)).
- **Immutability is absolute.** No in-place edit or delete — a correction is a *new* compensating event. This is what lets [data-versioning](../data-versioning-and-migration.md) migrate history **by reader, never by rewrite** (§4 there).
- **The log is the reconciliation anchor.** The [State Store](state-store.md) and [Checkpoint Store](checkpoint-store.md) must be reconcilable to this log by replay; under [ADR-0004](../../decisions/0004-event-sourcing-decision.md) it is the authority that settles divergence.
- **Ordering under concurrency** follows the [concurrency model](../../core/concurrency-and-consistency.md) ([ADR-0003](../../decisions/0003-shared-state-consistency-model.md)).

## Lifecycle & retention

- **Created** with a [Project](project-store.md); grows monotonically for the project's life.
- **Retention: effectively permanent for a source-of-record log** — the audit trail and the basis of replay must not be silently truncated ([P5](../../foundation/principles.md), [P13](../../foundation/principles.md)). Any compaction strategy (e.g. cold-tiering very old segments) is an explicit, stated policy that preserves the full ordered history, never a silent cap.
- **[Checkpoints](checkpoint-store.md) bound replay cost**, not log size: pruning checkpoints loses nothing because the log remains authoritative; pruning the *log* is governed strictly and recorded.
- **Evolves** under [data-versioning](../data-versioning-and-migration.md): historical events are interpreted through version-aware readers, keeping the trail authentic.

## Failure modes

- **Store unavailable.** The runtime cannot append (no progress on state changes) — it opens **safe/read-only mode** rather than mutating without recording ([P5](../../foundation/principles.md)); see [failure taxonomy](../../core/failure-taxonomy-and-degraded-modes.md).
- **Append failure mid-write.** The atomicity guarantee means a partially-written event is never visible; the runtime retries or halts — it never proceeds as if the change were recorded.
- **Ordering/gap detection.** Sequence positions are monotonic; a gap or out-of-order read is detected and treated as corruption, not silently skipped.
- **Corrupt segment.** Detected on read; recovery uses intact history plus the nearest valid [Checkpoint](checkpoint-store.md). The immutability invariant means corruption is detectable against the declared order.
- **Unbounded growth.** Addressed by stated retention/compaction policy ([P13](../../foundation/principles.md)) — never a silent truncation that would break replay or audit.

## Open decisions

- [ADR-0004](../../decisions/0004-event-sourcing-decision.md) — **the** system-of-record decision: is this log canonical (event sourcing) or the audit trail beside a state-of-record?
- [ADR-0009](../../decisions/0009-determinism-and-replay-strategy.md) — how recorded boundary outputs make replay deterministic.
- [ADR-0003](../../decisions/0003-shared-state-consistency-model.md) — event ordering under concurrency.
- [ADR-0008](../../decisions/0008-design-version-control-model.md) — per-branch history over the log.
- **Open (deferred):** the concrete log technology — a later-phase technology ADR ([P1](../../foundation/principles.md)).

## Related documents

[`core/contracts.md`](../../core/contracts.md) (Event Sink/Source) · [`core/event-bus.md`](../../core/event-bus.md) (transport half) · [`core/determinism-and-reproducibility.md`](../../core/determinism-and-reproducibility.md) · [`core/provenance-and-traceability.md`](../../core/provenance-and-traceability.md) · [`data/stores/state-store.md`](state-store.md) · [`data/stores/checkpoint-store.md`](checkpoint-store.md) · [`data/design-version-control.md`](../design-version-control.md) · [`data/data-versioning-and-migration.md`](../data-versioning-and-migration.md) · [`data/storage.md`](../storage.md) · [`decisions/0004-event-sourcing-decision.md`](../../decisions/0004-event-sourcing-decision.md)
