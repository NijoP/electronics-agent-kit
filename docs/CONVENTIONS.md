# Documentation Conventions

These conventions keep ~125 documents consistent and prevent drift. Every document in this repository follows them. They are themselves an architectural artifact: consistency is what lets the docs function as a single specification rather than a pile of notes.

## Scope reminder (Phase 0)

- **Markdown only.** No code files of any kind. No technology selection. Describe *what* and *why*, not *which library*.
- **Every decision is justified.** A non-obvious choice either carries its rationale inline or links to an [ADR](decisions/README.md).
- **No undocumented assumptions.** If a doc assumes something not yet decided, it says so explicitly, marked `> **Assumption:**`, and ideally links an open ADR.
- **Reusable intelligence goes to ECC**, not into these docs (see [`GLOSSARY.md` → ECC](GLOSSARY.md#ecc)).

## Document template

Every document opens with a one-paragraph **summary** answering: *what is this, what ring does it live in, and why does it exist?* Then, as applicable:

1. **Purpose & responsibilities** — what it owns; explicitly, what it does **not** own.
2. **Position in the architecture** — its ring, what it depends on (inward only), what depends on it.
3. **Body** — the substance, using the per-family template below where one applies.
4. **Contracts** — the [ports](core/contracts.md) it exposes and consumes.
5. **Failure modes** — how it fails and how the system degrades.
6. **Open decisions** — links to relevant [ADRs](decisions/README.md), including unresolved ones.
7. **Related documents** — cross-links.

## Per-family templates

To prevent the agents-vs-state-machines drift the architecture review flagged, each family uses a fixed section set:

| Family | Required sections |
|--------|-------------------|
| **Agent** ([`agents/`](agents/README.md)) | Purpose · Responsibilities · Inputs · Outputs · State · Events · Dependencies · Failure modes · Future improvements · **Two-part split** (deterministic use-case ‖ reasoning adapter) · **FSM cross-link** |
| **State machine** ([`state-machines/`](state-machines/README.md)) | States · Transitions · Events · Rollback · Recovery · Persistence · **Mermaid `stateDiagram-v2`** · **Agent/Engine cross-links** |
| **Store** ([`data/stores/`](data/stores/)) | Why it exists · Responsibilities · Conceptual data model · Access port · Consistency · Lifecycle & retention · Failure modes |
| **IR** ([`compiler/ir/`](compiler/)) | Purpose · Conceptual schema · Producers · Consumers · Invariants · Transformations in/out |
| **ADR** ([`decisions/`](decisions/README.md)) | Status · Context · Decision · Consequences · Alternatives considered |

**Division of responsibility (anti-duplication rule):** the **state machine** owns *States / Transitions / Events / Rollback / Recovery / Persistence*. The **agent** owns *Purpose / Inputs / Outputs / reasoning strategy / failure-of-reasoning*. They cross-reference; they never restate each other's fields.

## Naming

- Files: lowercase `kebab-case.md`. Folders are clean-architecture rings (see [`README.md`](README.md)).
- ADRs: `NNNN-short-title.md`, zero-padded, never renumbered.
- Defined terms are Capitalized and resolve to [`GLOSSARY.md`](GLOSSARY.md). Never use bare "planning" (see the disambiguation table there).

## Diagrams

- Use **Mermaid** fenced code blocks (a fenced block tagged `mermaid`) so diagrams live in version control as text.
- Architecture structure uses the **C4 model** (Context → Container → Component). The canonical set is in [`foundation/architecture-views.md`](foundation/architecture-views.md).
- State machines use `stateDiagram-v2`; interactions use `sequenceDiagram`; dependencies use `flowchart`.
- Every diagram has a one-line caption stating what it shows and from whose viewpoint.

## Cross-linking

- Link the first mention of any component to its primary doc.
- Use repo-relative links so they resolve on disk and in a viewer.
- The `[[wiki-style]]` form is reserved for ECC; in these docs use standard Markdown links.
- **Link-direction hygiene (dependency rule).** From an *inner-ring* doc (foundation/core/domain), link the **port** in [`core/contracts.md`](core/contracts.md), not the concrete outer adapter (a store, a cross-cutting impl, a frontend doc). The *outer* adapter declares "implements port X" and links inward. This keeps the documentation dependency graph pointing inward and prevents the port/adapter ring-cycles flagged in the [architecture health report](architecture-health-report.md) (finding F-4). Reverse-references for navigation (an IR naming its Producers/Consumers, a "Related documents" footer) are exempt — they are navigational, not dependencies.

## ADR process

1. A decision worth recording gets the next number in [`decisions/`](decisions/README.md) with status **Proposed**.
2. The doc that relies on the decision links to the ADR.
3. Status moves Proposed → **Accepted** (or **Rejected**); a later ADR may mark an earlier one **Superseded** (never edit the old one's decision — append).
4. Phase 0 ships ten **seed ADRs** capturing the load-bearing decisions; more accrete as the product evolves.

## Depth tiers

- **Deep** (`foundation/`, `core/`, `compiler/`, the four `engineering/` engines, `knowledge/`): comprehensive, fully justified, with diagrams and ADR links.
- **Solid** (everything else): substantial and complete against its family template, but focused.

Both tiers are *complete* documents — "solid" never means "stub."
