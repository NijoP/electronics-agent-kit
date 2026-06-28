# ADR-0001: Adopt clean-architecture rings and the dependency rule

> **Grounds:** [P1 — The Dependency Rule](../foundation/principles.md). **Primary documents:** [`core/contracts.md`](../core/contracts.md), [`README.md`](../README.md), [`foundation/architecture-views.md`](../foundation/architecture-views.md).

## Status

Accepted.

## Context

Electronics Agent Kit is intended to live for years, accrue many contributors, and survive the churn of the AI tooling landscape — model providers, UI frameworks, storage engines, and simulators will all change underneath it. The architecture review warned that without a single, enforceable rule about *who may depend on whom*, the codebase would drift into the usual entropy: the domain importing a UI widget, an engineering rule living inside a database adapter, a model-provider SDK leaking into the core. Once those dependencies exist they are nearly impossible to remove, and they make the system impossible to test, reproduce, or re-platform.

The product also makes specific promises — provider-independence ([P3](../foundation/principles.md)), determinism ([P4](../foundation/principles.md)), the UI holding no engineering rules ([P11](../foundation/principles.md)) — that are *only* achievable if the dependency direction is fixed and guarded. These promises cannot be bolted on later; they follow from where dependencies are allowed to point.

We need one organizing rule, decided up front, that every other document and (later) every module obeys.

## Decision

We organize the entire system as **concentric clean-architecture rings** and enforce the **Dependency Rule: source-level dependencies point only inward.** Outer rings may depend on inner rings; inner rings know nothing of outer rings.

The rings, innermost to outermost:

1. **Entities** — the [engineering domain model](../foundation/engineering-domain-model.md) and the [principles](../foundation/principles.md). Depends on nothing.
2. **Use cases / runtime** — the runtime [`core/`](../core/), the domain [`engineering/`](../engineering/) engines, the [`compiler/`](../compiler/), and the [`knowledge/`](../knowledge/) capabilities. Depends only on Entities and on each other's *contracts*.
3. **Interface adapters** — [`data/`](../data/), [`integration/`](../integration/), [`presentation/`](../presentation/). Implement inner contracts.
4. **Frameworks & drivers** — concrete technologies, deferred to a later phase.

Every cross-ring interaction is inverted through a **[Contract (port)](../core/contracts.md)**: the inner ring *defines* the interface it needs; the outer ring *implements* it as an [Adapter](../GLOSSARY.md#adapter). The core therefore depends on an abstraction it owns, never on an implementation. The directory layout *is* the ring structure, so the dependency rule is visible on disk and auditable by inspection.

This is an *architecture* decision, not a technology one: it fixes the shape of dependencies, not the choice of any framework, language, or library (those remain deferred per Phase 0).

## Consequences

### Positive
- **Provider/tech independence becomes structural.** Because the core depends only on contracts, swapping a model provider, store, simulator, or UI is an outer-ring change invisible to the domain — the precondition for [P3](../foundation/principles.md) and [P12](../foundation/principles.md).
- **Testability and determinism.** The inner rings can be exercised against stub adapters with no real I/O, which is what makes [deterministic replay](0009-determinism-and-replay-strategy.md) and quality testing feasible.
- **Drift is detectable.** A forbidden dependency (inner → outer) is a visible, reviewable violation rather than a subtle smell.
- **The docs compose into one specification.** Every later document can reference [`contracts.md`](../core/contracts.md) instead of re-inventing how to touch state, reasoning, or persistence.

### Negative
- **Indirection cost.** Every boundary crossing goes through a port, adding interfaces and ceremony that a quick-and-dirty design would skip.
- **Discipline tax.** Contributors must learn the ring rules and resist the convenient inward-pointing shortcut; this needs enforcement (review, and later, dependency checks).
- **Up-front design load.** Contracts must be defined before mechanisms, front-loading thought that a less principled approach would defer.

### Neutral
- The folder taxonomy is now load-bearing: moving a document between rings is an architectural act, not a cosmetic one.
- Some abstractions will have exactly one implementation for a long time; the inversion is still kept for the independence it guarantees.

## Alternatives considered

- **Layered (n-tier) architecture without dependency inversion.** Familiar and simple, but classic layering lets upper layers depend directly on lower concrete layers; the domain would end up depending on persistence and frameworks — exactly the coupling we must avoid. *Rejected:* does not deliver provider-independence or testable determinism.
- **No enforced rule ("pragmatic"/ball-of-mud).** Lowest friction early on. *Rejected:* the review identified this as the path to the god-objects and drift the whole documentation effort exists to prevent; coupling, once introduced, is effectively permanent.
- **Hexagonal / ports-and-adapters as a distinct model.** Functionally close to what we chose and fully compatible with it. *Rejected as a separate framing only:* clean-architecture rings give us the same port inversion plus an explicit inner-to-outer ordering of *several* rings that maps cleanly onto our foundation/core/adapters/frameworks split; we adopt ports-and-adapters *as* the boundary mechanism within the rings rather than as a competing top-level model.
- **Microservice decomposition up front.** Strong isolation between parts. *Rejected for Phase 0:* it imposes distribution, deployment, and consistency costs that conflict with a single shared [Engineering State](../core/shared-state-model.md) and the [determinism](0009-determinism-and-replay-strategy.md) goal; isolation is achieved by the ring/contract discipline without paying the distributed-systems tax.

## Related documents

[`foundation/principles.md`](../foundation/principles.md) (P1) · [`core/contracts.md`](../core/contracts.md) · [`foundation/architecture-views.md`](../foundation/architecture-views.md) · [`README.md`](../README.md) · [`GLOSSARY.md`](../GLOSSARY.md#contract-port)
