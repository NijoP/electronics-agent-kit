# Vision

> **Ring:** foundation. The "north star" document. Every other document should be reconcilable with what is written here.

## What we are building

**Electronics Agent Kit is an AI-native Engineering IDE for PCB and electronics design.** It is an **Engineering Runtime**: a deterministic kernel that orchestrates the full electronics design lifecycle — from a one-line product idea to manufacturable outputs — by coordinating many specialized engineering [state machines](../state-machines/README.md) over a single, shared, versioned [Engineering State](../core/shared-state-model.md).

The product is best understood as the fusion of three proven ideas, applied to electronics engineering for the first time:

- **Like Cursor** — an AI-native editor where intelligent agents work *with* the engineer inside the tool, not in a separate chat window.
- **Like Git** — every change to the design is versioned, branchable, mergeable, and attributable, with a complete history and the *why* behind every decision.
- **Like Unreal Engine** — a high-performance runtime that owns a rich domain model and orchestrates many subsystems in a coherent loop, with a powerful editor on top.

## What we are explicitly NOT building

- **Not another PCB editor.** Existing editors are manual drawing tools. We are a runtime that *reasons about* and *drives* the design; an editor surface is one view onto it.
- **Not another AI chatbot.** The intelligence is embedded in an engineering runtime that owns knowledge and enforces correctness — not a conversational wrapper around a model.
- **Not a thin LLM wrapper.** Per [P3](principles.md), LLMs are *only reasoning engines* behind a [strict boundary](../core/reasoning-engine-interface.md). The durable engineering knowledge lives in the runtime, not in prompts.

## The core thesis

> **The runtime owns the engineering knowledge. LLMs are only reasoning engines.**

This single sentence drives the entire architecture. Engineering knowledge — requirements, constraints, components, decisions, provenance — is durable, versioned, verifiable, and reproducible because it lives in the runtime's [domain model](engineering-domain-model.md) and [stores](../data/storage.md). The LLM supplies judgement on demand; it never holds the source of truth. This is what separates an *engineering tool you can trust* from a *plausible-text generator*. See [ADR-0002](../decisions/0002-runtime-owns-knowledge-llm-as-reasoning-engine.md).

## What the runtime coordinates

The complete electronics design lifecycle, as a sequence of orchestrated phases (the canonical list and mapping is in [`architecture-views.md`](architecture-views.md)):

Requirement Planning → Engineering Analysis → Constraint Extraction → Datasheet Intelligence → BOM Planning → Schematic Planning → ERC Verification → PCB Floor Planning → Component Placement → Routing Planning → DRC Verification → DFM Verification → EMC Analysis → Manufacturing Generation — all continuously improved by the cross-cutting [Learning Engine](../engineering/learning-engine.md).

Every subsystem communicates through the shared [Engineering State](../core/shared-state-model.md); none owns a private copy of the truth.

## Why this matters

Electronics design is slow, expensive, and expertise-bound. Mistakes are caught late (at fabrication) and cost real money and weeks of schedule. An engineering runtime that reasons with the engineer, enforces correctness continuously, remembers every decision, and reuses past experience can compress design cycles dramatically — while keeping the human [in command](../engineering/human-in-the-loop.md) and producing fully [traceable](../core/provenance-and-traceability.md), auditable results.

## Design tenets (the felt qualities)

1. **Trustworthy.** Every output is explainable and traceable to a requirement and a decision.
2. **Reproducible.** The same inputs produce the same design ([P4](principles.md)).
3. **Correct by construction.** Constraints and verification are continuous, not a final gate.
4. **The engineer is in command.** The AI proposes; the engineer disposes ([P10](principles.md)).
5. **Extensible for years.** Clean rings ([P1](principles.md)) let phases, agents, engines, and integrations be added without disturbing the kernel.

## Success criteria for the architecture (Phase 0)

This Phase 0 architecture succeeds if a competent team can, from these documents alone:
- understand every subsystem, its responsibilities, and its boundaries;
- see how data and control flow end-to-end;
- find the justification for every significant decision;
- begin implementation in any ring without ambiguity about contracts; and
- extend the system without violating the [principles](principles.md).

## Roadmap

Phasing from this architecture to a shipping product is in [`roadmap.md`](roadmap.md).

## Related documents
[`README.md`](../README.md) · [`principles.md`](principles.md) · [`engineering-domain-model.md`](engineering-domain-model.md) · [`architecture-views.md`](architecture-views.md) · [`system-overview.md`](system-overview.md) · [`roadmap.md`](roadmap.md)
