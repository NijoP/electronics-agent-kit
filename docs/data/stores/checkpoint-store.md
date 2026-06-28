# Checkpoint Store

> **Ring:** Interface adapters (outer). The Checkpoint Store is the **outer-ring [Adapter](../../GLOSSARY.md#adapter) that implements the [Checkpoint port](../../core/contracts.md#checkpoint-port)**. It persists restorable **snapshots of [Engineering State](../../core/shared-state-model.md)** captured at specific [Event](../../core/event-bus.md) sequence positions, so the [Checkpoint System](../../core/checkpoint-system.md) can reconstruct the runtime quickly via "nearest snapshot + tail replay." It is a **derived, disposable optimization** — never a source of truth. **It names no storage technology** ([P1](../../foundation/principles.md), Phase-0 rule).

---

## Why it exists

Reconstructing state by replaying the entire [Event Store](event-store.md) from origin is correct but slow. A checkpoint bounds recovery cost to "since the last snapshot." The Checkpoint Store exists to hold these snapshots durably. It is distinct from every other store because a snapshot is **a whole-state blob tied to a sequence position** — neither the mutable entity graph ([State Store](state-store.md)) nor the append-only log ([Event Store](event-store.md)) — and because it is **prunable without semantic loss** ([storage taxonomy](../storage.md)). This is the store the [Checkpoint System](../../core/checkpoint-system.md) writes through.

## Responsibilities

**Owns:**
- **Durable persistence of state snapshots**, each tied to a specific [Event](../../core/event-bus.md) **sequence position** plus capture metadata (trigger, phase, timestamp-as-data).
- **Serving the [Checkpoint port](../../core/contracts.md#checkpoint-port) operations** — capture (persist), list, restore (fetch), prune.
- **Integrity verification on load** — confirming a snapshot is consistent with its declared sequence position before it is trusted.
- **Bounded growth** — supporting a stated retention/prune policy that keeps recovery anchors ([P13](../../foundation/principles.md)).

**Does NOT own:**
- **The capture/restore/prune *policy and orchestration*.** That is the [Checkpoint System](../../core/checkpoint-system.md) (inner ring); the store is the durable medium.
- **The event history.** The [Event Store](event-store.md). A checkpoint *references* a sequence position; it never replaces the log.
- **Current live state.** The [State Store](state-store.md).
- **Undo/Redo or branching.** Different concepts ([checkpoint-system §§5–6](../../core/checkpoint-system.md)); a checkpoint carries no merge semantics.
- **Storage technology.** Deferred ([P1](../../foundation/principles.md)).

## Conceptual data model

A collection of **snapshot records**, each conceptually:

- the **captured [Engineering State](../../core/shared-state-model.md)** consistent at one sequence position (a [projection](../data-modeling.md) of the canonical state at that instant);
- the **sequence position** it is tied to — what makes "snapshot + tail replay" exact ([P4](../../foundation/principles.md));
- **capture metadata** — trigger (periodic / pre-risk / phase-boundary / shutdown), phase, recorded timestamp;
- an **integrity descriptor** for load-time validation;
- the **version/branch coordinate** ([design version control](../design-version-control.md)) the snapshot belongs to.

## Access port

Implements the **[Checkpoint port](../../core/contracts.md#checkpoint-port)** (defined by the runtime core): *capture · list · restore · prune*. The [Checkpoint System](../../core/checkpoint-system.md) consumes this port; it reads a consistent state at a sequence position from the [State Repository](../../core/contracts.md#state-repository) and persists it here, and on restore loads a snapshot as the base before the [Event Source](../../core/contracts.md#event-sink--event-source) replays the tail.

## Consistency

- **Snapshots are never torn.** Capture reads a state consistent at a sequence position per the [concurrency model](../../core/concurrency-and-consistency.md) ([ADR-0003](../../decisions/0003-shared-state-consistency-model.md)) — never a partial mid-mutation read.
- **A snapshot is re-derivable.** Given the [Event log](event-store.md), any checkpoint can be regenerated; losing all checkpoints only makes recovery slower, never impossible (when the log is authoritative — [ADR-0004](../../decisions/0004-event-sourcing-decision.md)).
- **Restore reconstructs, never edits.** Restore loads a base read; it does not modify history ([checkpoint-system §3](../../core/checkpoint-system.md)).
- **Works under either system-of-record resolution** ([ADR-0004](../../decisions/0004-event-sourcing-decision.md)): a pure cache under event sourcing, the durable base under state-of-record.

## Lifecycle & retention

- **Captured on triggers** — periodic (every N events), before an irreversible/side-effecting operation (e.g. a [manufacturing export](../../state-machines/manufacturing-generation.md)), at phase boundaries, and at clean shutdown ([checkpoint-system §3](../../core/checkpoint-system.md)).
- **Pruned under a stated retention policy** — keep enough to bound worst-case recovery time (e.g. anchors per completed phase) without unbounded growth ([P13](../../foundation/principles.md)).
- **Pruning loses nothing** because the [Event Store](event-store.md) retains the full authoritative history — the defining property that makes this store disposable.
- **Evolves** under [data-versioning](../data-versioning-and-migration.md): a snapshot is a versioned projection and stays interpretable across schema change.

## Failure modes

- **No valid checkpoint on recovery.** Fall back to replaying from the [log](event-store.md) origin (slower but correct); if neither is available, the [Runtime Lifecycle](../../core/runtime-lifecycle.md) opens safe/read-only mode rather than guessing ([failure taxonomy](../../core/failure-taxonomy-and-degraded-modes.md)).
- **Corrupt snapshot.** Detected on load via the integrity descriptor against its declared sequence position; **skipped in favour of the next-older valid snapshot**, then tail replay. A corrupt checkpoint is never trusted ([checkpoint-system §8](../../core/checkpoint-system.md)).
- **Capture during heavy mutation.** Never captures a torn state — consistent read at a sequence position ([concurrency model](../../core/concurrency-and-consistency.md)).
- **Storage pressure from snapshots.** Bounded by the stated prune policy ([P13](../../foundation/principles.md)); safe because the log remains authoritative.

## Open decisions

- [ADR-0004](../../decisions/0004-event-sourcing-decision.md) — sets whether checkpoints are a pure cache (event sourcing) or the durable base (state-of-record).
- [ADR-0009](../../decisions/0009-determinism-and-replay-strategy.md) — snapshot + tail-replay as deterministic reconstruction.
- [ADR-0003](../../decisions/0003-shared-state-consistency-model.md) — consistent capture under concurrency.
- [ADR-0008](../../decisions/0008-design-version-control-model.md) — checkpoints as efficient base states when materializing a [branch](../design-version-control.md).
- **Open (deferred):** the concrete snapshot storage technology — a later-phase technology ADR ([P1](../../foundation/principles.md)).

## Related documents

[`core/checkpoint-system.md`](../../core/checkpoint-system.md) (owns capture/restore/prune policy) · [`core/contracts.md`](../../core/contracts.md) (Checkpoint port) · [`data/stores/event-store.md`](event-store.md) · [`data/stores/state-store.md`](state-store.md) · [`data/design-version-control.md`](../design-version-control.md) · [`core/runtime-lifecycle.md`](../../core/runtime-lifecycle.md) · [`core/determinism-and-reproducibility.md`](../../core/determinism-and-reproducibility.md) · [`core/concurrency-and-consistency.md`](../../core/concurrency-and-consistency.md) · [`data/storage.md`](../storage.md)
