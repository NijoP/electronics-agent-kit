//! Engineering Analysis state machine (instance) — DETERMINISTIC in Phase 3.
//!
//! It derives one [`FunctionalBlock`] per accepted requirement that is not yet realized by a
//! block (P3: every block traces to the requirement it satisfies). It makes NO reasoning
//! calls — the stochastic boundary (P3) is untouched, so a reasoning-assisted architecture
//! synthesis is a clean future enhancement. Analysis is idempotent: a requirement that
//! already has a derived block is skipped, so re-running on a correctness-loop back commits
//! nothing new and keeps the run replay-identical. It then projects the [`EngineeringIr`] at
//! the engineering seam (P6) and records the boundary milestone. See
//! `docs/state-machines/engineering-analysis.md`.

use eak_compiler::{EngineeringIr, RequirementIr};
use eak_domain::{FunctionalBlock, ProvenanceLink, RelationType, RequirementStatus};
use eak_ports::Event;
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};

pub struct EngineeringAnalysisMachine;

impl EngineeringAnalysisMachine {
    pub fn new() -> Self {
        Self
    }
}
impl Default for EngineeringAnalysisMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for EngineeringAnalysisMachine {
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
            "Idle" => Ok(StepResult::Continue("Analyzing".into())),

            "Analyzing" => {
                let requirements = ctx.requirements();
                let existing = ctx.functional_blocks();

                for req in &requirements {
                    // Only accepted requirements are realized into architecture.
                    if req.status != RequirementStatus::Accepted {
                        continue;
                    }
                    // Idempotent: skip requirements already realized by a block (so a
                    // loop-back re-analysis is a no-op).
                    if existing.iter().any(|b| b.requirements.contains(&req.id)) {
                        continue;
                    }

                    let bid = ctx.fresh_id();
                    let block = FunctionalBlock {
                        id: bid,
                        name: format!("block-{}", req.id.short()),
                        function: req.statement.clone(),
                        requirements: vec![req.id],
                    };
                    // Provenance: the block derives from the requirement it realizes (P3) —
                    // the trace anchor for every component later minted from it.
                    let link = ProvenanceLink {
                        id: ctx.fresh_id(),
                        from: bid,
                        to: req.id,
                        relation: RelationType::DerivedFrom,
                    };
                    ctx.invoke(CapabilityRequest::CreateFunctionalBlock {
                        block,
                        links: vec![link],
                    })
                    .map_err(|e| MachineError::Internal(e.to_string()))?;
                }

                // Project the Engineering IR (P6) and record the boundary milestone. The
                // projection re-asserts traceability: every block must trace to a known
                // requirement (IrError -> Internal).
                let intent = ctx
                    .design_intent()
                    .ok_or_else(|| MachineError::Internal("no intent for lowering".into()))?;
                let reqs = ctx.requirements();
                let links = ctx.provenance_links();
                let blocks = ctx.functional_blocks();
                let constraints = ctx.constraints();
                let req_ir = RequirementIr::project(&intent, &reqs, &links)
                    .map_err(|e| MachineError::Internal(e.to_string()))?;
                let eng_ir = EngineeringIr::project(&req_ir, &blocks, &constraints)
                    .map_err(|e| MachineError::Internal(e.to_string()))?;
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
