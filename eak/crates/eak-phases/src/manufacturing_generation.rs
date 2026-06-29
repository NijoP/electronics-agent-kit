//! Manufacturing Generation state machine (instance) — the **terminal** phase of the Phase-3
//! lifecycle and the **global manufacturing gate**.
//!
//! Unlike the per-phase verification gates (ERC/DRC/BOM/DFM/EMC), which each scope their pass/fail
//! to their OWN rules, this phase enforces the cross-phase all-clear that the per-phase gating
//! (increment 5) deliberately reserved for here: it refuses to generate outputs while ANY open,
//! error-severity [`Violation`](eak_domain::Violation) remains anywhere in the design. That guard
//! — "no design with an open blocking defect is ever released to manufacture" — is the
//! [`VerificationEngine`](eak_engines::VerificationEngine)'s most consequential output.
//!
//! When the gate is clear it lowers the routed [`PcbIr`] and the [`BomIr`] into the terminal
//! [`ManufacturingIr`] (the fabrication outline + copper, the assembly pick-and-place, and the
//! procurement BOM joined at one seam), records the [`Event::ManufacturingGenerated`] release
//! milestone, and reports [`StepResult::Done`] (`Released`). When the gate is blocked it reports
//! [`StepResult::Failed`] (`Blocked`) and generates nothing — the design state is untouched. The
//! projection is a pure function of committed state, so a release replays bit-identically (P4).
//!
//! In the canonical lifecycle the [Manufacturing Agent](../../docs/agents/manufacturing-agent.md)
//! drives HITL release approval and writes artifacts to an Artifact Store; this deterministic
//! build auto-releases at `Autonomous` autonomy and treats the projected IR as the artifact. See
//! `docs/state-machines/manufacturing-generation.md`.

use eak_compiler::{BomIr, EngineeringIr, ManufacturingIr, PcbIr, RequirementIr, SchematicIr};
use eak_ports::Event;
use eak_runtime::{AgentContext, Machine, MachineError, StepResult};

pub struct ManufacturingGenerationMachine;

impl ManufacturingGenerationMachine {
    pub fn new() -> Self {
        Self
    }
}
impl Default for ManufacturingGenerationMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for ManufacturingGenerationMachine {
    fn name(&self) -> &str {
        "ManufacturingGeneration"
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
            "Idle" => Ok(StepResult::Continue("CheckingGate".into())),

            "CheckingGate" => {
                // The GLOBAL gate: no open, blocking (open + error-severity) violation may remain
                // ANYWHERE in the design — across every rule-check phase, not just one phase's own
                // rules. A waived violation is not blocking, so an accepted defect does not block
                // release; an unwaived error does.
                let open_blocking = ctx.violations().iter().filter(|v| v.is_blocking()).count();
                if open_blocking > 0 {
                    Ok(StepResult::Failed(format!(
                        "blocked: {open_blocking} open blocking violation(s) remain across the design"
                    )))
                } else {
                    Ok(StepResult::Continue("Generating".into()))
                }
            }

            "Generating" => {
                let mfg = project_manufacturing(ctx)?;
                // Record the release milestone (audit). The Manufacturing IR itself is a
                // projection (re-derivable), so nothing new is folded into engineering state.
                ctx.emit(vec![Event::ManufacturingGenerated {
                    schema_version: mfg.schema_version,
                    place_count: mfg.assignments.len(),
                    copper_count: mfg.copper.len(),
                    line_item_count: mfg.line_items.len(),
                }])
                .map_err(|e| MachineError::Internal(e.to_string()))?;
                Ok(StepResult::Done)
            }

            other => Err(MachineError::Internal(format!("unknown state {other}"))),
        }
    }
}

/// Lower canonical state through the full chain to the routed [`PcbIr`] and the [`BomIr`], then
/// join them into the terminal [`ManufacturingIr`], mapping any `IrError` to an internal machine
/// error. The chain re-derives both seams from committed state so the release is reproducible.
fn project_manufacturing(ctx: &mut dyn AgentContext) -> Result<ManufacturingIr, MachineError> {
    let intent = ctx
        .design_intent()
        .ok_or_else(|| MachineError::Internal("no intent for lowering".into()))?;
    let reqs = ctx.requirements();
    let links = ctx.provenance_links();
    let blocks = ctx.functional_blocks();
    let constraints = ctx.constraints();
    let components = ctx.components();
    let pins = ctx.pins();
    let nets = ctx.nets();
    let parts = ctx.parts();
    let line_items = ctx.bom_line_items();
    let board = ctx.board();
    let placements = ctx.placements();
    let tracks = ctx.tracks();
    let req_ir = RequirementIr::project(&intent, &reqs, &links)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    let eng_ir = EngineeringIr::project(&req_ir, &blocks, &constraints)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    let sch_ir = SchematicIr::project(&eng_ir, &components, &pins, &nets)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    let bom_ir = BomIr::project(&sch_ir, &parts, &line_items)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    let pcb_ir = PcbIr::project(&sch_ir, board.as_ref(), &placements, &tracks)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    ManufacturingIr::project(&pcb_ir, &bom_ir).map_err(|e| MachineError::Internal(e.to_string()))
}
