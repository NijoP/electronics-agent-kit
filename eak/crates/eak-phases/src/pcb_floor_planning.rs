//! PCB Floor Planning state machine (instance) — DETERMINISTIC in Phase 3.
//!
//! It commits the single physical [`Board`] outline the design must fit within — the floor
//! plan that every later placement is checked against (P13: nothing is laid out without a
//! substrate). It makes NO reasoning calls: the outline is sized by a pure scan of the
//! committed requirements (the first length-dimensioned target wins; a fixed default
//! otherwise), so a replay is bit-identical (P4). It is idempotent: if an outline already
//! exists (e.g. on a DRC-loop back the board is untouched) it commits nothing and reports
//! [`StepResult::Done`]. IR projection happens after placement, not here. See
//! `docs/state-machines/pcb-floor-planning.md`.

use eak_domain::{Board, RequirementCategory};
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};
use eak_units::{Dimension, PhysicalQuantity, Unit};

pub struct PcbFloorPlanningMachine;

impl PcbFloorPlanningMachine {
    pub fn new() -> Self {
        Self
    }
}
impl Default for PcbFloorPlanningMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for PcbFloorPlanningMachine {
    fn name(&self) -> &str {
        "PcbFloorPlanning"
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
            "Idle" => Ok(StepResult::Continue("Planning".into())),

            "Planning" => {
                // Idempotent: a design has exactly one outline. A re-entry (DRC loop-back)
                // leaves the committed board untouched and commits nothing.
                if ctx.board().is_some() {
                    return Ok(StepResult::Done);
                }

                // Size the outline deterministically (P4): the first length-dimensioned target
                // on a MECHANICAL requirement (in commit order) sets a square edge — only a
                // mechanical/enclosure requirement bounds the board, never an incidental length
                // target elsewhere (e.g. a wire-length or thermal-clearance figure). Absent any
                // such target, fall back to a generous 100 mm square so an unbounded design is
                // not spuriously failed by DRC. A square keeps the floor plan unambiguous before
                // any real layout heuristics exist (Phase 3 scope).
                let edge = ctx
                    .requirements()
                    .iter()
                    .filter(|r| r.category == RequirementCategory::Mechanical)
                    .flat_map(|r| r.targets.iter())
                    .find(|t| t.dimension() == Dimension::Length)
                    .copied()
                    .unwrap_or_else(|| PhysicalQuantity::new(100.0, Unit::Millimetre));

                let board = Board {
                    id: ctx.fresh_id(),
                    width: edge,
                    height: edge,
                    layers: 2,
                };
                ctx.invoke(CapabilityRequest::CreateBoard {
                    board,
                    links: vec![],
                })
                .map_err(|e| MachineError::Internal(e.to_string()))?;

                Ok(StepResult::Done)
            }

            other => Err(MachineError::Internal(format!("unknown state {other}"))),
        }
    }
}
