//! ERC Verification state machine (instance) — the gate of the schematic correctness loop.
//!
//! Structurally a sibling of `ConstraintVerificationMachine`, but its [`VerificationEngine`]
//! is loaded with the ERC rules ([`ErcPowerNetUndrivenRule`], [`ErcMultipleDriversRule`]) and
//! it runs them over the realized schematic (components, pins, nets). Each *new* finding
//! becomes a first-class [`Violation`] linked back to the net(s) it implicates so it is fully
//! traceable to its cause (P3), and the [`Event::VerificationCompleted`] milestone is
//! recorded. If any blocking (open, error-severity) violation remains it reports
//! [`StepResult::Failed`], which the orchestrator routes back to Schematic Planning;
//! otherwise the phase is [`StepResult::Done`]. Re-verification is idempotent — an
//! already-raised violation (open OR waived) is never duplicated — so a waiver granted
//! between passes lets the re-verify succeed. See `docs/state-machines/erc-verification.md`.

use eak_domain::{ProvenanceLink, RelationType, Violation, ViolationStatus};
use eak_engines::{
    ErcMultipleDriversRule, ErcPowerNetUndrivenRule, VerificationContext, VerificationEngine,
};
use eak_ports::Event;
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};

pub struct ErcVerificationMachine;

impl ErcVerificationMachine {
    pub fn new() -> Self {
        Self
    }

    /// The verification engine for this phase: the two Phase-3 ERC rules registered against
    /// the same generic framework that Constraint Verification uses (reuse: one framework,
    /// many checks).
    fn engine() -> VerificationEngine {
        VerificationEngine::new()
            .with_rule(Box::new(ErcPowerNetUndrivenRule::new()))
            .with_rule(Box::new(ErcMultipleDriversRule::new()))
    }
}
impl Default for ErcVerificationMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for ErcVerificationMachine {
    fn name(&self) -> &str {
        "ErcVerification"
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
                    // Link the violation to each implicated net; combined with the schematic's
                    // own DerivedFrom links this completes the trace
                    // Violation -> Net -> ... -> Component -> Block -> Requirement -> Intent.
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
