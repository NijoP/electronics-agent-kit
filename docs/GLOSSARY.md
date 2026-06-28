# Glossary

This glossary is the single source of truth for terminology across all Electronics Agent Kit documentation. When a term is used in any other document, it means exactly what it means here. Entities (the nouns of the engineering domain) are defined canonically in [`foundation/engineering-domain-model.md`](foundation/engineering-domain-model.md); this glossary covers runtime, architecture, and process vocabulary and links to the domain model for entities.

> **Convention:** capitalized terms (e.g. *Engineering State*, *Phase*, *Agent*) are defined terms with precise meaning. Lowercase uses are informal.

---

## The word "planning" (disambiguation)

"Planning" is dangerously overloaded in this domain. We never use the bare word in normative text. Use one of these:

| Term | Meaning |
|------|---------|
| **Reasoning plan** | The short-horizon sequence of steps an [Agent](#agent) intends to take inside one [Phase](#phase) (produced with help from the [Reasoning Engine](#reasoning-engine)). Lives in the [Planning Engine](#planning-engine). |
| **Workflow plan** | The graph of [Phases](#phase) for an entire project, owned by the [Workflow Orchestrator](#workflow-orchestrator). |
| **Requirement Planning** | The first engineering [Phase](#phase): turning intent into structured requirements. |
| **Floor Planning** | The PCB phase that allocates board regions to functional blocks. |
| **Placement Planning / Routing Planning** | PCB phases that decide where parts go and how nets connect. |

When a doc says "the planner," it must qualify which of the above it means.

---

## Core architecture terms

### Engineering Runtime
The deterministic kernel of the product. It owns the [Engineering State](#engineering-state), sequences [Phases](#phase), runs [State Machines](#state-machine), dispatches [Agents](#agent), records [Events](#event), and enforces all [Contracts](#contract). It is the only component permitted to mutate engineering knowledge. See [`core/engineering-runtime.md`](core/engineering-runtime.md).

### Reasoning Engine
The abstraction (port) through which the system obtains stochastic judgement from a large language model. The runtime depends only on this port, never on a specific model or provider. "LLMs are only reasoning engines" is enforced here. See [`core/reasoning-engine-interface.md`](core/reasoning-engine-interface.md).

### Agent
A unit of engineering work bound to one or more [Phases](#phase). Every Agent has two clearly separated halves: a **deterministic engineering use-case** (manipulates [Engineering State](#engineering-state) via [Contracts](#contract), calls [Engines](#engine)) and a **reasoning adapter** (asks the [Reasoning Engine](#reasoning-engine) for judgement). Agents never persist or own knowledge themselves. See [`agents/README.md`](agents/README.md).

### Engine
A deterministic domain service that encodes reusable engineering logic, callable by many [Agents](#agent) and [Phases](#phase). The four engines are the [Constraint Engine](#constraint-engine), [Planning Engine](#planning-engine), [Verification Engine](#verification-engine), and [Learning Engine](#learning-engine). Engines contain *no* stochastic reasoning.

### Phase
One discrete stage of the engineering process (e.g. *Requirement Planning*, *Routing Planning*, *DRC Verification*). Each Phase is modeled as a [State Machine](#state-machine) and is driven by one or more [Agents](#agent). The full set of Phases is enumerated in [`foundation/architecture-views.md`](foundation/architecture-views.md).

### State Machine (FSM)
The formal model of a single [Phase](#phase): its States, Transitions, Events, rollback, recovery, and persistence semantics. The reusable *framework* for all state machines is in [`core/state-machine-framework.md`](core/state-machine-framework.md); the per-phase *instances* are in [`state-machines/`](state-machines/README.md).

### Execution Engine
The runtime component that actually runs a [State Machine](#state-machine): evaluates transitions, invokes [Agents](#agent), applies effects, and commits [Events](#event). It is *mechanism*; the [Workflow Orchestrator](#workflow-orchestrator) decides *what* runs and the [Scheduler](#scheduler) decides *when*. See [`core/execution-engine.md`](core/execution-engine.md).

### Workflow Orchestrator
Owns the [Workflow plan](#the-word-planning-disambiguation): the directed graph of [Phases](#phase) for a project, including branches, gates, and dependencies. See [`core/workflow-orchestration.md`](core/workflow-orchestration.md).

### Scheduler
Decides *when* runnable work executes, under concurrency, priority, and resource/cost budgets. See [`core/scheduler.md`](core/scheduler.md).

### Contract (Port)
An interface at a clean-architecture ring boundary that inverts a dependency. Inner rings define contracts; outer rings implement them as [Adapters](#adapter). The catalog of contracts is in [`core/contracts.md`](core/contracts.md).

### Adapter
An outer-ring implementation of a [Contract](#contract) — e.g. a concrete store implementing a repository port, or a model client implementing the [Reasoning Engine](#reasoning-engine) port.

### Capability
A named, schema-described action the runtime exposes to [Agents](#agent) (e.g. "create component", "run DRC", "query datasheet"). The full catalog, with permissions and side-effect declarations, is the [Capability Registry](#capability-registry). See [`core/capability-registry.md`](core/capability-registry.md).

### Capability Registry
The authoritative catalog of [Capabilities](#capability). An Agent may only act through registered capabilities.

---

## State, events, and time

### Engineering State
The single, versioned, authoritative model of everything known about a design within a [Project](#project): requirements, constraints, components, nets, placement, routing, BOM, verification results, decisions, and their provenance. Defined structurally in [`core/shared-state-model.md`](core/shared-state-model.md); its entities in [`foundation/engineering-domain-model.md`](foundation/engineering-domain-model.md).

### Event
An immutable, ordered record of something that happened (a state change, a decision, an agent action, a reasoning call). Events are the unit of [provenance](#provenance) and the basis of [deterministic replay](#determinism). Transported by the [Event Bus](#event-bus), persisted in the [Event Store](#event-store).

### Event Bus
The in-process transport that delivers [Events](#event) to subscribers. Transport only — persistence is the [Event Store's](#event-store) job. See [`core/event-bus.md`](core/event-bus.md).

### Checkpoint
A captured, restorable snapshot of [Engineering State](#engineering-state) at a point in time, used for recovery and rollback. Distinct from [Undo/Redo](#undoredo) (user-facing command history) and [Design Branch](#design-branch) (version control). All three are reconciled in [`core/checkpoint-system.md`](core/checkpoint-system.md).

### Undo/Redo
User-facing reversal of user-initiated commands. A presentation/interaction concept layered on top of [Events](#event); not the same as a [Checkpoint](#checkpoint).

### Design Branch
A divergent line of [Engineering State](#engineering-state) history, mergeable back — "Git for hardware." See [`data/design-version-control.md`](data/design-version-control.md).

### Determinism
The property that, given the same inputs and the same recorded [Reasoning Engine](#reasoning-engine) outputs, the runtime reproduces the same [Engineering State](#engineering-state) exactly. The strategy for achieving this despite stochastic reasoning is in [`core/determinism-and-reproducibility.md`](core/determinism-and-reproducibility.md).

### Provenance
The complete, queryable lineage of any engineering fact: which requirement, constraint, decision, agent, reasoning call, and external evidence produced it. See [`core/provenance-and-traceability.md`](core/provenance-and-traceability.md).

---

## Compiler / representations

### Intermediate Representation (IR)
A typed, serializable representation of the design at a [Phase](#phase) boundary. The [Engineering State](#engineering-state) is canonical; each IR is a projection/serialization of it suitable for one stage. The IRs are: [Requirement IR](compiler/ir/requirement-ir.md), [Engineering IR](compiler/ir/engineering-ir.md), [BOM IR](compiler/ir/bom-ir.md), [Schematic IR](compiler/ir/schematic-ir.md), [PCB IR](compiler/ir/pcb-ir.md), [Manufacturing IR](compiler/ir/manufacturing-ir.md). See [`compiler/compiler-ir.md`](compiler/compiler-ir.md).

### Lowering / Transformation
A defined, invariant-preserving conversion from one [IR](#intermediate-representation-ir) to the next (analogous to a compiler lowering pass). See [`compiler/transformations.md`](compiler/transformations.md).

---

## Engineering-domain terms

> Entity nouns (Component, Net, Pin, Footprint, Symbol, Requirement, Constraint, Decision, Violation, etc.) are defined canonically in [`foundation/engineering-domain-model.md`](foundation/engineering-domain-model.md). A few process terms used widely:

### Constraint
A machine-checkable restriction on the design (electrical, physical, thermal, manufacturing, regulatory). Managed by the [Constraint Engine](#constraint-engine). Distinguish *Constraint Extraction* (the [Phase](#phase) that derives constraints) from the [Constraint Engine](#constraint-engine) (the service that stores/resolves/checks them).

### Constraint Engine
The cross-cutting service that stores, resolves, and checks [Constraints](#constraint). See [`engineering/constraint-engine.md`](engineering/constraint-engine.md).

### Planning Engine
The deterministic service that produces and manages [Reasoning plans](#the-word-planning-disambiguation) for Agents. See [`engineering/planning-engine.md`](engineering/planning-engine.md).

### Verification Engine
The generic rule-evaluation / violation / waiver framework that *ERC*, *DRC*, and *DFM* phases specialize. See [`engineering/verification-engine.md`](engineering/verification-engine.md).

### Learning Engine
The cross-cutting service that captures reusable engineering experience to improve future reasoning and defaults. See [`engineering/learning-engine.md`](engineering/learning-engine.md). (Reusable *meta* intelligence about how we build the product goes to ECC, not here.)

### ERC / DRC / DFM / EMC
Electrical Rule Check / Design Rule Check / Design For Manufacturability / Electromagnetic Compatibility — verification and analysis [Phases](#phase). ERC, DRC, DFM are rule-check phases over the [Verification Engine](#verification-engine); EMC is analysis.

### BOM
Bill of Materials — the list of physical parts, quantities, and sourcing for a design. See [BOM IR](compiler/ir/bom-ir.md) and the [BOM Agent](agents/bom-agent.md).

### Datasheet Intelligence
The [Phase](#phase) and [Agent](agents/datasheet-agent.md) that extract structured engineering facts (parameters, pinouts, limits) from component datasheets into the [Knowledge Graph](#knowledge-graph).

### Physical Quantity
A typed value carrying a unit and tolerance (e.g. `3.3 V ±5 %`). The runtime treats quantities as first-class to prevent dimensional errors. See [`engineering/units-and-quantities.md`](engineering/units-and-quantities.md).

---

## Knowledge & memory

### Knowledge Graph
The capability for modeling and querying interconnected engineering facts (parts, parameters, relationships, standards). A *capability/port* — distinct from its [Knowledge-Graph Store](data/stores/knowledge-graph-store.md) adapter. See [`knowledge/knowledge-graph.md`](knowledge/knowledge-graph.md).

### Vector Memory
The capability for semantic similarity retrieval over engineering content (e.g. "find similar reference designs"). A *capability/port* — distinct from its [Vector Store](data/stores/vector-store.md) adapter. See [`knowledge/vector-memory.md`](knowledge/vector-memory.md).

---

## Persistence (stores)

| Store | Purpose | Doc |
|-------|---------|-----|
| **State Store** | Persists [Engineering State](#engineering-state). | [link](data/stores/state-store.md) |
| **Event Store** | Persists the ordered [Event](#event) log (system of record candidate). | [link](data/stores/event-store.md) |
| **Vector Store** | Backs [Vector Memory](#vector-memory). | [link](data/stores/vector-store.md) |
| **Knowledge-Graph Store** | Backs the [Knowledge Graph](#knowledge-graph). | [link](data/stores/knowledge-graph-store.md) |
| **Session Store** | Per-user/session interaction state. | [link](data/stores/session-store.md) |
| **Checkpoint Store** | Persists [Checkpoints](#checkpoint). | [link](data/stores/checkpoint-store.md) |
| **Project Store** | Project metadata and registry. | [link](data/stores/project-store.md) |
| **Artifact Store** | Generated outputs (Gerbers, reports, exports). | [link](data/stores/artifact-store.md) |

---

## Process & governance

### Project
The top-level container for one design effort: its [Engineering State](#engineering-state), history, sessions, and artifacts. See [`data/stores/project-store.md`](data/stores/project-store.md).

### Session
A bounded period of user interaction with a [Project](#project). See [`collaboration/multi-user-and-sessions.md`](collaboration/multi-user-and-sessions.md).

### Autonomy Level
The degree to which the system may act without human approval, from advisory to supervised to autonomous. Defined in [`engineering/human-in-the-loop.md`](engineering/human-in-the-loop.md).

### Waiver
An explicit, recorded acceptance of a [Violation](#erc--drc--dfm--emc) that would otherwise block progress. Managed by the [Verification Engine](#verification-engine).

### ADR (Architecture Decision Record)
A numbered, immutable record of one justified architectural decision. See [`decisions/README.md`](decisions/README.md).

### ECC
The external long-term memory for *reusable engineering-of-the-product intelligence* (patterns, conventions, prompts). Per project rule, such intelligence is saved to ECC, never embedded in these docs.
