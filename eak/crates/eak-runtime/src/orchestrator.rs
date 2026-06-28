//! Workflow orchestration (policy) — owns the phase plan and sequences phases.
//!
//! Phase 1's plan is a linear chain (Requirement Planning -> Engineering Analysis stub);
//! there are no loop-backs or gates yet. Advancement is outcome-driven, not time-driven.

use crate::fsm::{ExecutionEngine, Machine, PhaseOutcome};
use crate::protocol::AgentContext;

/// The phase DAG. Phase 1 uses a linear sequence of machines.
pub struct WorkflowPlan {
    pub phases: Vec<Box<dyn Machine>>,
}
impl WorkflowPlan {
    pub fn new(phases: Vec<Box<dyn Machine>>) -> Self {
        Self { phases }
    }
}

pub struct Orchestrator {
    engine: ExecutionEngine,
}
impl Orchestrator {
    pub fn new() -> Self {
        Self {
            engine: ExecutionEngine::new(),
        }
    }

    /// Run each phase in order; stop on the first failure (no loop-backs in Phase 1).
    pub fn run(
        &self,
        plan: &mut WorkflowPlan,
        ctx: &mut dyn AgentContext,
    ) -> Vec<(String, PhaseOutcome)> {
        let mut results = Vec::new();
        for phase in plan.phases.iter_mut() {
            let name = phase.name().to_string();
            let outcome = self.engine.run(phase.as_mut(), &mut *ctx);
            let failed = matches!(outcome, PhaseOutcome::Failed(_));
            results.push((name, outcome));
            if failed {
                break;
            }
        }
        results
    }
}
impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}
