# ADR-0002: The runtime owns the knowledge; LLMs are only reasoning engines

> **Grounds:** [P2 — The Runtime Owns the Knowledge](../foundation/principles.md), [P3 — LLMs Are Only Reasoning Engines](../foundation/principles.md). **Primary documents:** [`core/reasoning-engine-interface.md`](../core/reasoning-engine-interface.md), [`core/engineering-runtime.md`](../core/engineering-runtime.md), [`core/shared-state-model.md`](../core/shared-state-model.md).

## Status

Accepted.

## Context

The dominant pattern for AI products is the "thin wrapper": the application is a prompt, and the model holds the knowledge in its weights and its context window. For an engineering tool that must be *trusted with real hardware decisions*, that pattern is disqualifying. Knowledge held in a prompt is ephemeral (it vanishes when the context rolls over), unversioned (you cannot diff or audit it), unverifiable (you cannot check it against domain rules), and non-reproducible (the same prompt may yield different answers tomorrow, or from a different provider). None of that is acceptable for artifacts an engineer will sign off and a factory will build.

At the same time, large language models are genuinely valuable: they supply *judgement* — ranking candidate parts, proposing a topology, interpreting a datasheet — that no deterministic algorithm provides. The architecture must capture that value without surrendering ownership of the truth to a stochastic, opaque, swappable component.

This is the product's central thesis and its core differentiator, so it must be fixed as an architectural decision rather than an implementation habit.

## Decision

The **[Engineering Runtime](../core/engineering-runtime.md) is the sole owner of all engineering knowledge** — entities, decisions, evidence, and provenance live in the runtime's [Engineering State](../core/shared-state-model.md) and stores, never inside an agent, a prompt, or the UI. **Large language models are confined to the role of *reasoning engine*:** they supply judgement only, reached through exactly one boundary, the **[Reasoning Engine port](../core/reasoning-engine-interface.md)**.

Concretely:

1. **Knowledge is durable runtime data, not prompt text.** Anything the system "knows" is a first-class entity in the [domain model](../foundation/engineering-domain-model.md), recorded as [Events](../core/event-bus.md) with [provenance](../core/provenance-and-traceability.md). Prompts are *composed by the runtime from this owned knowledge*; they are an output, not a store.
2. **The model is an external dependency behind a port.** The domain core has *zero* knowledge of any model, provider, protocol, or token concept ([P1](../foundation/principles.md)/[P12](../foundation/principles.md)). The concrete model client is a deferred outer-ring [Adapter](../GLOSSARY.md#adapter).
3. **Propose, don't commit.** Reasoning output is a *candidate*, never truth and never state. It is schema-validated and domain-validated before it may influence state, and only the deterministic core may commit it. *An agent may propose via reasoning; only the runtime commits.*

This decision selects an *approach* — runtime-owned knowledge plus a single reasoning boundary — and deliberately selects **no** model, provider, or vendor (Phase 0).

## Consequences

### Positive
- **Trust and auditability.** Because knowledge is owned, versioned, and provenance-linked, any fact in a design can be traced to the requirement and reasoning that produced it ([P5](../foundation/principles.md)) — the difference between "plausible" and "defensible" engineering output.
- **Determinism is reachable.** Confining all stochasticity to one recorded boundary is precisely what lets the rest of the system be deterministic and [replayable](0009-determinism-and-replay-strategy.md) ([P4](../foundation/principles.md)).
- **Provider-independence.** Swapping, combining, or routing between providers is an outer-ring concern; the product is not hostage to any single vendor's pricing, availability, or roadmap.
- **Safety.** Every model output is validated against domain rules before it can touch state, so "plausible nonsense" is caught at the boundary rather than shipped to a fab.

### Negative
- **More engineering than a wrapper.** Owning a domain model, state, events, and validation is far more work than calling an API with a clever prompt.
- **Context-assembly burden.** Because knowledge is not in the model, the runtime must actively retrieve and assemble the right context for each judgement request (via [knowledge](../knowledge/knowledge-graph.md) and [vector memory](../knowledge/vector-memory.md)) — a non-trivial, ongoing responsibility.
- **The model can't "just answer."** Every useful answer must be shaped into a schema and validated, which constrains how freely the model's fluency can be used.

### Neutral
- The reasoning boundary becomes the single most carefully instrumented seam in the system (recording, cost governance, redaction, validation all converge there).
- Improvements in raw model capability accrue to us only through the port; they do not change the architecture, only the quality of judgement behind it.

## Alternatives considered

- **Thin LLM wrapper (knowledge in the prompt/weights).** Fast to build, fluent demos. *Rejected:* unversioned, unauditable, non-reproducible, unverifiable — fatal for an engineering tool of record. This is the pattern the [vision](../foundation/vision.md) explicitly rejects.
- **Fine-tuned / domain-specialized model as the source of truth.** Bakes engineering knowledge into a model. *Rejected:* still opaque and unversioned at the fact level, couples the product to a specific model, and cannot provide per-fact provenance or deterministic replay.
- **Retrieval-augmented generation as the architecture (not just a technique).** Treats a vector index as the knowledge store and the model as the reasoner over it. *Rejected as the top-level model:* retrieval is a useful *input to* a reasoning request, but a similarity index is not an authoritative, versioned, invariant-checked engineering state; we keep retrieval as a [capability](../knowledge/vector-memory.md) feeding the port, not as the system of record.
- **Let agents hold their own state/knowledge.** Convenient locality. *Rejected:* it recreates the very fragmentation this ADR prevents and leads to the god-object agents [ADR-0006](0006-agent-fsm-separation.md) guards against; knowledge must be central and owned.

## Related documents

[`foundation/principles.md`](../foundation/principles.md) (P2, P3) · [`core/reasoning-engine-interface.md`](../core/reasoning-engine-interface.md) · [`core/engineering-runtime.md`](../core/engineering-runtime.md) · [`core/shared-state-model.md`](../core/shared-state-model.md) · [`core/contracts.md`](../core/contracts.md#reasoning-engine-port) · [ADR-0009](0009-determinism-and-replay-strategy.md) · [ADR-0006](0006-agent-fsm-separation.md)
