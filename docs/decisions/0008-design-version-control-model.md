# ADR-0008: Design version control ("Git for hardware")

> **Grounds:** [P5 — Everything Is Traceable](../foundation/principles.md), [P6 — One Canonical Model, Many Projections](../foundation/principles.md), [P10 — Humans Stay in Command](../foundation/principles.md). **Primary documents:** [`data/design-version-control.md`](../data/design-version-control.md), [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md), [`core/checkpoint-system.md`](../core/checkpoint-system.md).

## Status

Accepted.

## Context

Engineering is exploratory: an engineer (or an autonomous agent) wants to try an alternative power topology, route the board a different way overnight, or evaluate a competing part — *without* endangering the known-good design, and with the ability to compare, keep, or discard the experiment. In software this is solved by version control with cheap branches and merges. Electronics tools have historically lacked a real equivalent: designs are versioned by file copies and ad-hoc naming, with no semantic diff and no safe merge.

Two facts make a true "Git for hardware" both necessary and achievable here:

- **[Autonomy](0010-human-in-the-loop-autonomy-levels.md) needs safe sandboxes.** "Let the AI route the board overnight" is only acceptable if the attempt happens on a branch that can be reviewed and merged only if good ([P10](../foundation/principles.md)).
- **The foundations already exist.** The system has stable [Entity IDs](../foundation/engineering-domain-model.md), an authoritative [event log](0004-event-sourcing-decision.md), and a canonical [domain model](0005-ir-as-canonical-phase-boundary-representation.md). Branching over text files is impossible to merge semantically; branching over *identified engineering entities with a history* is.

We must decide how divergent lines of engineering history are created, compared, and reconciled — distinct from in-line [concurrency](0003-shared-state-consistency-model.md), which governs a *single* history.

## Decision

We adopt a **design version control model — "Git for hardware" — operating on [Engineering State](../core/shared-state-model.md) and keyed on stable [Entity IDs](../foundation/engineering-domain-model.md), not on files or text.**

1. **Branch = a divergent line of engineering history.** A [Design Branch](../GLOSSARY.md#design-branch) forks the canonical history at a point and accumulates its own [Events](../core/event-bus.md); the original line is untouched. Branching is cheap because history is an append-only log.
2. **Identity, not position, defines "the same thing."** A `Component`, `Net`, or `Constraint` is the same entity across branches because it shares an Entity ID — so diff and merge reason about *entities and their decisions*, not coordinates or file lines. Renames, edits, and refactors do not break the correspondence.
3. **Diff is semantic.** The difference between two branches is expressed in domain terms (entities added/changed/removed, [Decisions](../foundation/engineering-domain-model.md#decision) made), with [provenance](../core/provenance-and-traceability.md), not as opaque binary or text deltas.
4. **Merge of *intent* is a decision, never an automatic algorithm.** Independent changes reconcile cleanly; genuinely conflicting intents (two different placements for one component) are surfaced as a [Decision](../foundation/engineering-domain-model.md#decision) to an agent or human ([P10](../foundation/principles.md)) — the same principle the [concurrency model](0003-shared-state-consistency-model.md) applies in-line, extended across branches.
5. **Distinct from neighbours.** Branches are *not* [Checkpoints](../core/checkpoint-system.md) (restorable snapshots) or [Undo/Redo](../GLOSSARY.md#undoredo) (user command history); [`checkpoint-system.md`](../core/checkpoint-system.md) reconciles all three.

This decides a *version-control approach*; it names no VCS, storage, or diff/merge technology (Phase 0).

## Consequences

### Positive
- **Safe exploration and safe autonomy.** Risky or AI-driven experiments run on a branch and merge only if good — the precondition that makes higher [autonomy levels](0010-human-in-the-loop-autonomy-levels.md) acceptable.
- **Meaningful diffs and reviewable history.** Engineers see *what changed and why* in domain terms, with full provenance ([P5](../foundation/principles.md)) — impossible with file-copy versioning.
- **Reuses the same substrate.** Branch/merge are operations over the [event-sourced](0004-event-sourcing-decision.md) history and stable identities the system already maintains; little new conceptual machinery, much new capability.
- **Collaboration story.** Multiple lines of work can proceed and reconcile, underpinning [multi-user](../collaboration/multi-user-and-sessions.md) scenarios.

### Negative
- **Semantic merge is genuinely hard.** Entity-level three-way merge with intent-conflict detection is far more complex than text merge, even with stable IDs; correctness here is demanding to build and test.
- **Intent conflicts require human/agent decisions.** Some merges cannot be automatic by design, which adds review burden rather than removing it.
- **Identity discipline is mandatory.** Every entity must carry a stable, opaque ID through every operation; any place that keys on name or position breaks branching — a constraint on all of the [domain model](../foundation/engineering-domain-model.md).
- **History and branch metadata grow** and need retention/garbage-collection policy.

### Neutral
- This model governs *cross-branch* reconciliation; *in-line* concurrency remains [ADR-0003](0003-shared-state-consistency-model.md). The two are complementary and share the conflict-as-decision philosophy.
- Import/export to external EDA formats is a projection ([ADR-0005](0005-ir-as-canonical-phase-boundary-representation.md)), so branching remains a property of the canonical model, not of any exchange file.

## Alternatives considered

- **File-based / snapshot versioning (copy the design, name it `v2`).** Familiar, trivial. *Rejected:* no semantic diff, no safe merge, no provenance; cannot support branch-and-merge of *intent*, which is the whole point.
- **Reuse a text-oriented VCS on serialized design files.** Leverages mature tooling. *Rejected:* text/line merge is meaningless for an engineering graph; a one-line move can reorder a file and produce spurious conflicts, while a real intent conflict goes undetected. Identity must be semantic, not positional.
- **Snapshots/checkpoints only, no branching.** Gives rollback. *Rejected:* checkpoints restore a point in *one* history; they cannot represent two coexisting, comparable, mergeable lines of exploration.
- **Automatic CRDT-style merge of branches.** Always merges without asking. *Rejected:* auto-merging *engineering intent* can synthesize a design nobody chose, violating [P10](../foundation/principles.md) — the same reason [ADR-0003](0003-shared-state-consistency-model.md) rejects CRDT merge in-line.

## Related documents

[`data/design-version-control.md`](../data/design-version-control.md) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md) · [`core/checkpoint-system.md`](../core/checkpoint-system.md) · [`core/concurrency-and-consistency.md`](../core/concurrency-and-consistency.md) · [`core/provenance-and-traceability.md`](../core/provenance-and-traceability.md) · [`foundation/principles.md`](../foundation/principles.md) · [ADR-0003](0003-shared-state-consistency-model.md) · [ADR-0004](0004-event-sourcing-decision.md) · [ADR-0010](0010-human-in-the-loop-autonomy-levels.md)
