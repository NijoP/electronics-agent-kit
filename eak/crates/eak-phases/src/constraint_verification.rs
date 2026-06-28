//! Constraint Verification state machine (instance) — the gate of the correctness loop.
//!
//! It runs the generic [`VerificationEngine`] over the committed requirements and
//! constraints, turns each *new* finding into a first-class [`Violation`] (linked back to
//! the constraints it implicates so it is fully traceable to its cause), and records the
//! [`Event::VerificationCompleted`] milestone. If any blocking (open, error-severity)
//! violation remains it reports [`StepResult::Failed`], which the orchestrator routes back
//! to Constraint Extraction; otherwise the phase is [`StepResult::Done`]. Re-verification is
//! idempotent — an already-raised violation (open OR waived) is never duplicated — so a
//! waiver granted between passes lets the re-verify succeed. See
//! `docs/state-machines/constraint-verification.md`.

use eak_domain::{ProvenanceLink, RelationType, Violation, ViolationStatus};
use eak_engines::{ConstraintConsistencyRule, VerificationContext, VerificationEngine};
use eak_ports::Event;
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};

pub struct ConstraintVerificationMachine;

impl ConstraintVerificationMachine {
    pub fn new() -> Self {
        Self
    }

    /// The verification engine for this phase. Phase 2 registers a single rule; ERC/DRC/DFM
    /// rules plug into the same engine in later phases.
    fn engine() -> VerificationEngine {
        VerificationEngine::new().with_rule(Box::new(ConstraintConsistencyRule::new()))
    }
}
impl Default for ConstraintVerificationMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for ConstraintVerificationMachine {
    fn name(&self) -> &str {
        "ConstraintVerification"
    }

    fn initial(&self) -> String {
        "Idle".into()
    }

    fn step(
        &mut self,
        state: &str,
        ctx: &mut dyn AgentContext,
    ) -> Result<StepResult, MachineError> {
        match state {
            "Idle" => Ok(StepResult::Continue("Verifying".into())),

            "Verifying" => {
                let engine = Self::engine();
                let requirements = ctx.requirements();
                let constraints = ctx.constraints();
                // The schematic slices are empty at this phase (synthesis happens later);
                // passing them keeps the engine's context uniform across verification phases.
                let components = ctx.components();
                let pins = ctx.pins();
                let nets = ctx.nets();
                let parts = ctx.parts();
                let bom_line_items = ctx.bom_line_items();
                // The PCB layer is empty at this phase (the floor plan comes later); binding the
                // owned board/placements to locals keeps the engine's context uniform across all
                // verification phases.
                let board = ctx.board();
                let placements = ctx.placements();
                let tracks = ctx.tracks();
                let findings = engine.run(&VerificationContext {
                    requirements: &requirements,
                    constraints: &constraints,
                    components: &components,
                    pins: &pins,
                    nets: &nets,
                    parts: &parts,
                    bom_line_items: &bom_line_items,
                    board: board.as_ref(),
                    placements: &placements,
                    tracks: &tracks,
                });

                let existing = ctx.violations();
                for finding in &findings {
                    // Dedup by (rule, subjects) against ANY existing violation — open or
                    // waived — so loop-back re-verification never double-raises.
                    let already = existing
                        .iter()
                        .any(|v| v.rule == finding.rule && v.subjects == finding.subjects);
                    if already {
                        continue;
                    }

                    let vid = ctx.fresh_id();
                    let violation = Violation {
                        id: vid,
                        rule: finding.rule.clone(),
                        severity: finding.severity,
                        subjects: finding.subjects.clone(),
                        message: finding.message.clone(),
                        status: ViolationStatus::Open,
                    };
                    // Link the violation to each implicated constraint; combined with the
                    // constraints' own DerivedFrom links this completes the trace
                    // Violation -> Constraint -> Requirement -> Intent.
                    let links: Vec<ProvenanceLink> = finding
                        .subjects
                        .iter()
                        .map(|subject| ProvenanceLink {
                            id: ctx.fresh_id(),
                            from: vid,
                            to: *subject,
                            relation: RelationType::TracesTo,
                        })
                        .collect();

                    ctx.invoke(CapabilityRequest::RaiseViolation { violation, links })
                        .map_err(|e| MachineError::Internal(e.to_string()))?;
                }

                // Re-read after raising so the milestone reflects committed state.
                let open_blocking = ctx.violations().iter().filter(|v| v.is_blocking()).count();
                ctx.emit(vec![Event::VerificationCompleted {
                    rule_count: engine.rule_count(),
                    open_violations: open_blocking,
                }])
                .map_err(|e| MachineError::Internal(e.to_string()))?;

                if open_blocking > 0 {
                    Ok(StepResult::Failed(format!(
                        "{open_blocking} blocking violation(s) open"
                    )))
                } else {
                    Ok(StepResult::Done)
                }
            }

            other => Err(MachineError::Internal(format!("unknown state {other}"))),
        }
    }
}
