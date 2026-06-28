# ADR-0010: Configurable autonomy levels; AI proposes, engineer disposes

> **Grounds:** [P10 — Humans Stay in Command](../foundation/principles.md), [P3 — LLMs Are Only Reasoning Engines](../foundation/principles.md), [P5 — Everything Is Traceable](../foundation/principles.md). **Primary documents:** [`engineering/human-in-the-loop.md`](../engineering/human-in-the-loop.md), [`core/workflow-orchestration.md`](../core/workflow-orchestration.md).

## Status

Accepted.

## Context

An AI-native engineering tool that can *act* on a design sits between two failure modes. Too little autonomy and it is merely a chat assistant — the engineer does all the work and the AI's leverage is wasted. Too much, or the wrong kind, and it makes consequential, hard-to-reverse changes to real hardware decisions without the engineer's understanding or consent — unacceptable for professional engineering, and corrosive to trust.

Different contexts need different settings: a safety-critical power stage or a new user wants advice only; a trusted, well-scoped, repetitive sub-flow (say, routing within fixed rules) can run with far less supervision; most day-to-day work sits in between. A single fixed autonomy setting cannot serve all of these, and a hidden default that lets the AI act silently would be the worst of all.

We must decide how human authority over AI action is modelled and enforced — and the decision must guarantee that authority is configurable, explicit, and above all **reversible and traceable**.

## Decision

We adopt **configurable [Autonomy Levels](../engineering/human-in-the-loop.md) governed by the discipline "AI proposes, engineer disposes."**

1. **A graduated spectrum, not a binary.**
   - **Advisory** — the AI may only *suggest*; every design-significant change needs a human.
   - **Supervised** (the expected default) — the AI may make low-risk, easily-reversible changes within declared bounds; significant changes, gate crossings, and [waivers](../engineering/verification-engine.md) need approval.
   - **Autonomous** — the AI may act within explicit scope/budget/risk bounds; bound-exceeding or irreversible-by-nature actions still require a human.
2. **Set per project, refinable per phase or per [capability](../core/capability-registry.md) class** (e.g. autonomous routing but advisory part-selection). The level is a dial, not a switch.
3. **Risk-aware.** The autonomy decision combines the active level with each [Capability's](../core/capability-registry.md) declared side-effects, so a high-impact action can demand approval even at a high level.
4. **Propose vs. dispose seam.** [Agents](../agents/README.md) and the [Reasoning Engine](../core/reasoning-engine-interface.md) *propose* (a [Decision](../foundation/engineering-domain-model.md#decision) with [Evidence](../foundation/engineering-domain-model.md#evidence)); only a human — or an explicitly authorized, recorded autonomous action — *disposes* (commits). This sits exactly on the deterministic-use-case / reasoning-adapter boundary ([ADR-0006](0006-agent-fsm-separation.md)) and the propose/commit seam ([ADR-0002](0002-runtime-owns-knowledge-llm-as-reasoning-engine.md)).
5. **Reversibility is the non-negotiable precondition.** Every action — human or AI, at any level — is **traceable** ([events](../core/event-bus.md) + decisions + [provenance](../core/provenance-and-traceability.md)), **undoable** ([undo/redo](../GLOSSARY.md#undoredo)), **restorable** ([checkpoints](../core/checkpoint-system.md)), and **forkable** ([branches](0008-design-version-control-model.md)). No path — not even the autonomous level — bypasses a recorded disposal ([P2](../foundation/principles.md)).

This decides a *control model*; it names no UI, approval, or policy technology (Phase 0).

## Consequences

### Positive
- **Human authority is explicit and configurable** ([P10](../foundation/principles.md)): the engineer chooses how much to delegate, per context, and can always see and undo what the system did.
- **Autonomy becomes safe to offer.** Because [event-sourcing](0004-event-sourcing-decision.md) makes every action recorded and reversible, "let the AI route the board overnight" is a safe proposition, not a gamble — reversibility is the architectural feature that licenses delegation.
- **Trust ramps gradually.** Users start advisory and widen autonomy as confidence grows; the same product serves cautious and power-user regimes.
- **Rejections are signal.** A disposed-rejection is itself an [Event](../core/event-bus.md), feeding the [Learning Engine](../engineering/learning-engine.md) and the audit trail ([P5](../foundation/principles.md)).

### Negative
- **Approval friction at low autonomy.** Advisory/supervised modes gate work on human attention; a project can sit idle awaiting approval (a visible *gated* state, not a silent stall).
- **The autonomy decision is real machinery.** Combining level, capability side-effects, bounds, budgets, and authority routing (via the [Security/Policy port](../core/contracts.md)) for *every* proposal is non-trivial to build and reason about.
- **Configuration surface and misconfiguration risk.** A per-project/phase/capability dial is powerful but can be set wrongly; the system must make the active level and its implications legible.

### Neutral
- The granularity of assignment (project vs. phase vs. per-capability) and how risk classes are defined are refinements left open for a future ADR; this ADR fixes the *model and its guarantees*.
- The gate *mechanism* lives in the [Workflow Orchestrator](../core/workflow-orchestration.md) and waiver lifecycle in the [Verification Engine](../engineering/verification-engine.md); this decision owns *what a gate means* and *when approval is required*, not those mechanisms.

## Alternatives considered

- **Fully autonomous by default ("agentic" end-to-end).** Maximal leverage, great demos. *Rejected:* unacceptable for professional hardware work — consequential, costly changes without consent destroy trust and violate [P10](../foundation/principles.md).
- **Advisory-only (assistant that never acts).** Maximally safe. *Rejected:* wastes the product's core leverage; the AI could safely do far more given reversibility, so refusing to ever act under-delivers.
- **A single fixed autonomy setting.** Simple to implement. *Rejected:* no one setting fits safety-critical work, routine work, and trusted sub-flows simultaneously; autonomy must be context-configurable.
- **Per-action confirmation prompts only (no level model).** Always ask before each change. *Rejected:* it is just permanent Advisory mode with extra clicks — it cannot express "act autonomously within these bounds" and produces approval fatigue that erodes the very oversight it intends.

## Related documents

[`engineering/human-in-the-loop.md`](../engineering/human-in-the-loop.md) · [`core/workflow-orchestration.md`](../core/workflow-orchestration.md) · [`engineering/verification-engine.md`](../engineering/verification-engine.md) · [`core/capability-registry.md`](../core/capability-registry.md) · [`core/checkpoint-system.md`](../core/checkpoint-system.md) · [`data/design-version-control.md`](../data/design-version-control.md) · [`foundation/principles.md`](../foundation/principles.md) (P10) · [ADR-0002](0002-runtime-owns-knowledge-llm-as-reasoning-engine.md) · [ADR-0006](0006-agent-fsm-separation.md) · [ADR-0008](0008-design-version-control-model.md)
