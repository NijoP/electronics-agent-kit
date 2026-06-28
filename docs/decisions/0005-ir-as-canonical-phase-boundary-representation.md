# ADR-0005: IRs are canonical phase-boundary projections of the domain model

> **Grounds:** [P6 — One Canonical Model, Many Projections](../foundation/principles.md). **Primary documents:** [`compiler/compiler-ir.md`](../compiler/compiler-ir.md), [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md), [`compiler/transformations.md`](../compiler/transformations.md).

## Status

Accepted.

## Context

Electronics Agent Kit borrows the compiler metaphor: a design is progressively "lowered" through stages, each crossing a [Phase](../state-machines/README.md) boundary with a typed, serializable **[Intermediate Representation (IR)](../compiler/compiler-ir.md)** — Requirement IR → Engineering IR → BOM/Schematic IR → PCB IR → Manufacturing IR. IRs are exactly what reproducibility and provenance want at a boundary: a stable, serializable artifact you can name, diff, and point to.

But the architecture review flagged a serious risk. The plan had an IR set, store schemas, and (originally missing) a domain model — **three structures all describing the same engineering nouns.** Left as peers, they would become three drifting sources of truth: a `Component` would mean subtly different things in the Schematic IR, the State Store, and the agents' heads, and the differences would accumulate silently until the system was internally inconsistent. The review identified the absent domain model as the single largest gap precisely because, without one canonical definition, the IRs *become* the definition — several times over.

We must decide, once, the authority relationship between the [domain model](../foundation/engineering-domain-model.md) and the IRs.

## Decision

The **[Engineering Domain Model](../foundation/engineering-domain-model.md) is the single canonical source of truth. IRs are strict *projections* (phase-boundary serializations) of it — never rival definitions.**

1. **One definition, many views.** Every entity (`Component`, `Net`, `Constraint`, …) is defined exactly once, in the domain model. An IR is a *view* of the relevant slice of that model, shaped for one phase boundary; [store schemas](../data/storage.md) and UI view-models are likewise projections, not competing definitions ([P6](../foundation/principles.md)).
2. **IRs are derived, not authored in parallel.** An IR is produced *from* canonical [Engineering State](../core/shared-state-model.md) and, when a [transformation](../compiler/transformations.md) (lowering) writes results back, it writes into the canonical model — the IR is never a second place where truth is independently edited.
3. **Lowerings are invariant-preserving and traceable.** Each transformation from one IR to the next preserves the domain invariants and carries [provenance](../core/provenance-and-traceability.md): "this PCB IR was lowered from that Schematic IR by that pass under those [Decisions](../foundation/engineering-domain-model.md#decision)."
4. **Stable [Entity IDs](../foundation/engineering-domain-model.md) thread through every projection,** so the same entity is recognizably itself in every IR, store, and view — the bridge that makes projection coherent and [replay](0009-determinism-and-replay-strategy.md) able to re-bind references.

This decides the *representational architecture*; it picks no serialization format, schema language, or storage technology (Phase 0).

## Consequences

### Positive
- **Drift becomes structurally impossible.** Collapsing three would-be sources of truth into one canonical model with derived views removes the drift the review warned about ([P6](../foundation/principles.md)).
- **Reproducibility and provenance anchors.** A serializable IR at a boundary is a deterministic, replayable artifact ([P4](../foundation/principles.md), [ADR-0009](0009-determinism-and-replay-strategy.md)) and a natural provenance checkpoint.
- **Phase independence with consistency.** Each phase gets an IR tailored to its needs without forking the model; new phases add projections, not new definitions.
- **Typed values carry through.** [Physical Quantities](0007-units-and-physical-quantity-type-system.md) serialize consistently across every IR because they are defined once in the model.

### Negative
- **Projection machinery to build and maintain.** Deriving IRs from the model (and lowerings writing back) is more work than letting each phase keep its own structure.
- **The canonical model is now a bottleneck for change.** A new attribute is added in one authoritative place and rippled to projections; this is the cost of single-source-of-truth.
- **Round-trip discipline.** Any IR that is edited in a tool must reconcile back into the canonical model rather than living as an independent artifact, which constrains how external-format round-tripping is handled.

### Neutral
- The "compiler" framing is now precise rather than metaphorical: IRs are projections and transformations are lowerings, with the domain model as the typed source language.
- Exchange with external EDA formats is treated as *import/export adapters* that project to/from the canonical model, not as alternative sources of truth.

## Alternatives considered

- **IRs as independent, co-equal sources of truth (no canonical model).** Each phase owns its representation. *Rejected:* this *is* the multi-source-of-truth drift the review identified as the largest risk; there would be no single answer to "what is a Component."
- **A store schema as the canonical model.** Let persistence define the nouns. *Rejected:* couples the domain definition to a storage technology (violating [P1](../foundation/principles.md)/[ADR-0001](0001-adopt-clean-architecture-dependency-rule.md)) and shapes the domain around storage convenience rather than engineering meaning.
- **One monolithic representation for all phases (no per-phase IRs).** Maximal consistency. *Rejected:* a single mega-structure forces every phase to carry every other phase's concerns, is unwieldy to serialize at boundaries, and loses the clean lowering/provenance story.
- **Direct external EDA formats as the IRs.** Reuse existing standards as the working representation. *Rejected:* external formats are lossy, tool-specific, and not designed to carry typed quantities, intent, or provenance; they are import/export projections, not the canonical model.

## Related documents

[`compiler/compiler-ir.md`](../compiler/compiler-ir.md) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md) · [`compiler/transformations.md`](../compiler/transformations.md) · [`core/shared-state-model.md`](../core/shared-state-model.md) · [`foundation/principles.md`](../foundation/principles.md) (P6) · [ADR-0007](0007-units-and-physical-quantity-type-system.md) · [ADR-0008](0008-design-version-control-model.md) · [ADR-0009](0009-determinism-and-replay-strategy.md)
