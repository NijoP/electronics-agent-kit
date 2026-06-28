//! Engineering Analysis — a STUB second phase (P13: clearly marked, not a real phase).
//!
//! It exists only to prove the orchestrator sequences more than one phase and that the
//! Requirement IR -> Engineering IR lowering seam works. It performs the trivial lowering
//! and records the boundary milestone; it adds no engineering content.

use eak_compiler::{lower_to_engineering_ir, RequirementIr};
use eak_ports::Event;
use eak_runtime::{AgentContext, Machine, MachineError, StepResult};

pub struct EngineeringAnalysisStub;

impl EngineeringAnalysisStub {
    pub fn new() -> Self {
        Self
    }
}
impl Default for EngineeringAnalysisStub {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for EngineeringAnalysisStub {
    fn name(&self) -> &str {
        "EngineeringAnalysis"
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
            "Idle" => Ok(StepResult::Continue("Lowering".into())),

            "Lowering" => {
                let intent = ctx
                    .design_intent()
                    .ok_or_else(|| MachineError::Internal("no intent for lowering".into()))?;
                let reqs = ctx.requirements();
                let links = ctx.provenance_links();
                let req_ir = RequirementIr::project(&intent, &reqs, &links)
                    .map_err(|e| MachineError::Internal(e.to_string()))?;
                let eng_ir = lower_to_engineering_ir(&req_ir);
                ctx.emit(vec![Event::EngineeringIrProduced {
                    schema_version: eng_ir.schema_version,
                    block_count: eng_ir.blocks.len(),
                }])
                .map_err(|e| MachineError::Internal(e.to_string()))?;
                Ok(StepResult::Done)
            }

            other => Err(MachineError::Internal(format!("unknown state {other}"))),
        }
    }
}
