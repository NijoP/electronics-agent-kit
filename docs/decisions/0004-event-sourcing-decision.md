# ADR-0004: The event log is the system of record (event sourcing)

> **Grounds:** [P4 — Determinism by Default](../foundation/principles.md), [P5 — Everything Is Traceable](../foundation/principles.md), [P2 — The Runtime Owns the Knowledge](../foundation/principles.md). **Primary documents:** [`core/event-bus.md`](../core/event-bus.md), [`data/stores/event-store.md`](../data/stores/event-store.md), [`core/provenance-and-traceability.md`](../core/provenance-and-traceability.md).
>
> **Also referenced as:** several documents link this decision as `0004-event-sourcing-decision.md`. That is an alias for **this** ADR (`0004-event-sourcing-decision.md`) — the same single decision.

## Status

Accepted.

## Context

The runtime already commits every mutation as an ordered [Event](../core/event-bus.md) ([ADR-0003](0003-shared-state-consistency-model.md)), and the product makes hard promises that all depend on history: deterministic [replay](0009-determinism-and-replay-strategy.md) ([P4](../foundation/principles.md)), complete [provenance](../core/provenance-and-traceability.md) for every fact ([P5](../foundation/principles.md)), restorable [checkpoints](../core/checkpoint-system.md), and [design version control](0008-design-version-control-model.md).

That raises a foundational question that must be answered once, because everything downstream assumes the answer: **is the event log the *system of record* — the authoritative truth from which current state is derived — or merely an *audit log* written alongside an independently-authoritative current-state store?**

The distinction is not cosmetic. If current state is authoritative and events are a side-channel, the two can disagree, replay can diverge from "live" state, and the audit trail becomes a best-effort artifact rather than ground truth. For a tool whose entire value proposition is trustworthy, reproducible, explainable engineering output, an audit log that *might* match reality is worthless.

## Decision

**We adopt event sourcing: the ordered, append-only [Event](../core/event-bus.md) log is the system of record.** The current [Engineering State](../core/shared-state-model.md) is a *derived projection* — the deterministic fold of the event log — not an independent source of truth.

1. **Events are facts, append-only and immutable.** Nothing is updated or deleted in place; a change is a new Event, and a reversal is a *compensating* Event. History is never rewritten (the same discipline this repository's own [ADR process](README.md) follows).
2. **State is a fold of the log.** Current state = replaying events in order. Any [State Store](../data/stores/state-store.md) is a materialized projection/cache of the log, reconcilable to it, never a rival authority.
3. **[Checkpoints](../core/checkpoint-system.md) are an optimization, not the record.** A checkpoint is a snapshot keyed to a log sequence point that bounds replay cost; the log past the checkpoint remains authoritative, and a checkpoint inconsistent with the log is rejected.
4. **Multi-entity engineering acts commit as an atomic, contiguous set of Events** — readers see all or none.

This is an *architecture* decision about where truth lives; it selects no database, log technology, or serialization format (Phase 0).

## Consequences

### Positive
- **Determinism and replay are free consequences, not add-ons.** "Same log → same state" ([P4](../foundation/principles.md)) holds by construction because state *is* the log's fold ([ADR-0009](0009-determinism-and-replay-strategy.md)).
- **Provenance is total and trustworthy.** Every fact traces to the exact Events, [Decisions](../foundation/engineering-domain-model.md#decision), and [Evidence](../foundation/engineering-domain-model.md#evidence) that produced it ([P5](../foundation/principles.md)); the audit trail *is* the data, so it cannot drift from reality.
- **Time-travel, undo, recovery, and branching** all reduce to operations over one authoritative history ([checkpoints](../core/checkpoint-system.md), [undo/redo](../GLOSSARY.md#undoredo), [design version control](0008-design-version-control-model.md)).
- **Crash recovery is principled.** Consistent state is recovered by replaying to the head (accelerated by the nearest checkpoint).

### Negative
- **Read performance needs projections.** Querying current state means folding the log, so materialized read models/snapshots are required for responsiveness — added machinery to build and keep consistent.
- **Schema/representation evolution is harder.** Old events must remain interpretable forever (or be migrated under explicit, recorded discipline); event shape becomes a long-lived contract governed by [data versioning](../data/data-versioning-and-migration.md).
- **Storage grows monotonically.** An append-only log only gets longer; retention, compaction-via-checkpoint, and archival policies are required.
- **Mental-model shift.** Contributors must think in events and compensations, not in-place edits — a real learning curve.

### Neutral
- The log doubles as the consistency substrate ([ADR-0003](0003-shared-state-consistency-model.md)) and the provenance fabric — one structure, several guarantees.
- Whether the [Event Store](../data/stores/event-store.md) physically keeps the entire log forever or a checkpoint-plus-tail hybrid is a *storage* choice deferred to that store's document; the *system-of-record* semantics decided here hold either way.

## Alternatives considered

- **Audit log alongside an authoritative current-state store (state-oriented + change log).** Familiar; fast reads. *Rejected:* the two can diverge, replay would not be guaranteed to reproduce live state, and the audit trail would be best-effort — defeating [P4](../foundation/principles.md)/[P5](../foundation/principles.md), the product's reason to exist.
- **CRUD with in-place updates, no event history.** Simplest, fastest to build. *Rejected:* destroys history, makes provenance and deterministic replay impossible, and cannot support reliable undo/branch/recovery.
- **Snapshots only (periodic full-state dumps, no event log).** Gives rollback points. *Rejected:* coarse-grained, loses the *why* between snapshots (no per-change [Decision](../foundation/engineering-domain-model.md#decision)/provenance), and cannot reproduce intermediate states or support fine-grained replay.
- **Event sourcing with no snapshots.** Pure, simple model. *Rejected as the operational form:* unbounded replay cost makes recovery and large-project reads impractical; we keep events authoritative but add [checkpoints](../core/checkpoint-system.md) purely as an acceleration.

## Related documents

[`core/event-bus.md`](../core/event-bus.md) · [`data/stores/event-store.md`](../data/stores/event-store.md) · [`core/provenance-and-traceability.md`](../core/provenance-and-traceability.md) · [`core/checkpoint-system.md`](../core/checkpoint-system.md) · [`core/shared-state-model.md`](../core/shared-state-model.md) · [`foundation/principles.md`](../foundation/principles.md) · [ADR-0003](0003-shared-state-consistency-model.md) · [ADR-0009](0009-determinism-and-replay-strategy.md) · [ADR-0008](0008-design-version-control-model.md)
