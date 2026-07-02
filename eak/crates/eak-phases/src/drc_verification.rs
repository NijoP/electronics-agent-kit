//! DRC Verification state machine (instance) — the gate of the PCB correctness loop.
//!
//! Structurally a sibling of [`ErcVerificationMachine`](crate::ErcVerificationMachine), but its
//! [`VerificationEngine`] is loaded with the DRC rules ([`DrcOutOfBoundsRule`],
//! [`DrcCourtyardOverlapRule`], [`DrcTraceWidthRule`], [`DrcUnroutedNetRule`],
//! [`DrcNetOpenRule`], [`DrcCopperClearanceRule`]) and it runs them
//! over the physical layer (the board outline, its placements, the committed nets, and the routed
//! tracks). Each *new* finding becomes a
//! first-class [`Violation`] linked back to the placement(s) or track(s) it implicates so it is
//! fully traceable to its cause (P3), and the [`Event::VerificationCompleted`] milestone is
//! recorded. If any blocking (open, error-severity) violation remains — e.g. a courtyard off the
//! board or a trace finer than the process floor — it reports [`StepResult::Failed`], which the
//! orchestrator routes back to Routing Planning; otherwise the phase is [`StepResult::Done`].
//! Re-verification is idempotent — an already-raised violation (open OR waived) is never
//! duplicated — so a waiver granted between passes lets the re-verify succeed. See
//! `docs/state-machines/drc-verification.md`.

use eak_domain::{ProvenanceLink, RelationType, Violation, ViolationStatus};
use eak_engines::{
    DrcAmpacityWidthRule, DrcCopperClearanceRule, DrcCourtyardOverlapRule, DrcNetOpenRule,
    DrcOutOfBoundsRule, DrcTraceWidthRule, DrcUnroutedNetRule, VerificationContext,
    VerificationEngine,
};
use eak_ports::Event;
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};

pub struct DrcVerificationMachine;

impl DrcVerificationMachine {
    pub fn new() -> Self {
        Self
    }

    /// The verification engine for this phase: the seven Phase-3 DRC rules — two placement geometry
    /// checks, the routing trace-width (process-floor) check, the net-realization completeness
    /// check, the net-connectivity (open-detection) check, the copper-to-copper clearance
    /// (short-margin) check, and the ampacity trace-width (current-carrying) check — registered
    /// against the same generic framework that Constraint, ERC, and BOM Verification use (reuse:
    /// one framework, many checks).
    fn engine() -> VerificationEngine {
        VerificationEngine::new()
            .with_rule(Box::new(DrcOutOfBoundsRule::new()))
            .with_rule(Box::new(DrcCourtyardOverlapRule::new()))
            .with_rule(Box::new(DrcTraceWidthRule::new()))
            .with_rule(Box::new(DrcUnroutedNetRule::new()))
            .with_rule(Box::new(DrcNetOpenRule::new()))
            .with_rule(Box::new(DrcCopperClearanceRule::new()))
            .with_rule(Box::new(DrcAmpacityWidthRule::new()))
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
                // Bind the owned board/placements/tracks to locals so their borrows outlive the
                // context the engine reasons over.
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
                    // Link the violation to each implicated subject — a placement (geometry
                    // rules) or a track (trace-width rule). Combined with the subject's own
                    // TracesTo links this completes the trace back to intent, e.g.
                    // Violation -> Placement -> Component -> Block -> Requirement -> Intent, or
                    // Violation -> Track -> Net -> ... -> Requirement -> Intent.
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

                // Re-read after raising so the gate reflects committed state. The phase fails on
                // ITS OWN open, blocking findings only (`count_open_blocking` scopes to this
                // engine's rules) — an open violation from a different rule-check phase, e.g. a
                // DFM violation seen while DRC re-runs on a DFM loop-back, is the Manufacturing
                // gate's concern, not this phase's.
                let open_blocking = engine.count_open_blocking(&ctx.violations());
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
