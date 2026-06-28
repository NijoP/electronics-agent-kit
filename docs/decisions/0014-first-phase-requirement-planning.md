# ADR-0014 — First implemented phase: Requirement Planning

**Status:** Accepted (Phase 1)

## Context
[The roadmap](../foundation/roadmap.md) prescribes proving the kernel end-to-end on exactly
one phase and recommends Requirement Planning. This ADR confirms that choice (the roadmap
left it open to confirm when implementation begins).

## Decision
Implement **[Requirement Planning](../state-machines/requirement-planning.md)** as the first
and only real phase in Phase 1: it is the lifecycle root, needs the fewest upstream
entities, and exercises the full loop (intent → reasoning → validated → committed →
traceable) on simple data. A thin **Engineering Analysis stub** is added only to prove
multi-phase orchestration and the Requirement IR → Engineering IR lowering seam.

## Consequences
- Phase 1 models only five entities + one relationship; all downstream entities are out of
  scope (named explicitly, P13).
- Proves principles P1–P4 and P8 and the quality attributes reproducibility, auditability,
  testability.

## Alternatives considered
- Starting at a verification phase (ERC/DRC) — needs more upstream entities first.
