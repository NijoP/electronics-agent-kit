# ADR-0009: Determinism via deterministic core + recorded reasoning + replay

> **Grounds:** [P4 — Determinism by Default](../foundation/principles.md), [P3 — LLMs Are Only Reasoning Engines](../foundation/principles.md), [P5 — Everything Is Traceable](../foundation/principles.md). **Primary documents:** [`core/determinism-and-reproducibility.md`](../core/determinism-and-reproducibility.md), [`core/event-bus.md`](../core/event-bus.md), [`core/concurrency-and-consistency.md`](../core/concurrency-and-consistency.md).

## Status

Accepted.

## Context

The product's value proposition is *trustworthy, auditable, reproducible* engineering output — the difference between a design that is "plausible" and one that is defensible. Reproducibility is what lets an engineer reconstruct exactly how a state came to be, lets quality testing pin behaviour, and lets [version control](0008-design-version-control-model.md) treat history as solid ground.

But the system is built on a stochastic component: a large language model whose output can vary run-to-run and provider-to-provider. There is an apparent contradiction — *a deterministic system built on a non-deterministic engine* — and it must be resolved architecturally, not hoped away. The other sources of non-determinism (wall-clock time, randomness, external I/O such as simulations, parts data, and datasheet extraction) compound the problem.

We must decide, once, *how* the runtime delivers determinism despite all of this.

## Decision

We resolve the contradiction with one idea: **isolate, capture, and record all non-determinism at its boundary, then replay from the record.**

1. **A deterministic core.** The runtime's validate-and-commit logic, the [Engines](../GLOSSARY.md#engine), the agent **deterministic use-cases** ([ADR-0006](0006-agent-fsm-separation.md)), the [state-machine framework](../core/state-machine-framework.md), and the fold of the [event log](0004-event-sourcing-decision.md) into state are *all deterministic* — no ambient clock reads, no unseeded randomness, no unordered iteration. Any such leak is a defect the architecture forbids and quality tests for.
2. **All non-determinism captured at a thin boundary.** Every non-deterministic input — LLM judgement (via the [Reasoning Engine port](../core/reasoning-engine-interface.md)), external analysis/parts/datasheet I/O, time, randomness — is recorded as (or alongside) an [Event](../core/event-bus.md) at the moment it enters: the request, the output/result, the model identity/version, decisive parameters, the timestamp value, the seed.
3. **Seeded randomness.** Legitimate pseudo-randomness (e.g. a placement/routing heuristic) draws only from a generator whose **seed is recorded**, so the "random" exploration retraces identically on replay.
4. **Replay from the ordered log.** Given the same log, replay re-runs the deterministic effects in order, **serving recorded outputs for every boundary call instead of contacting any provider**, and reproduces identical [Engineering State](../core/shared-state-model.md). Stable [Entity IDs](../foundation/engineering-domain-model.md) let every reference re-bind. [Checkpoints](../core/checkpoint-system.md) accelerate replay but never replace the log.
5. **The contract, stated honestly:** *same log + same core version + recorded boundary outputs → identical state.* Re-running with *live* reasoning is a *new* recorded history, not a replay; recorded external facts are point-in-time.

This decides a *strategy*; it names no model, RNG, or storage technology (Phase 0).

## Consequences

### Positive
- **Reproducibility despite stochasticity.** Once a creative reasoning result is recorded, the design that follows from it is fully reproducible ([P4](../foundation/principles.md)) — turning AI output from plausible into auditable.
- **Determinism is architectural, not accidental.** It falls out of confining stochasticity to the reasoning boundary ([ADR-0002](0002-runtime-owns-knowledge-llm-as-reasoning-engine.md)) and the two-part agent split ([ADR-0006](0006-agent-fsm-separation.md)), riding the single ordered log ([ADR-0003](0003-shared-state-consistency-model.md)/[ADR-0004](0004-event-sourcing-decision.md)).
- **Enables audit, time-travel, rollback, version control, and deterministic tests** — all of which reduce to replay over a recorded history ([P5](../foundation/principles.md)).
- **Creativity is preserved.** First-run reasoning is as free as the model allows; only *reproduction* is deterministic — the tension between creativity and determinism is resolved by recording, not by suppression.

### Negative
- **Pervasive recording obligation.** Every boundary must faithfully record its inputs/outputs; a single unrecorded source of non-determinism makes replay diverge. This is a strict, system-wide discipline.
- **Storage and fidelity cost.** Full reasoning records (requests, outputs, parameters) are large and must be durable; the log only grows ([ADR-0004](0004-event-sourcing-decision.md) trade-offs apply).
- **Core-version sensitivity.** Replay fidelity is contracted against a core version; changing the deterministic core's own logic can change replay outcomes, so cross-version replay is itself a versioning concern.
- **"It must be deterministic" constrains implementation.** No unordered collections on the write path, no live clock reads, no ambient RNG — real constraints on how the core may be built.

### Neutral
- Replay reproduces from records; it does not re-derive new judgement — a deliberate, stated boundary, not a limitation to hide ([P13](../foundation/principles.md)).
- External fact drift (a part going EOL) correctly makes a *live re-run* differ from a *replay*; the two are distinct operations by design.

## Alternatives considered

- **Accept non-determinism (re-run the model on demand).** Simplest. *Rejected:* destroys reproducibility, audit, and stable version control — the product's core promises.
- **Force determinism by constraining the model** (e.g. greedy/zero-temperature decoding everywhere). *Rejected:* not reliably reproducible across providers/versions, and it sacrifices the judgement quality the model is used for; record-and-replay gets reproducibility *without* dulling first-run reasoning.
- **Snapshot-only reproducibility (save full state often, no replay).** Gives restore points. *Rejected:* cannot reconstruct *intermediate* states or the *why*, and stores whole states instead of compact events; it is rollback, not reproducibility.
- **Cache model responses without integrating them into an ordered event history.** Cheap memoization. *Rejected:* a detached cache is not keyed to an authoritative order or to per-Decision provenance, so it cannot guarantee that replay reconstructs the exact same state.

## Related documents

[`core/determinism-and-reproducibility.md`](../core/determinism-and-reproducibility.md) · [`core/reasoning-engine-interface.md`](../core/reasoning-engine-interface.md) · [`core/event-bus.md`](../core/event-bus.md) · [`core/concurrency-and-consistency.md`](../core/concurrency-and-consistency.md) · [`core/checkpoint-system.md`](../core/checkpoint-system.md) · [`foundation/principles.md`](../foundation/principles.md) (P4) · [ADR-0002](0002-runtime-owns-knowledge-llm-as-reasoning-engine.md) · [ADR-0003](0003-shared-state-consistency-model.md) · [ADR-0004](0004-event-sourcing-decision.md) · [ADR-0006](0006-agent-fsm-separation.md)
