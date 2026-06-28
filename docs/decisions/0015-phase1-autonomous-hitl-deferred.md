# ADR-0015 — Phase 1 runs autonomously; human-in-the-loop deferred

**Status:** Accepted (Phase 1)

## Context
The [Requirement Planning FSM](../state-machines/requirement-planning.md) models human-in-the-loop
states (`AwaitingClarification`, `ReviewingRequirements`). Phase 1 proves P1–P4 and P8, not
[P10 (human-in-command)](../foundation/principles.md), and has no frontend (that is Phase 3).

## Decision
Run Phase 1 at **`Autonomy::Autonomous`**: the autonomous transition path is exercised; the
HITL states are modelled in the FSM but inert. The capability handler rejects mutations
under `Supervised` autonomy with an explicit "HITL deferred" message rather than silently
proceeding.

## Consequences
- No interactive approval UI is needed to prove the architecture; the driver is a CLI/test
  harness.
- P10 (autonomy levels + approval gates) becomes real in a later phase, behind the same
  ports, without kernel changes.

## Alternatives considered
- Implement the supervised path now via the CLI — added surface that does not advance the
  Phase-1 goal of proving the kernel.
