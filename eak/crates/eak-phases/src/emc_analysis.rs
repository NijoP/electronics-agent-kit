//! EMC Analysis state machine (instance) — the electromagnetic-compatibility gate.
//!
//! Structurally a sibling of [`DfmVerificationMachine`](crate::DfmVerificationMachine): its
//! [`VerificationEngine`] is loaded with the EMC rule set ([`EmcAntennaLengthRule`]) and it runs
//! it over the routed physical layer (the board's tracks, sized against the design's stated
//! operating/emission frequency). It runs only after DFM passes — the last gate of the Phase-3
//! lifecycle before Manufacturing. Each *new* finding becomes a first-class [`Violation`] linked
//! back to the track(s) it implicates so it is fully traceable to its cause (P3), and the
//! [`Event::VerificationCompleted`] milestone is recorded. If any blocking (open, error-severity)
//! violation remains — e.g. an electrically-long trace acting as an antenna — it reports
//! [`StepResult::Failed`], which the orchestrator routes back to **Routing Planning** (emissions
//! and coupling are routing-dominated: a re-route is what changes the trace geometry); otherwise
//! the phase is [`StepResult::Done`]. Re-verification is idempotent — an already-raised violation
//! (open OR waived) is never duplicated — so a waiver granted between passes lets the re-verify
//! succeed.
//!
//! In the full lifecycle EMC is *analysis* — it interprets simulated fields against limits via an
//! external Simulation port (see `docs/state-machines/emc-analysis.md`). This machine is the
//! deterministic Phase-3 subset: a closed-form geometric proxy (electrically-long-trace emission)
//! on the same generic [`VerificationEngine`] framework as ERC/DRC/DFM. The external-simulation,
//! Analysis-Result path remains the documented target and is deferred.
//!
//! Per-phase gating (increment 5): the EMC pass/fail gate scopes to its OWN rule via
//! [`VerificationEngine::count_open_blocking`], so an open violation from a different rule-check
//! phase seen while EMC re-runs on a loop-back is the Manufacturing gate's concern, not this
//! phase's. See `docs/state-machines/emc-analysis.md`.

use eak_domain::{ProvenanceLink, RelationType, Violation, ViolationStatus};
use eak_engines::{EmcAntennaLengthRule, VerificationContext, VerificationEngine};
use eak_ports::Event;
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};

pub struct EmcAnalysisMachine;

impl EmcAnalysisMachine {
    pub fn new() -> Self {
        Self
    }

    /// The verification engine for this phase: the Phase-3 EMC rule registered against the same
    /// generic framework that Constraint, ERC, BOM, DRC, and DFM Verification use (reuse: one
    /// framework, many checks).
    fn engine() -> VerificationEngine {
        VerificationEngine::new().with_rule(Box::new(EmcAntennaLengthRule::new()))
    }
}
impl Default for EmcAnalysisMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for EmcAnalysisMachine {
    fn name(&self) -> &str {
        "EmcAnalysis"
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
            "Idle" => Ok(StepResult::Continue("Analyzing".into())),

            "Analyzing" => {
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
                    // Link the violation to each implicated track. Combined with the track's own
                    // TracesTo link to the net it realizes, this completes the trace back to
                    // intent: Violation -> Track -> Net -> ... -> Requirement -> Intent.
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
                // DRC or DFM violation seen while EMC re-runs on a routing loop-back, is the
                // Manufacturing gate's concern, not this phase's.
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
