# ADR-0012 — Event-sourced file log as the Phase-1 persistence substrate

**Status:** Accepted (Phase 1)

## Context
The Phase-1 exit criterion requires that a phase's recorded history **replays to identical
state**. Per [ADR-0004](0004-event-sourcing-decision.md), state is the fold of an event log.

## Decision
Persist the event log as an **append-only JSON-lines file** (`FileEventLog`, one
`EventRecord` per line) behind the [Event-log port](../core/contracts.md). Engineering State
is an in-memory projection rebuilt by folding the log; `replay` re-folds the recorded
history without calling the model or reading the clock.

## Consequences
- The simplest substrate that demonstrates the exit criterion; the event log is the single
  source of truth and is human-inspectable.
- Entity ids are minted at record time and carried in events; timestamps are recorded and
  never re-read on replay — both prerequisites for byte-identical reconstruction (P4).
- An embedded database (state/checkpoint stores) can replace the file later behind the same
  port without touching the kernel.

## Alternatives considered
- **Embedded DB (SQLite/redb)** now — more machinery than Phase 1 needs to prove replay.
- **In-memory only** — would not demonstrate durable replay across processes.
