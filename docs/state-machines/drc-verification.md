# State Machine — DRC Verification

> **Ring:** Use cases / runtime (inner) — a [State Machine](../GLOSSARY.md#state-machine-fsm) **instance** ([framework](../core/state-machine-framework.md)). This is **Phase 11**: it runs the **Design Rule Check** over the routed [PCB IR](../compiler/ir/pcb-ir.md) — a geometric/electrical-clearance specialization of the generic [Verification Engine](../engineering/verification-engine.md) [Rule → Violation → Waiver](../engineering/verification-engine.md#3-the-generic-rule--violation--waiver-lifecycle) lifecycle. Driven by the [DRC Agent](../agents/drc-agent.md). On unwaived error, its `Failed` terminal is routed by the [orchestrator](../core/workflow-orchestration.md) **back to [Routing Planning](routing-planning.md)**. This doc owns *States · Transitions · Events · Rollback · Recovery · Persistence*; the [agent](../agents/drc-agent.md) owns violation-explanation reasoning ([anti-duplication](../CONVENTIONS.md)).

## Bindings

| Binding | Value |
|---------|-------|
| Driving agent | [DRC Agent](../agents/drc-agent.md) |
| Engines used | [Verification Engine](../engineering/verification-engine.md) (design-rule set) |
| IR | **checks** [PCB IR](../compiler/ir/pcb-ir.md) (writes [Violations](../foundation/engineering-domain-model.md#violation)/[Waivers](../foundation/engineering-domain-model.md#waiver)) |
| Upstream | [Routing Planning](routing-planning.md) |
| Downstream (pass) | [DFM Verification](dfm-verification.md) |
| Loop-back (fail) | **↺ [Routing Planning](routing-planning.md)** |
| Framework | conforms to [state-machine-framework](../core/state-machine-framework.md) |

## States

| State | Kind | Meaning |
|-------|------|---------|
| `Idle` | Initial | Awaits activation when the routed [PCB IR](../compiler/ir/pcb-ir.md) is ready. |
| `LoadingPCBIR` | Normal (Gathering) | Reads the physical-domain scope ([Tracks](../foundation/engineering-domain-model.md#track--routing)/[Footprints](../foundation/engineering-domain-model.md#footprint)) the rule set evaluates. |
| `EvaluatingRules` | Normal (Working) | [Verification Engine](../engineering/verification-engine.md) evaluates geometric/clearance rules (track-to-track, annular ring, min width, courtyard) and creates/deduplicates [Violations](../foundation/engineering-domain-model.md#violation). |
| `TriagingViolations` | Normal (Reviewing) | [DRC Agent](../agents/drc-agent.md) explains violations and suggests fixes (reasoning *explains*; severity/gating stay deterministic). |
| `AwaitingDisposition` | Waiting / HITL | Engineer reroutes, or authorizes [Waivers](../foundation/engineering-domain-model.md#waiver) at the [Autonomy Level](../engineering/human-in-the-loop.md). |
| `RecordingWaivers` | Normal (Applying) | Persists authorized waivers with rationale, scope, expiry, and [provenance](../core/provenance-and-traceability.md). |
| `Passed` | Terminal (success) | No open error-severity violations; orchestrator advances to [DFM Verification](dfm-verification.md). |
| `Failed` | Terminal (failure) | Open error-severity violations remain → orchestrator loops back to [Routing Planning](routing-planning.md). |

## Transitions

| From → To | Guard | Effect (agent / engine) | Events emitted |
|-----------|-------|-------------------------|----------------|
| `Idle → LoadingPCBIR` | routed PCB IR present | open scope | `PhaseEntered` |
| `LoadingPCBIR → EvaluatingRules` | scope loaded | [Verification Engine](../engineering/verification-engine.md) runs rule set | `DRCRunStarted` |
| `EvaluatingRules → Passed` | no open error violations | finalize | `ViolationsRecorded`, `DRCPassed`, `PhaseCompleted` |
| `EvaluatingRules → TriagingViolations` | error violations exist | agent triages | `ViolationsRecorded` |
| `TriagingViolations → AwaitingDisposition` | needs human disposition | present | `DispositionRequested` |
| `AwaitingDisposition → RecordingWaivers` | waiver(s) authorized | record waivers | `ViolationWaived` |
| `AwaitingDisposition → Failed` | engineer chooses to fix at source | abort phase | `DRCFailed`, `PhaseFailed` |
| `RecordingWaivers → EvaluatingRules` | waivers recorded | re-evaluate gate | `DRCReRun` |
| `EvaluatingRules → Failed` | unwaived errors persist after re-run | abort phase | `DRCFailed`, `PhaseFailed` |

## Events

- **Consumed:** `PhaseActivated`, `PCBIREnriched` (routing ready), `WaiverAuthorized` / `FixRequested` (from [HITL](../engineering/human-in-the-loop.md)).
- **Emitted:** `PhaseEntered`, `DRCRunStarted`, `ViolationsRecorded`, `ViolationWaived`, `DRCReRun`, `DRCPassed`, `DRCFailed`, `PhaseCompleted`, `PhaseFailed`. `DRCFailed` is the **loop-back signal** the [orchestrator](../core/workflow-orchestration.md) routes to [Routing Planning](routing-planning.md); `DRCPassed` advances the workflow.

## Rollback

- **Pre-commit:** verification is read-mostly; the only mutations are Violation status and Waivers. A waiver failing authorization in `RecordingWaivers` is abandoned before commit — the violation stays open.
- **Post-commit:** a recorded waiver is reversed by a compensating transition (the [Verification Engine](../engineering/verification-engine.md) reverts the covered violation to *Open*); the audit trail is preserved. Violations are evaluation facts, never deleted.

## Recovery

- **Resumable:** `LoadingPCBIR`, `TriagingViolations`, `AwaitingDisposition`, `RecordingWaivers` — rebuilt by event replay from the last [Checkpoint](../core/checkpoint-system.md).
- **Non-resumable:** `EvaluatingRules` — a crashed evaluation is **re-run** from a clean read of the [PCB IR](../compiler/ir/pcb-ir.md) rather than resumed mid-pass; evaluation is deterministic and idempotent ([P4](../foundation/principles.md)).

## Persistence

Position is event-sourced. Each evaluation run, its inputs, the resulting [Violations](../foundation/engineering-domain-model.md#violation) (stable identity for cross-iteration deduplication), and any [Waivers](../foundation/engineering-domain-model.md#waiver) persist in [Engineering State](../core/shared-state-model.md). The manufacturing gate result is a pure function of the persisted violation set.

## Diagram

```mermaid
stateDiagram-v2
  [*] --> Idle
  Idle --> LoadingPCBIR: activated (guard: routed PCB IR ready)
  LoadingPCBIR --> EvaluatingRules: scope loaded
  EvaluatingRules --> Passed: no open errors
  EvaluatingRules --> TriagingViolations: errors exist
  TriagingViolations --> AwaitingDisposition: needs disposition
  AwaitingDisposition --> RecordingWaivers: waiver authorized
  AwaitingDisposition --> Failed: fix at source
  RecordingWaivers --> EvaluatingRules: re-evaluate gate
  EvaluatingRules --> Failed: unwaived errors persist
  Passed --> [*]
  Failed --> [*]
```
*Figure: the DRC Verification machine. `Failed` is an outcome the [orchestrator](../core/workflow-orchestration.md) turns into a loop-back to [Routing Planning](routing-planning.md). Viewpoint: the runtime.*

## Failure modes

- **Unwaived error violations** → `Failed` → loop-back to [Routing Planning](routing-planning.md). The machine never reroutes itself ([P7](../foundation/principles.md)).
- **Indeterminate rule** (insufficient geometry data) is treated as *not passable* ([Verification Engine](../engineering/verification-engine.md) policy).
- **Expired/out-of-scope waiver** re-arms its covered violation, re-blocking the gate on the next run.

## Related documents

[`agents/drc-agent.md`](../agents/drc-agent.md) · [`engineering/verification-engine.md`](../engineering/verification-engine.md) · [`compiler/ir/pcb-ir.md`](../compiler/ir/pcb-ir.md) · [`core/workflow-orchestration.md`](../core/workflow-orchestration.md) · [`state-machines/routing-planning.md`](routing-planning.md) · [`state-machines/dfm-verification.md`](dfm-verification.md) · [`state-machines/README.md`](README.md)
