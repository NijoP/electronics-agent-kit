# Project Store

> **Ring:** Interface adapters (outer). The Project Store is the **outer-ring [Adapter](../../GLOSSARY.md#adapter)** that persists the **[Project](../../GLOSSARY.md#project) registry and metadata** — the catalog of all design efforts and the addressing roots from which each project's [Engineering State](../../core/shared-state-model.md), history, sessions, and artifacts are resolved. It **fronts the [State Repository port](../../core/contracts.md#state-repository) at registry scope** (project-level entities, above any single design's state). **It names no storage technology** ([P1](../../foundation/principles.md), Phase-0 rule).

---

## Why it exists

A [Project](../../GLOSSARY.md#project) is the top-level container for one design effort. The system needs to know *which projects exist*, resolve a project's identity, and find the roots of its [State Store](state-store.md), [Event Store](event-store.md), [Checkpoint Store](checkpoint-store.md), and [Artifact Store](artifact-store.md). That registry is **cross-project metadata that lives above any single design** — a different scope and access shape (catalog lookup) from the per-design entity graph — which is why it is a distinct store ([storage taxonomy](../storage.md)). It is the entry point that makes a project addressable at all.

## Responsibilities

**Owns:**
- **The project registry** — the durable catalog of all [Projects](../../GLOSSARY.md#project): identity, name, status, ownership, creation/recorded-timestamps, and access scope.
- **Addressing roots** — the binding from a Project to where its [Engineering State](../../core/shared-state-model.md), [event history](event-store.md), [checkpoints](checkpoint-store.md), and [artifacts](artifact-store.md) live, and which [design branches](../design-version-control.md) it has.
- **Project-level metadata** — non-design-significant attributes (description, tags, collaborators) and configuration scope ([Configuration port](../../core/contracts.md)).
- **Lookup/listing** for the [Presentation/Query port](../../core/contracts.md#presentation--query-port) (project pickers, dashboards).

**Does NOT own:**
- **The design content.** Each project's [Engineering State](../../core/shared-state-model.md) is the [State Store](state-store.md); the Project Store holds the *registry entry*, not the design ([P2](../../foundation/principles.md)).
- **History, snapshots, sessions, artifacts.** Owned by the [Event](event-store.md), [Checkpoint](checkpoint-store.md), [Session](session-store.md), and [Artifact](artifact-store.md) stores respectively; the Project Store *points at* them.
- **Authentication / identity provider.** That is [security](../../crosscutting/security.md); it records resolved ownership/scope only.
- **Storage technology.** Deferred ([P1](../../foundation/principles.md)).

## Conceptual data model

A **catalog of project records**, each conceptually:

- a **stable, opaque project identity** ([Entity ID](../../core/shared-state-model.md) discipline applied at project scope — references by ID, not by name);
- **descriptive metadata** — name, description, tags, status, ownership/collaborators, recorded timestamps;
- **addressing roots** — references to the project's [state](state-store.md), [history](event-store.md), [checkpoints](checkpoint-store.md), [artifacts](artifact-store.md), and the set of [design branches](../design-version-control.md);
- **scope/visibility** for access control ([Security/Policy port](../../core/contracts.md)).

Per [data-modeling](../data-modeling.md), the project name is a mutable *attribute*; the project identity is the immutable key everything else resolves through.

## Access port

Fronts the **[State Repository port](../../core/contracts.md#state-repository)** at registry scope — get/query project records, apply validated changes to registry metadata — and serves read-only project listings via the [Presentation/Query port](../../core/contracts.md#presentation--query-port). Registry mutations follow the same justified-change discipline as any other state ([shared-state-model](../../core/shared-state-model.md)), scoped by the [Security/Policy port](../../core/contracts.md).

## Consistency

- **Strong for the registry.** The catalog is authoritative for "which projects exist and where their data roots are"; a stale or wrong root would mis-resolve a whole project, so registry writes are validated and consistent ([concurrency model](../../core/concurrency-and-consistency.md)).
- **Referential integrity to per-project stores.** Addressing roots must resolve to live stores; a dangling root is a detectable fault, not a silent null ([P13](../../foundation/principles.md)).
- **Registry changes are recorded** as justified changes/[Events](../../core/event-bus.md) like any other state mutation ([P5](../../foundation/principles.md)).

## Lifecycle & retention

- **Created** when a project is created; **retained for the life of the project** (a source-of-truth registry).
- **Archival/soft-delete, not silent erasure** — a removed project is marked retired with a tombstone so its history and artifacts remain auditable ([P5](../../foundation/principles.md)); hard deletion is an explicit, governed action ([governance](../../governance/)).
- **Evolves** under [data-versioning & migration](../data-versioning-and-migration.md).

## Failure modes

- **Store unavailable.** Projects cannot be listed or opened (no addressing roots); the system reports unavailability rather than guessing roots ([failure taxonomy](../../core/failure-taxonomy-and-degraded-modes.md)).
- **Dangling addressing root** (registry points at missing per-project data). Detected on open; surfaced as a recoverable fault — never a silent empty project.
- **Identity collision / ambiguity.** Prevented by opaque, unique project identity ([data-modeling](../data-modeling.md)).
- **Cross-tenant visibility leak.** Prevented by scope enforcement ([Security/Policy port](../../core/contracts.md)) — a user lists only permitted projects.

## Open decisions

- [ADR-0008](../../decisions/0008-design-version-control-model.md) — how a project's [design-branch](../design-version-control.md) set is represented in the registry.
- [ADR-0003](../../decisions/0003-shared-state-consistency-model.md) — consistency of registry mutations under concurrency.
- **Open (deferred):** the concrete registry storage technology and archival policy specifics — a later-phase decision ([P1](../../foundation/principles.md)).

## Related documents

[`core/shared-state-model.md`](../../core/shared-state-model.md) · [`core/contracts.md`](../../core/contracts.md) (State Repository, Presentation/Query) · [`data/stores/state-store.md`](state-store.md) · [`data/stores/event-store.md`](event-store.md) · [`data/stores/session-store.md`](session-store.md) · [`data/stores/artifact-store.md`](artifact-store.md) · [`collaboration/multi-user-and-sessions.md`](../../collaboration/multi-user-and-sessions.md) · [`data/storage.md`](../storage.md) · [`GLOSSARY.md`](../../GLOSSARY.md) (Project)
