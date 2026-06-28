# Session Store

> **Ring:** Interface adapters (outer). The Session Store persists **per-user, per-[Session](../../GLOSSARY.md#session) interaction state** — the ephemeral working context of a user interacting with a [Project](project-store.md): the [Undo/Redo](../../GLOSSARY.md#undoredo) stack, current selection/view, in-flight commands, and presentation preferences. It is an outer-ring [Adapter](../../GLOSSARY.md#adapter) that **fronts the [Presentation/Query port](../../core/contracts.md#presentation--query-port)** (the boundary through which the UI interacts), holding the interaction state that port's clients accumulate. **It names no storage technology** ([P1](../../foundation/principles.md), Phase-0 rule). Its defining rule: **session state is not design state** and must never pollute the durable engineering record.

---

## Why it exists

Collaboration and multi-user work ([`multi-user-and-sessions.md`](../../collaboration/multi-user-and-sessions.md)) require somewhere to keep *who is doing what, right now* — separately from the design itself. This is short-lived, per-user, non-design-significant state with its own access shape (scoped key/value per session) and its own retention (transient). Putting it in the [State Store](state-store.md) would corrupt the design record with interaction noise and break the rule that **only justified [Decisions](../../foundation/engineering-domain-model.md#decision) change the design** ([P2](../../foundation/principles.md)). Hence a distinct store ([storage taxonomy](../storage.md)).

## Responsibilities

**Owns:**
- **Per-session interaction state** — selection, active view, panel layout, transient preferences, and **the [Undo/Redo](../../GLOSSARY.md#undoredo) command stack** for the session ([checkpoint-system §5](../../core/checkpoint-system.md)).
- **In-flight command/request context** — what the user has issued but not yet committed, and presentation-only scratch state.
- **Session identity & lifetime** — associating a session with a user and a [Project](project-store.md), and with the user's permitted scope ([Security/Policy port](../../core/contracts.md)).

**Does NOT own:**
- **Design / engineering state.** That is the [State Store](state-store.md). A session *references* design entities by [Entity ID](../../core/shared-state-model.md); it never holds authoritative design content ([P2](../../foundation/principles.md)).
- **The durable history.** Committed changes are [Events](../../core/event-bus.md) in the [Event Store](event-store.md). Undo issues *compensating* commands that become events; the session holds only the *stack*, not the authoritative history ([checkpoint-system §5](../../core/checkpoint-system.md)).
- **Engineering rules.** None — presentation/interaction only ([P11](../../foundation/principles.md)).
- **Authentication / identity provider.** That is [security](../../crosscutting/security.md); the session merely carries the resolved scope.
- **Storage technology.** Deferred ([P1](../../foundation/principles.md)).

## Conceptual data model

A collection of **session records**, each conceptually:

- a **session identity** bound to a user and a [Project](project-store.md);
- **interaction state** — current selection, view/layout, preferences (presentation-only);
- the **[Undo/Redo](../../GLOSSARY.md#undoredo) stack** — an ordered list of the session's user commands, each referencing the committed [Events](../../core/event-bus.md) it produced (so undo can emit compensating commands);
- **in-flight context** — issued-but-uncommitted commands;
- **scope/visibility** — the user's permitted access for this session.

All design references are **by Entity ID** ([data-modeling](../data-modeling.md)); the session stores pointers and interaction metadata, never copies of design entities.

## Access port

Fronts the **[Presentation/Query port](../../core/contracts.md#presentation--query-port)** — the boundary through which the [frontend](../../presentation/frontend.md) subscribes to projections and issues commands ([P11](../../foundation/principles.md)). The session is where that port's per-client interaction state is accumulated and persisted across reconnects. Access is scoped by the [Security/Policy port](../../core/contracts.md).

## Consistency

- **Weak/relaxed by design.** Session state is convenience state; brief loss (e.g. a dropped selection) degrades UX, never correctness, because **no design truth lives here**.
- **Undo correctness comes from events, not the session.** Undo/Redo operates by emitting compensating commands that the runtime turns into [Events](../../core/event-bus.md) ([P5](../../foundation/principles.md) — history stays immutable); the session stack is a convenience index into that authoritative history, so a corrupted stack costs undo depth, not design integrity.
- **Multi-session coherence** (two sessions on one project) is reconciled at the design layer via the [concurrency model](../../core/concurrency-and-consistency.md) and [collaboration](../../collaboration/multi-user-and-sessions.md), not by this store.

## Lifecycle & retention

- **Created** when a user opens a session on a [Project](project-store.md); **transient** — bounded by the session's life plus a stated idle/expiry window ([P13](../../foundation/principles.md) — explicit, not silent).
- **Expired/closed sessions are reclaimable** without design-knowledge loss, because nothing design-significant lives here.
- **Survives reconnect** within its window so a user resumes where they left off; beyond the window it is discarded.
- **Not branch-versioned** — interaction state is per-session, not per-[design-branch](../design-version-control.md), though a session *points at* a branch/version coordinate.

## Failure modes

- **Store unavailable.** The user can still work against live design state through the [Presentation/Query port](../../core/contracts.md); only convenience state (selection, undo depth) is lost — never design data ([failure taxonomy](../../core/failure-taxonomy-and-degraded-modes.md)).
- **Lost/corrupt undo stack.** Undo depth is reduced; design integrity is unaffected because authoritative history is the [Event Store](event-store.md).
- **Stale in-flight command** after a crash. Uncommitted means unrecorded; nothing reaches design state without a committed [Event](../../core/event-bus.md), so a lost in-flight command simply did not happen ([P2](../../foundation/principles.md)).
- **Cross-session/tenant leakage risk.** Prevented by scope enforcement ([Security/Policy port](../../core/contracts.md)); a session sees only its user's permitted scope.

## Open decisions

- [ADR-0003](../../decisions/0003-shared-state-consistency-model.md) — how concurrent sessions on one project are reconciled at the design layer.
- [ADR-0010](../../decisions/0010-human-in-the-loop-autonomy-levels.md) — how session-level autonomy preferences are represented and scoped.
- **Open (deferred):** the concrete storage technology and session-expiry policy specifics — a later-phase decision ([P1](../../foundation/principles.md)).

## Related documents

[`collaboration/multi-user-and-sessions.md`](../../collaboration/multi-user-and-sessions.md) · [`core/checkpoint-system.md`](../../core/checkpoint-system.md) (Undo/Redo reconciliation) · [`presentation/frontend.md`](../../presentation/frontend.md) · [`core/contracts.md`](../../core/contracts.md) (Presentation/Query port) · [`data/stores/state-store.md`](state-store.md) · [`data/stores/event-store.md`](event-store.md) · [`data/stores/project-store.md`](project-store.md) · [`crosscutting/security.md`](../../crosscutting/security.md) · [`data/storage.md`](../storage.md)
