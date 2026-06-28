# Artifact Store

> **Ring:** Interface adapters (outer). The Artifact Store persists **generated outputs** — manufacturing deliverables (Gerbers, drill files, pick-and-place), reports, and exports produced by [Manufacturing Generation](../../state-machines/manufacturing-generation.md) and other export phases. These are large, immutable, content-addressable blobs derived from [Engineering State](../../core/shared-state-model.md). It is an outer-ring [Adapter](../../GLOSSARY.md#adapter) that **fronts the [State Repository port](../../core/contracts.md#state-repository) for large generated outputs**, keyed by [Entity ID](../../core/shared-state-model.md) and provenance. **It names no storage technology or file format** ([P1](../../foundation/principles.md), Phase-0 rule).

---

## Why it exists

Engineering produces *deliverables* — the files a fab or assembly house consumes, plus reports and exports for humans. These differ from every other persisted thing: they are **large opaque blobs**, **immutable once generated**, and **derived** from the canonical design. Storing them in the [State Store](state-store.md) (a mutable entity graph) or the [Event Store](event-store.md) (an append-only change log) would be the wrong shape and would bloat stores tuned for small structured records ([storage taxonomy](../storage.md)). Hence a distinct store optimized for large content-addressed blobs with rich provenance back to the design that produced them.

## Responsibilities

**Owns:**
- **Durable persistence of generated artifacts** — manufacturing outputs (Gerbers, drill, pick-and-place, fab/assembly notes), [verification](../../engineering/verification-engine.md) and [analysis](../../foundation/engineering-domain-model.md#analysis-result) reports, and exports.
- **Provenance binding** — each artifact references the exact design **version/[branch](../design-version-control.md) coordinate**, [Event](../../core/event-bus.md) sequence position, and the [Decision](../../foundation/engineering-domain-model.md#decision)/generation run that produced it, so any deliverable is traceable to its source ([P5](../../foundation/principles.md)).
- **Content addressing / integrity** — identifying artifacts by content so the *same* output is recognizable and verifiable, and a delivered file can be proven to match what was generated.
- **Retrieval** for download/export via the [Presentation/Query port](../../core/contracts.md#presentation--query-port).

**Does NOT own:**
- **Generation logic.** Producing Gerbers/reports is [Manufacturing Generation](../../state-machines/manufacturing-generation.md) and the relevant agents/engines via the [Capability port](../../core/contracts.md); the store only persists results.
- **The canonical design.** Artifacts are *derived projections* of [Engineering State](../../core/shared-state-model.md) ([P6](../../foundation/principles.md)); the design is the [State Store](state-store.md).
- **The [Manufacturing IR](../../compiler/ir/manufacturing-ir.md).** The IR is the in-runtime phase-boundary representation; an artifact is its *externalized, delivered* form.
- **Storage technology / file formats.** Deferred ([P1](../../foundation/principles.md)).

## Conceptual data model

A collection of **artifact records**, each conceptually:

- the **artifact content** (a large opaque blob — Gerber set, drill file, report, export);
- a **descriptor** — kind, role, intended consumer (fab / assembly / human / downstream tool);
- a **content-derived identity** for integrity and de-duplication;
- **provenance** — the source design **version/branch coordinate**, [Event](../../core/event-bus.md) sequence position, generating run/[Decision](../../foundation/engineering-domain-model.md#decision), and the [Manufacturing IR](../../compiler/ir/manufacturing-ir.md) it was lowered from ([P5](../../foundation/principles.md));
- **scope/visibility** ([Security/Policy port](../../core/contracts.md)) and any [IP/licensing](../../governance/) markers.

Per [data-modeling](../data-modeling.md), the blob is *shape*; its *meaning and authority* live in the design version it was generated from.

## Access port

Fronts the **[State Repository port](../../core/contracts.md#state-repository)** for large derived outputs — store artifact, retrieve by identity/descriptor, list by project/version — and serves downloads via the [Presentation/Query port](../../core/contracts.md#presentation--query-port). Access is scoped by the [Security/Policy port](../../core/contracts.md).

## Consistency

- **Immutable once written.** A generated artifact is never edited; a regenerated output is a **new** artifact with its own provenance — the same immutability principle as the [Event Store](event-store.md), for the same reason (a delivered file must remain exactly what was shipped, [P5](../../foundation/principles.md)).
- **Reproducible from source.** Because generation is [deterministic](../../core/determinism-and-reproducibility.md) ([P4](../../foundation/principles.md)), an artifact is re-derivable from its recorded source version; content addressing lets a regenerated output be proven identical.
- **Strong provenance integrity.** An artifact without a resolvable source version is rejected — every deliverable must trace to the design that produced it ([P5](../../foundation/principles.md)).
- **Capture before irreversible delivery.** Generation that precedes an irreversible external step (e.g. sending to a fab) is paired with a [Checkpoint](checkpoint-store.md) trigger ([checkpoint-system §3](../../core/checkpoint-system.md)).

## Lifecycle & retention

- **Created** by [Manufacturing Generation](../../state-machines/manufacturing-generation.md)/export runs; **retained as a deliverable record** — what was shipped must remain auditable ([P5](../../foundation/principles.md)).
- **Re-derivable** from the recorded source version, so storage tiering of old artifacts is safe under a stated policy ([P13](../../foundation/principles.md)); deletion is governed, not silent ([governance](../../governance/)).
- **Versioned with the design** — distinct artifacts exist per [branch](../design-version-control.md)/version that generated them; superseded artifacts are retired with provenance, not overwritten.
- **Evolves** under [data-versioning](../data-versioning-and-migration.md) for descriptor/metadata; blob *formats* are a later-phase concern.

## Failure modes

- **Store unavailable.** Generated outputs cannot be persisted or downloaded; the runtime reports failure and does not mark a deliverable as produced ([failure taxonomy](../../core/failure-taxonomy-and-degraded-modes.md)) — and never lets an *un-persisted* artifact be treated as delivered.
- **Integrity mismatch on retrieval** (content does not match its identity). Detected via content addressing; the artifact is rejected and **regenerated from the recorded source version** ([P4](../../foundation/principles.md)).
- **Orphaned artifact** (source version unresolvable). Surfaced as a provenance fault, not silently served — a deliverable must trace to its design ([P5](../../foundation/principles.md)).
- **Storage pressure from large blobs.** Bounded by a stated retention/tiering policy ([P13](../../foundation/principles.md)); safe because artifacts are re-derivable.
- **Cross-tenant/IP leakage risk.** Prevented by scope and [IP/licensing](../../governance/) markers enforced via the [Security/Policy port](../../core/contracts.md).

## Open decisions

- [ADR-0004](../../decisions/0004-event-sourcing-decision.md) — artifacts bind to an [Event](event-store.md) sequence position; their re-derivability rests on the system-of-record resolution.
- [ADR-0008](../../decisions/0008-design-version-control-model.md) — per-[branch](../design-version-control.md) artifact versioning.
- [ADR-0009](../../decisions/0009-determinism-and-replay-strategy.md) — deterministic generation underwrites artifact reproducibility.
- **Open (deferred):** the concrete blob storage technology, output file formats, and retention/tiering specifics — later-phase decisions ([P1](../../foundation/principles.md)).

## Related documents

[`state-machines/manufacturing-generation.md`](../../state-machines/manufacturing-generation.md) (the producer) · [`compiler/ir/manufacturing-ir.md`](../../compiler/ir/manufacturing-ir.md) (the IR it externalizes) · [`core/contracts.md`](../../core/contracts.md) (State Repository, Presentation/Query) · [`core/provenance-and-traceability.md`](../../core/provenance-and-traceability.md) · [`data/stores/state-store.md`](state-store.md) · [`data/stores/event-store.md`](event-store.md) · [`data/stores/checkpoint-store.md`](checkpoint-store.md) · [`data/design-version-control.md`](../design-version-control.md) · [`governance/`](../../governance/) · [`data/storage.md`](../storage.md)
