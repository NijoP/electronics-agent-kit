//! BOM Verification state machine (instance) — the gate of the BOM correctness loop.
//!
//! Structurally a sibling of [`ErcVerificationMachine`](crate::ErcVerificationMachine), but its
//! [`VerificationEngine`] is loaded with the BOM rules ([`BomCoverageRule`],
//! [`BomLifecycleRule`]) and it runs them over the realized schematic plus its bill of
//! materials (parts, line items). Each *new* finding becomes a first-class [`Violation`] linked
//! back to the subject(s) it implicates so it is fully traceable to its cause (P3), and the
//! [`Event::VerificationCompleted`] milestone is recorded. If any blocking (open,
//! error-severity) violation remains — e.g. an end-of-life part — it reports
//! [`StepResult::Failed`], which the orchestrator routes back to BOM Planning; otherwise the
//! phase is [`StepResult::Done`]. Re-verification is idempotent — an already-raised violation
//! (open OR waived) is never duplicated — so a waiver granted between passes lets the re-verify
//! succeed. See `docs/state-machines/bom-verification.md`.

use eak_domain::{ProvenanceLink, RelationType, Violation, ViolationStatus};
use eak_engines::{BomCoverageRule, BomLifecycleRule, VerificationContext, VerificationEngine};
use eak_ports::Event;
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};

pub struct BomVerificationMachine;

impl BomVerificationMachine {
    pub fn new() -> Self {
        Self
    }

    /// The verification engine for this phase: the two Phase-3 BOM rules registered against
    /// the same generic framework that Constraint and ERC Verification use (reuse: one
    /// framework, many checks).
    fn engine() -> VerificationEngine {
        VerificationEngine::new()
            .with_rule(Box::new(BomCoverageRule::new()))
            .with_rule(Box::new(BomLifecycleRule::new()))
    }
}
impl Default for BomVerificationMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for BomVerificationMachine {
    fn name(&self) -> &str {
        "BomVerification"
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
                let findings = engine.run(&VerificationContext {
                    requirements: &requirements,
                    constraints: &constraints,
                    components: &components,
                    pins: &pins,
                    nets: &nets,
                    parts: &parts,
                    bom_line_items: &bom_line_items,
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
                    // Link the violation to each implicated subject (a component for coverage,
                    // a line item for lifecycle); combined with the BOM's own TracesTo links
                    // this completes the trace Violation -> LineItem/Component -> ... -> Intent.
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
