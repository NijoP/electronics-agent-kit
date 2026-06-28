# Project Explorer

> **Ring:** Interface adapters — presentation (outer). The project explorer is the [IDE shell](../frontend.md)'s **navigator**: a structured, browsable view of the [Project](../../GLOSSARY.md#project) and its [Engineering State](../../core/shared-state-model.md) — requirements, functional blocks, components, nets, the BOM, board/layers, verification results, decisions — letting the engineer find any entity and reveal it in the right viewer or panel. It exists because a design is a large interconnected model, and the engineer needs a single tree/index to orient and navigate it — the code-IDE "explorer" ergonomic, applied to an engineering model rather than files. It **reflects** the runtime's state structure and **issues navigation/selection commands**; it holds **no engineering rules** and is not the source of truth ([P11](../../foundation/principles.md), [P2](../../foundation/principles.md)).

---

## 1. Purpose & responsibilities

### What it owns

- **Presenting the project structure.** Rendering a navigable hierarchy/index of the [Engineering State](../../core/shared-state-model.md) — the [domain-model](../../foundation/engineering-domain-model.md) entities grouped meaningfully (by requirement, by [functional block](../../foundation/engineering-domain-model.md#functional-block), by entity type, by [phase](../../core/workflow-orchestration.md) artifact).
- **Navigation & reveal.** Letting the engineer select an entity and reveal it in the appropriate surface ([schematic viewer](schematic-viewer.md), [PCB viewer](pcb-viewer.md), [diagnostics](diagnostics.md), [AI proposals](ai-interaction-model.md)).
- **Selection & filtering.** Search, filter, and grouping of entities; maintaining a selection that other surfaces can follow.
- **Status decoration.** Annotating entries with status cues sourced from runtime projections (e.g. an entity that carries an open [Violation](../../foundation/engineering-domain-model.md#violation), a phase artifact not yet produced) — *displayed*, not computed.

### What it does **NOT** own

- **The state structure itself.** The shape and contents of the [Engineering State](../../core/shared-state-model.md) are the runtime's; the explorer renders a projection of it ([P2](../../foundation/principles.md)).
- **Engineering logic.** No verification, constraint resolution, or gating; status decorations come from the [Verification Engine](../../engineering/verification-engine.md)/runtime, never from the explorer ([P11](../../foundation/principles.md)).
- **Mutation authority.** Renaming, deleting, or otherwise changing an entity happens only by issuing a command (mapped to a [Capability](../../core/capability-registry.md)) that the runtime validates and commits.
- **Project/session lifecycle.** Opening/closing/creating projects is governed by the [Project Store](../../GLOSSARY.md#project) and [sessions](../../collaboration/multi-user-and-sessions.md); the explorer triggers these via commands, it does not own them.

---

## 2. Position in the architecture

```mermaid
flowchart LR
  STATE[Engineering State\n(domain-model entities)] --> PROJ[State-structure projection\nhierarchy + status decorations]
  PROJ --> PQ[[Presentation/Query port]]
  PQ --> PE[Project Explorer\nbrowse · select · reveal]
  PE -->|navigation command / entity command| PQ
  PE -. drives selection .-> VIEWS[Viewers & panels]
  classDef ui fill:#eef0fb,stroke:#5b66c9;
  class PE,VIEWS ui;
```
*Figure: the explorer renders a state-structure projection and drives selection across the other surfaces; structural changes go back as commands. Viewpoint: the presentation ring.*

---

## 3. How it gets its data

- **State-structure projection.** The explorer subscribes, over the [Presentation/Query port](../../core/contracts.md#presentation-query-port), to a read-only projection of the [Engineering State](../../core/shared-state-model.md) structure — entities by [Entity ID](../../foundation/engineering-domain-model.md), their relationships, and the groupings used for display. This is a *projection of the canonical model* ([P6](../../foundation/principles.md)), not a parallel definition.
- **Status decorations.** Per-entity status (e.g. has-open-violation, phase-artifact-present) arrives folded into the projection or via the [diagnostics](diagnostics.md) projection from the [Verification Engine](../../engineering/verification-engine.md); the explorer displays these, computing none.
- **Live updates.** As the runtime commits [Events](../../core/event-bus.md) (entities created/changed, violations opened/closed, phases advanced), the projection updates and the tree re-decorates.

---

## 4. Reflecting the Engineering State structure

The explorer's organization tracks the [shared state model](../../core/shared-state-model.md) and the [domain model](../../foundation/engineering-domain-model.md), so navigating the tree *is* navigating the design:

- **Requirements & analysis** — [Requirements](../../foundation/engineering-domain-model.md) and the [functional blocks](../../foundation/engineering-domain-model.md#functional-block) realizing them.
- **Logical design** — [Components](../../foundation/engineering-domain-model.md#component), [Pins](../../foundation/engineering-domain-model.md#pin), [Nets](../../foundation/engineering-domain-model.md#net), [Symbols](../../foundation/engineering-domain-model.md#symbol) (the [Schematic IR](../../compiler/ir/schematic-ir.md) projection).
- **Physical design** — [Board / Layer Stack](../../foundation/engineering-domain-model.md#board--layer-stack), [Placement](../../foundation/engineering-domain-model.md#placement), [Track / Routing](../../foundation/engineering-domain-model.md#track--routing) (the [PCB IR](../../compiler/ir/pcb-ir.md) projection).
- **BOM** — parts/quantities/sourcing (the [BOM IR](../../compiler/ir/bom-ir.md) projection).
- **Verification & decisions** — [Violations](../../foundation/engineering-domain-model.md#violation)/[Waivers](../../foundation/engineering-domain-model.md#waiver) and the [Decisions](../../foundation/engineering-domain-model.md#decision)/provenance that justify the design.

Because all of these are projections of one canonical model, the explorer can cross-link them (a component to its net, its placement, its decisions) without holding any of it.

---

## 5. User interactions

- **Browse & search** the structure; filter by type, phase, or status.
- **Select** an entity to make it the shared selection; other surfaces follow it.
- **Reveal** an entity in the [schematic viewer](schematic-viewer.md), [PCB viewer](pcb-viewer.md), [diagnostics](diagnostics.md), or [AI interaction](ai-interaction-model.md).
- **Act on an entity** via context commands (mapped to [Capabilities](../../core/capability-registry.md)) — create/rename/delete/assign — each validated and committed by the runtime.
- **Trace provenance** — jump from an entity to its [Decision/Evidence](../../core/provenance-and-traceability.md) lineage.

---

## 6. What it does NOT do (no engineering rules)

The explorer computes no verification result, resolves no constraint, decides no gate, and changes no entity on its own. Status badges are runtime-sourced; structural edits are runtime-committed commands. It is a navigator over a projection of the canonical model ([P11](../../foundation/principles.md), [P6](../../foundation/principles.md)).

---

## 7. Contracts

- **Consumes:** the [Presentation/Query port](../../core/contracts.md#presentation-query-port) — the state-structure projection (and folded-in status/diagnostics), and command issuance for navigation and entity actions. The structure originates in the [Shared State Model](../../core/shared-state-model.md); status originates in the [Verification Engine](../../engineering/verification-engine.md).

---

## 8. Failure modes

- **Projection stale/unavailable.** The tree shows last-known structure marked stale and disables entity commands until reconnected; it never invents entities.
- **Entity command rejected** (schema-invalid, unpermitted, gated). No change; the explorer surfaces the reason ([P13](../../foundation/principles.md)).
- **Large model performance.** The explorer virtualizes/lazily loads the projection; correctness is unaffected since the runtime holds the truth.
- **Referenced entity missing** (e.g. removed concurrently). The entry resolves to "no longer present" rather than a stale phantom, consistent with the live projection.

---

## 9. Open decisions

- [ADR-0001](../../decisions/0001-adopt-clean-architecture-dependency-rule.md) — the explorer is a leaf consumer of the state projection.
- [ADR-0005](../../decisions/0005-ir-as-canonical-phase-boundary-representation.md) — entity groupings align with IR projections of the canonical model.
- **Open:** the default grouping/organization scheme(s) offered (by-block vs. by-type vs. by-phase) — a presentation refinement recorded here per [P13](../../foundation/principles.md).

---

## 10. Related documents

[`presentation/frontend.md`](../frontend.md) · [`core/shared-state-model.md`](../../core/shared-state-model.md) · [`foundation/engineering-domain-model.md`](../../foundation/engineering-domain-model.md) · [`core/contracts.md`](../../core/contracts.md#presentation-query-port) · [`core/provenance-and-traceability.md`](../../core/provenance-and-traceability.md) · [`presentation/frontend/schematic-viewer.md`](schematic-viewer.md) · [`presentation/frontend/pcb-viewer.md`](pcb-viewer.md) · [`presentation/frontend/diagnostics.md`](diagnostics.md) · [`foundation/principles.md`](../../foundation/principles.md) (P2, P6, P11)
