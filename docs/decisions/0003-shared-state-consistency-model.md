# ADR-0003: Shared Engineering State consistency & concurrency model

> **Grounds:** [P2 — The Runtime Owns the Knowledge](../foundation/principles.md), [P4 — Determinism by Default](../foundation/principles.md), [P5 — Everything Is Traceable](../foundation/principles.md). **Primary documents:** [`core/concurrency-and-consistency.md`](../core/concurrency-and-consistency.md), [`core/shared-state-model.md`](../core/shared-state-model.md), [`core/event-bus.md`](../core/event-bus.md).
>
> **Also referenced as:** some core documents link this decision under the filenames `0003-shared-state-consistency-model.md` and `0003-shared-state-consistency-model.md`. Those are aliases for **this** ADR (`0003-shared-state-consistency-model.md`); they describe the same single decision and should be reconciled to this file.

## Status

Accepted.

## Context

A runtime that orchestrates many engineering [Phases](../state-machines/README.md) over **one shared [Engineering State](../core/shared-state-model.md)** will, by design, have several units of work in flight at once: an analysis agent reasoning while the engineer edits a constraint; two phases the [scheduler](../core/scheduler.md) ran in parallel; a long simulation completing while routing advances. Without an explicit consistency model these interleave unpredictably, mutations race, and determinism collapses.

Two forces make this hard in *this* system specifically:

- **Reasoning is long and variable.** A unit of work may block on an LLM call for seconds. Any scheme that holds locks across that window would serialize the whole product and stall the engineer.
- **Engineering changes must never be silently lost or silently merged.** [P5](../foundation/principles.md) (traceability) and [P13](../foundation/principles.md) (no silent loss) forbid last-writer-wins; [P2](../foundation/principles.md)/[P10](../foundation/principles.md) forbid a data-structure merging *intent* into a design no human or agent decided.

We need one model — decided once — for how concurrent mutations are **isolated**, **ordered**, and **reconciled on conflict**, governing concurrency *within a single line of history* (cross-branch divergence is [ADR-0008](0008-design-version-control-model.md)).

## Decision

The Shared State Model uses a **single authoritative writer realized through an append-only ordered event log, with optimistic, scope-based concurrency for the units of work that feed it.** Four rules:

1. **Single-writer commit.** All mutations are serialized through the [Engineering Runtime](../core/engineering-runtime.md). Canonical state advances by appending to **one ordered [Event](../core/event-bus.md) log per [Project](../data/stores/project-store.md)**. There is exactly one commit point and exactly one authority.
2. **Optimistic, scoped units of work.** Concurrent producers (agents, phases, the UI) prepare proposed mutations *optimistically* over a **consistent read snapshot** at a known log sequence point, each declaring the **working set** (scope) it reads and intends to change. They hold no long locks; a slow [reasoning call](../core/reasoning-engine-interface.md) never blocks another phase.
3. **Validate-and-append at commit.** A proposal commits only if its read-set is still valid against the current log head (no conflicting intervening Event). Otherwise it is rejected and rebased/re-reasoned/retried, with bounded retries; on exhaustion it escalates loudly.
4. **Log order is the source of truth for ordering.** "Happened-before" is defined by position in the log, not by wall-clock time; time is recorded metadata, never a comparator.

Conflicts of *data* are rebased; conflicts of *intent* (two placements for one component) are surfaced as a [Decision](../foundation/engineering-domain-model.md#decision) to the responsible agent or — per [Autonomy Level](../engineering/human-in-the-loop.md) — to the human, never resolved by a merge algorithm.

This selects a *consistency approach*; the concrete store/engine remains deferred (Phase 0).

## Consequences

### Positive
- **One authority, one order.** Satisfies [P2](../foundation/principles.md) (sole mutator) and gives [determinism](0009-determinism-and-replay-strategy.md) the single stable order it requires ([P4](../foundation/principles.md)).
- **High concurrency without long locks.** The expensive work (reasoning, engine evaluation) happens optimistically *outside* the serialized section; only the cheap validate-and-append is serialized, so throughput stays high.
- **No silent loss or silent merge.** A losing unit never overwrites; it re-derives. Genuine disagreements become explicit, traceable decisions ([P5](../foundation/principles.md), [P13](../foundation/principles.md)).
- **The substrate is reused, not added.** The ordered log this model commits to is the same log [provenance](../core/provenance-and-traceability.md) and [replay](0009-determinism-and-replay-strategy.md) already need (see [ADR-0004](0004-event-sourcing-decision.md)).

### Negative
- **Optimism wastes work under contention.** A conflict can discard completed (sometimes costly) reasoning and force a re-run; hot entities can degrade toward serial throughput.
- **Conflict handling is real machinery.** Read-set tracking, rebase, re-reason, and escalation paths must be built and tested; this is more complex than a single global lock.
- **A single commit point is a potential bottleneck.** It is deliberately cheap, but it is still the one global serialization point per project.

### Neutral
- Wall-clock time is demoted to metadata everywhere; ordering reasoning must always go through log position.
- Whether the [Event Store](../data/stores/event-store.md) is the *sole* record or a checkpoint-plus-log hybrid is an orthogonal storage choice (see [ADR-0004](0004-event-sourcing-decision.md)/[ADR-0009](0009-determinism-and-replay-strategy.md)); the consistency contract holds regardless.

## Alternatives considered

- **Pessimistic locking** (lock entities for the duration of work). Simple, conflict-free. *Rejected:* agent + LLM work is long and variable; holding locks across reasoning serializes the system and risks deadlock across multi-entity engineering acts.
- **Last-writer-wins.** Trivial. *Rejected:* silently discards engineering changes — unacceptable under [P5](../foundation/principles.md) and [P13](../foundation/principles.md).
- **Full multi-writer CRDT merge.** Conflict-free, highly concurrent. *Rejected:* automatic merge of *engineering intent* can synthesize a design nobody decided, violating [P2](../foundation/principles.md)/[P10](../foundation/principles.md); merging intent is a *decision*, not a data-structure side effect.
- **Multiple independent writers (no single commit point).** Maximizes parallel writes. *Rejected:* defeats the single authoritative order that [determinism](0009-determinism-and-replay-strategy.md) and traceability depend on; reconciling several write streams reintroduces exactly the merge-of-intent problem above.

## Related documents

[`core/concurrency-and-consistency.md`](../core/concurrency-and-consistency.md) · [`core/shared-state-model.md`](../core/shared-state-model.md) · [`core/event-bus.md`](../core/event-bus.md) · [`core/determinism-and-reproducibility.md`](../core/determinism-and-reproducibility.md) · [`core/scheduler.md`](../core/scheduler.md) · [`foundation/principles.md`](../foundation/principles.md) · [ADR-0004](0004-event-sourcing-decision.md) · [ADR-0008](0008-design-version-control-model.md) · [ADR-0009](0009-determinism-and-replay-strategy.md)
