# ADR-0011 — Implementation language: Rust, one crate per ring

**Status:** Accepted (Phase 1)

## Context
Phase 0 deferred all technology choices. Phase 1 implements the deterministic kernel and
needs a language and a module structure that make the [Dependency Rule (P1)](../foundation/principles.md)
real, not aspirational.

## Decision
Implement the runtime in **Rust**, as a Cargo **workspace with one crate per architecture
ring** (`eak-units`, `eak-domain` → `eak-ports` → `eak-runtime` → `eak-engines`,
`eak-compiler` → `eak-phases` → `eak-store`, `eak-reasoning` → `eak-cli`). Dependency
edges point only inward; outer adapter crates implement inner [contracts](../core/contracts.md).

## Consequences
- Cargo forbids dependency cycles, and the kernel simply does not list adapter crates, so
  **P1 is enforced at compile time**. A guard unit test in `eak-runtime` fails the build if
  the kernel ever gains an outward dependency.
- Sum types model the FSM/IR/Event hierarchies precisely; `Result` makes the validation
  seam (P3) explicit; performance matches the "deterministic high-performance kernel" goal.

## Alternatives considered
- **TypeScript/Node** — single language with the future frontend, but ring isolation would
  be convention-only and determinism/performance weaker.
- **Go** — simple and fast, but weaker sum types for the FSM/IR modelling and looser
  module-boundary enforcement.
