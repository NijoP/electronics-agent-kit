# ADR-0006: Two-part agents, separated from state machines

> **Grounds:** [P8 — Agents Are Two-Part, Never God-Objects](../foundation/principles.md), [P7 — Mechanism, Policy, and Instance Are Separated](../foundation/principles.md), [P3 — LLMs Are Only Reasoning Engines](../foundation/principles.md). **Primary documents:** [`core/agent-runtime-protocol.md`](../core/agent-runtime-protocol.md), [`agents/README.md`](../agents/README.md), [`core/state-machine-framework.md`](../core/state-machine-framework.md).

## Status

Accepted.

## Context

The architecture review found a structural duplication and a god-object risk in the original plan: it was unclear whether an "agent" and a "phase state machine" were the same thing described twice, and an undivided agent threatened to become an object that spanned every ring — composing prompts, calling a model, deciding engineering outcomes, mutating state, persisting knowledge, and driving its own control flow all at once. Such an object would violate the [dependency rule](0001-adopt-clean-architecture-dependency-rule.md) (it would touch all rings), entangle deterministic logic with stochastic reasoning (defeating [determinism](0009-determinism-and-replay-strategy.md)), and own knowledge it must not own ([ADR-0002](0002-runtime-owns-knowledge-llm-as-reasoning-engine.md)).

Two distinct concerns were being conflated:

- **The process** of a [Phase](../state-machines/README.md): its states, legal transitions, rollback, recovery, and persistence — a control-flow concern.
- **The work** done within a phase: manipulating engineering state and asking for judgement — a use-case concern.

And within "the work," two more were conflated: deterministic domain logic and stochastic reasoning. We must decide a clean separation, once, so all 13 agents and 14 state machines share one shape.

## Decision

We separate concerns along two cuts.

**Cut 1 — Agent vs. State Machine (work vs. process).** A [State Machine](../state-machines/README.md) owns the *process* of a phase (States / Transitions / Events / Rollback / Recovery / Persistence); an [Agent](../agents/README.md) owns the *work* (Purpose / Inputs / Outputs / reasoning strategy / failure-of-reasoning). A transition's *effect* **invokes** an agent; the two cross-reference but never restate each other's fields ([P7](../foundation/principles.md), and the [anti-duplication rule](../CONVENTIONS.md)). They are different kinds of thing, not two names for one.

**Cut 2 — Every agent is two-part (deterministic use-case ‖ reasoning adapter).** Each agent is split into:

1. a **deterministic engineering use-case** — touches [Engineering State](../core/shared-state-model.md) only through [Contracts](../core/contracts.md), calls the [Engines](../GLOSSARY.md#engine), validates, and is the only half that may *commit*; and
2. a **reasoning adapter** — the only part that talks to the [Reasoning Engine port](../core/reasoning-engine-interface.md), assembling prompts from runtime-owned context and returning *proposals*.

The seam between domain logic and stochastic reasoning runs *between* these halves ([P8](../foundation/principles.md), [P3](../foundation/principles.md)) — never through the middle of one tangled object. Agents never persist or own knowledge ([ADR-0002](0002-runtime-owns-knowledge-llm-as-reasoning-engine.md)).

This decides a *structural pattern*; it names no agent framework or model (Phase 0).

## Consequences

### Positive
- **No god-objects, no duplication.** The agents-vs-FSM ambiguity the review flagged is resolved: process and work are distinct, with a fixed division of fields ([P7](../foundation/principles.md)).
- **The reasoning seam is clean and singular.** All stochasticity is confined to reasoning adapters, which is what makes the deterministic core deterministic and [replayable](0009-determinism-and-replay-strategy.md) ([P4](../foundation/principles.md)). The two-part split mirrors the [deterministic/stochastic split](../core/determinism-and-reproducibility.md) exactly.
- **Testability.** The deterministic half can be tested with stubbed reasoning; the reasoning half can be evaluated against recorded judgements — the basis of the [quality strategy](../quality/).
- **Reuse and uniformity.** All 14 state machines instantiate one [framework](../core/state-machine-framework.md); all 13 agents share one [protocol](../core/agent-runtime-protocol.md), so a contributor learns the shape once.

### Negative
- **More moving parts per phase.** A single conceptual "do the work" becomes a state machine plus an agent split into two halves — more files, more wiring than a monolithic agent.
- **Indirection on every action.** The reasoning half must hand validated proposals to the deterministic half to commit; convenient shortcuts (reason-and-write in one place) are forbidden.
- **Cross-referencing discipline.** Authors must keep agent and state-machine docs from restating each other, which requires care under the anti-duplication rule.

### Neutral
- Two agents intentionally span two adjacent phases each, and one agent (Learning) is cross-cutting and bound to no phase — the [phase map](../foundation/architecture-views.md) is the authority on this mapping, not a 1:1 assumption.
- The split makes the [autonomy](0010-human-in-the-loop-autonomy-levels.md) "propose vs. dispose" seam fall naturally on the use-case/adapter boundary.

## Alternatives considered

- **One unified "agent" that is also its own state machine.** Fewer concepts. *Rejected:* recreates the god-object spanning all rings, entangles deterministic and stochastic code, and reintroduces the very duplication the review called out.
- **Agent = state machine (merge the two).** Treat each phase as a single FSM with embedded work. *Rejected:* conflates process with work; rollback/recovery/persistence and reasoning strategy have genuinely different shapes and lifecycles and benefit from separate, reusable frameworks ([P7](../foundation/principles.md)).
- **Two-part split, but let the reasoning adapter also commit state.** Simpler call flow. *Rejected:* it would put a stochastic component on the write path, breaking [P3](../foundation/principles.md) and [determinism](0009-determinism-and-replay-strategy.md); only the deterministic half may commit.
- **Split agents by engineering subdomain only (no deterministic/reasoning cut).** Organizes by domain, not by stochasticity. *Rejected:* it leaves reasoning and deterministic logic interleaved inside each agent, which is exactly the seam determinism requires to be clean.

## Related documents

[`core/agent-runtime-protocol.md`](../core/agent-runtime-protocol.md) · [`agents/README.md`](../agents/README.md) · [`core/state-machine-framework.md`](../core/state-machine-framework.md) · [`core/reasoning-engine-interface.md`](../core/reasoning-engine-interface.md) · [`foundation/architecture-views.md`](../foundation/architecture-views.md) · [`foundation/principles.md`](../foundation/principles.md) (P7, P8) · [ADR-0002](0002-runtime-owns-knowledge-llm-as-reasoning-engine.md) · [ADR-0009](0009-determinism-and-replay-strategy.md)
