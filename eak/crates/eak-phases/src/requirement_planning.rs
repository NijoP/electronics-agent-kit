//! Requirement Planning state machine (instance). Phase 1 drives the AUTONOMOUS path of
//! the documented FSM; the human-in-the-loop states are modelled in the doc but inert
//! here. See `docs/state-machines/requirement-planning.md`.

use crate::agent::RequirementAgent;
use eak_compiler::RequirementIr;
use eak_domain::RequirementStatus;
use eak_ports::Event;
use eak_runtime::{
    Agent, AgentActivation, AgentContext, AgentOutcome, Budget, Machine, MachineError, StepResult,
};

pub struct RequirementPlanningMachine {
    agent: RequirementAgent,
    redraft_attempts: u32,
}

impl RequirementPlanningMachine {
    pub fn new() -> Self {
        Self {
            agent: RequirementAgent::new(),
            redraft_attempts: 0,
        }
    }
}
impl Default for RequirementPlanningMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for RequirementPlanningMachine {
    fn name(&self) -> &str {
        "RequirementPlanning"
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
            "Idle" => Ok(StepResult::Continue("CapturingIntent".into())),

            "CapturingIntent" => {
                if ctx.design_intent().is_some() {
                    Ok(StepResult::Continue("StructuringRequirements".into()))
                } else {
                    Ok(StepResult::Failed("intent absent or unreadable".into()))
                }
            }

            "StructuringRequirements" => {
                let activation = AgentActivation {
                    phase: "RequirementPlanning".into(),
                    goal: "structure intent into testable requirements".into(),
                    budget: Budget {
                        max_reasoning_calls: 1,
                    },
                };
                match self.agent.activate(ctx, &activation) {
                    AgentOutcome::Success { .. } => {
                        Ok(StepResult::Continue("CommittingRequirements".into()))
                    }
                    AgentOutcome::NeedsHuman(m) => Ok(StepResult::Failed(format!(
                        "needs human: {m} (HITL deferred)"
                    ))),
                    AgentOutcome::Failed(m) => Ok(StepResult::Failed(m)),
                }
            }

            "CommittingRequirements" => {
                // Project the Requirement IR (P6) and record the boundary milestone.
                let intent = ctx
                    .design_intent()
                    .ok_or_else(|| MachineError::Internal("intent vanished".into()))?;
                let reqs = ctx.requirements();
                let links = ctx.provenance_links();
                let ir = RequirementIr::project(&intent, &reqs, &links)
                    .map_err(|e| MachineError::Internal(e.to_string()))?;
                ctx.emit(vec![Event::RequirementIrProduced {
                    schema_version: ir.schema_version,
                    requirement_count: ir.requirements.len(),
                }])
                .map_err(|e| MachineError::Internal(e.to_string()))?;
                Ok(StepResult::Continue("ValidatingRequirements".into()))
            }

            "ValidatingRequirements" => {
                let reqs = ctx.requirements();
                let has_untestable = reqs
                    .iter()
                    .any(|r| r.status == RequirementStatus::Accepted && !r.is_testable());
                if has_untestable && self.redraft_attempts < 1 {
                    self.redraft_attempts += 1;
                    Ok(StepResult::Continue("StructuringRequirements".into()))
                } else if has_untestable {
                    Ok(StepResult::Failed(
                        "requirements remain untestable after redraft".into(),
                    ))
                } else {
                    Ok(StepResult::Done)
                }
            }

            other => Err(MachineError::Internal(format!("unknown state {other}"))),
        }
    }
}
