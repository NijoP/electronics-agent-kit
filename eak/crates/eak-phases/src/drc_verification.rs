//! DRC Verification state machine (instance) — the gate of the PCB correctness loop.
//!
//! Structurally a sibling of [`ErcVerificationMachine`](crate::ErcVerificationMachine), but its
//! [`VerificationEngine`] is loaded with the DRC rules ([`DrcOutOfBoundsRule`],
//! [`DrcCourtyardOverlapRule`]) and it runs them over the physical layer (the board outline
//! plus its placements). Each *new* finding becomes a first-class [`Violation`] linked back to
//! the placement(s) it implicates so it is fully traceable to its cause (P3), and the
//! [`Event::VerificationCompleted`] milestone is recorded. If any blocking (open,
//! error-severity) violation remains — e.g. a courtyard off the board — it reports
//! [`StepResult::Failed`], which the orchestrator routes back to Component Placement; otherwise
//! the phase is [`StepResult::Done`]. Re-verification is idempotent — an already-raised
//! violation (open OR waived) is never duplicated — so a waiver granted between passes lets the
//! re-verify succeed. See `docs/state-machines/drc-verification.md`.

use eak_domain::{ProvenanceLink, RelationType, Violation, ViolationStatus};
use eak_engines::{
    DrcCourtyardOverlapRule, DrcOutOfBoundsRule, VerificationContext, VerificationEngine,
};
use eak_ports::Event;
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};

pub struct DrcVerificationMachine;

impl DrcVerificationMachine {
    pub fn new() -> Self {
        Self
    }

    /// The verification engine for this phase: the two Phase-3 DRC rules registered against the
    /// same generic framework that Constraint, ERC, and BOM Verification use (reuse: one
    /// framework, many checks).
    fn engine() -> VerificationEngine {
        VerificationEngine::new()
            .with_rule(Box::new(DrcOutOfBoundsRule::new()))
            .with_rule(Box::new(DrcCourtyardOverlapRule::new()))
    }
}
impl Default for DrcVerificationMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for DrcVerificationMachine {
    fn name(&self) -> &str {
        "DrcVerification"
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
                // Bind the owned board/placements to locals so their borrows outlive the
                // context the engine reasons over.
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
                    // Link the violation to each implicated placement; combined with the
                    // placements' own TracesTo links this completes the trace
                    // Violation -> Placement -> Component -> Block -> Requirement -> Intent.
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
