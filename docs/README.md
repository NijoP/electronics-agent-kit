# Electronics Agent Kit — Architecture Documentation

> **Phase 0 — Architecture.** This repository currently contains **documentation only**. No source code, no technology choices, no implementation. Every document here describes *what* a component is, *why* it exists, and *how it relates to the rest of the system* — never *which library or language* implements it. Technology selection is a later phase and is tracked only as open decisions in [`decisions/`](decisions/README.md).

Electronics Agent Kit is an **AI-native Engineering IDE for PCB and electronics design**. It is not a PCB editor and it is not a chatbot. It is an **Engineering Runtime** — a deterministic kernel that orchestrates many engineering state machines over a single shared, versioned engineering state. The runtime *owns* the engineering knowledge; large language models are used only as **reasoning engines** behind a strict boundary.

If you read nothing else, read these five documents in order:

1. [`foundation/vision.md`](foundation/vision.md) — what we are building and why.
2. [`foundation/principles.md`](foundation/principles.md) — the architectural laws every other document obeys.
3. [`foundation/engineering-domain-model.md`](foundation/engineering-domain-model.md) — the canonical vocabulary (the "Entities" ring). Everything references it.
4. [`core/contracts.md`](core/contracts.md) — the boundary ports that make the layering real.
5. [`foundation/architecture-views.md`](foundation/architecture-views.md) — the C4 views and the **canonical phase → state-machine → agent → engine map**.

---

## How this documentation is organized

The directory layout *is* the architecture. It follows the clean-architecture **dependency rule**: inner rings know nothing about outer rings. Source dependencies only ever point inward. (See [`foundation/principles.md`](foundation/principles.md).)

```
Entities  ─▶  Use Cases / Runtime  ─▶  Interface Adapters  ─▶  Frameworks & Drivers
foundation     core + engineering        data + integration       (deferred — no tech yet)
               + compiler + knowledge     + presentation
```

| Ring | Folder | What lives here | Depth |
|------|--------|-----------------|-------|
| Entities | [`foundation/`](foundation/) | Vision, principles, the canonical domain model, quality attributes, system + architecture views, roadmap. | Deep |
| Use cases / runtime | [`core/`](core/) | The runtime kernel: lifecycle, execution, contracts, shared state, concurrency, FSM framework, event bus, scheduler, orchestration, checkpointing, errors, the reasoning-engine port, the agent protocol, determinism, provenance. | Deep |
| Domain — compiler | [`compiler/`](compiler/) | The Intermediate Representations (IRs) and the transformations between them. | Deep |
| Domain — engines | [`engineering/`](engineering/) | The constraint, planning, verification, and learning engines, plus EE realities (units, component library, standards, human-in-the-loop). | Deep |
| Domain — knowledge | [`knowledge/`](knowledge/) | Knowledge-graph and vector-memory **capabilities** (ports), distinct from their stores. | Deep |
| Infrastructure — data | [`data/`](data/) | The persistence model, data modeling, versioning/migration, design version control, and the eight stores. | Solid |
| Infrastructure — integration | [`integration/`](integration/) | Plugin system, IPC, backend hosting, simulation interface, supply-chain/parts data. | Solid |
| Infrastructure — presentation | [`presentation/`](presentation/) | The frontend shell and every panel/viewer. The UI is presentation-only. | Solid |
| Cross-cutting | [`crosscutting/`](crosscutting/) | Security, logging/observability, configuration, performance, cost governance. | Solid |
| Cross-cutting | [`collaboration/`](collaboration/) | Multi-user, sessions, workspaces. | Solid |
| Cross-cutting | [`governance/`](governance/) | Data licensing/IP, safety/liability/ethics. | Solid |
| Cross-cutting | [`quality/`](quality/) | Testing/validation strategy and agent evaluation. | Solid |
| Instances | [`agents/`](agents/README.md) | One document per AI agent (13). Each agent is a deterministic engineering use-case plus a reasoning adapter. | Solid |
| Instances | [`state-machines/`](state-machines/README.md) | One document per engineering phase (14). Each owns its states/transitions/events. | Solid |
| Decisions | [`decisions/`](decisions/README.md) | Architecture Decision Records — every justified decision. | Per-ADR |

## Conventions

All documents follow a shared template, naming scheme, diagram standard, and ADR process described in [`CONVENTIONS.md`](CONVENTIONS.md). Shared terminology is defined once in [`GLOSSARY.md`](GLOSSARY.md) — in particular the heavily overloaded word **"planning"**, which has several distinct meanings disambiguated there.

The architecture has been audited; see the [`architecture-health-report.md`](architecture-health-report.md) for the dependency-graph analysis, clean-architecture boundary check, coupling/instability metrics, and the prioritized findings backlog.

## The one-paragraph mental model

A **Project** holds a single, versioned **Engineering State** (the [shared state model](core/shared-state-model.md)). The user advances the design through a sequence of **phases**, each modeled as a **state machine** ([`state-machines/`](state-machines/README.md)). Phases are sequenced by the [workflow orchestrator](core/workflow-orchestration.md) and run by the [execution engine](core/execution-engine.md). Within a phase, an **agent** ([`agents/`](agents/README.md)) does the work: its deterministic half manipulates the engineering state through [contracts](core/contracts.md) and calls **engines** ([constraint](engineering/constraint-engine.md), [planning](engineering/planning-engine.md), [verification](engineering/verification-engine.md)), while its reasoning half asks an LLM for judgement through the [reasoning-engine port](core/reasoning-engine-interface.md). Every state change is an event on the [event bus](core/event-bus.md), persisted to the [event store](data/stores/event-store.md), giving full [provenance](core/provenance-and-traceability.md) and [deterministic replay](core/determinism-and-reproducibility.md). Phase boundaries are crossed by lowering one [IR](compiler/compiler-ir.md) to the next.
