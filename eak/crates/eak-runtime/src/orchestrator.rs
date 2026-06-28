//! Workflow orchestration (policy) — owns the phase plan and sequences phases.
//!
//! Phase 1's plan was a linear chain. Phase 2 adds the *correctness loop*: a [`LoopBack`]
//! edge routes a failed phase back to an earlier one (e.g. verification -> extraction) so a
//! detected problem is automatically re-worked, up to a bounded number of retries. A global
//! step cap guarantees the loop always terminates (P13 — no silent infinite loops).

use crate::fsm::{ExecutionEngine, Machine, PhaseOutcome};
use crate::protocol::AgentContext;

/// A correctness-loop edge: when phase `from` fails, re-enter phase `to` (normally an
/// earlier phase) instead of aborting, at most `max_retries` times across the run.
#[derive(Debug, Clone)]
pub struct LoopBack {
    pub from: String,
    pub to: String,
    pub max_retries: u32,
}

/// The phase plan: an ordered list of machines plus any loop-back edges between them. With
/// no loop-backs it degenerates to the Phase-1 linear chain (stop on first failure).
pub struct WorkflowPlan {
    pub phases: Vec<Box<dyn Machine>>,
    pub loopbacks: Vec<LoopBack>,
}
impl WorkflowPlan {
    /// A linear plan with no loop-backs (Phase-1 behaviour).
    pub fn new(phases: Vec<Box<dyn Machine>>) -> Self {
        Self {
            phases,
            loopbacks: Vec::new(),
        }
    }

    /// A plan with correctness-loop edges.
    pub fn with_loopbacks(phases: Vec<Box<dyn Machine>>, loopbacks: Vec<LoopBack>) -> Self {
        Self { phases, loopbacks }
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

    /// Sequence the phases. On a phase failure, take an applicable loop-back edge (jumping
    /// the cursor back to its target) if one still has retry budget; otherwise stop — the
    /// workflow has failed. The returned log lists every phase execution in order, so a
    /// re-worked phase appears more than once.
    pub fn run(
        &self,
        plan: &mut WorkflowPlan,
        ctx: &mut dyn AgentContext,
    ) -> Vec<(String, PhaseOutcome)> {
        let mut results = Vec::new();

        // Owned names so loop-back target resolution doesn't borrow `plan.phases` while we
        // also need to mutably run a phase.
        let names: Vec<String> = plan.phases.iter().map(|p| p.name().to_string()).collect();
        let index_of = |name: &str| names.iter().position(|n| n == name);

        // Per-edge retry budget consumed so far, parallel to `plan.loopbacks`.
        let mut used = vec![0u32; plan.loopbacks.len()];
        let total_retries: u32 = plan.loopbacks.iter().map(|l| l.max_retries).sum();
        // Worst case a phase re-runs once per retry that can reach it; this cap bounds the
        // total executions well above that and turns any cycle into an explicit failure.
        let cap = plan.phases.len() as u32 * (total_retries + 1) + 1;

        let mut cursor = 0usize;
        let mut steps = 0u32;
        while cursor < plan.phases.len() {
            steps += 1;
            if steps > cap {
                results.push((
                    "<workflow>".to_string(),
                    PhaseOutcome::Failed(format!(
                        "workflow step cap {cap} exceeded (possible loop-back cycle)"
                    )),
                ));
                break;
            }

            let outcome = self.engine.run(plan.phases[cursor].as_mut(), &mut *ctx);
            let failed = matches!(outcome, PhaseOutcome::Failed(_));
            results.push((names[cursor].clone(), outcome));

            if !failed {
                cursor += 1;
                continue;
            }

            // Failed: look for a loop-back edge from this phase that still has budget and a
            // resolvable target. Extract owned data so no borrow of `plan` outlives this.
            let recovery = plan
                .loopbacks
                .iter()
                .enumerate()
                .find(|(idx, lb)| {
                    lb.from == names[cursor]
                        && used[*idx] < lb.max_retries
                        && index_of(&lb.to).is_some()
                })
                .map(|(idx, lb)| (idx, lb.to.clone()));

            match recovery {
                Some((idx, to)) => {
                    used[idx] += 1;
                    cursor = index_of(&to).expect("target resolved above");
                }
                None => break, // no recovery path: the workflow fails here.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsm::{MachineError, StepResult};
    use crate::protocol::{Autonomy, CapabilityAck, CapabilityError, CapabilityRequest};
    use eak_domain::{Constraint, DesignIntent, EntityId, ProvenanceLink, Requirement, Violation};
    use eak_ports::{Event, ReasoningError, ReasoningRequest, ReasoningResponse, Seq, StoreError};

    /// A context that only services the engine's phase-lifecycle `emit` calls; the toy
    /// machines below never read state or reason.
    struct NoopCtx {
        next: u128,
    }
    impl AgentContext for NoopCtx {
        fn autonomy(&self) -> Autonomy {
            Autonomy::Autonomous
        }
        fn fresh_id(&mut self) -> EntityId {
            self.next += 1;
            EntityId(self.next)
        }
        fn design_intent(&self) -> Option<DesignIntent> {
            None
        }
        fn requirements(&self) -> Vec<Requirement> {
            vec![]
        }
        fn provenance_links(&self) -> Vec<ProvenanceLink> {
            vec![]
        }
        fn constraints(&self) -> Vec<Constraint> {
            vec![]
        }
        fn violations(&self) -> Vec<Violation> {
            vec![]
        }
        fn reason(
            &mut self,
            _req: ReasoningRequest,
        ) -> Result<(Seq, ReasoningResponse), ReasoningError> {
            Err(ReasoningError::Unavailable)
        }
        fn invoke(&mut self, _req: CapabilityRequest) -> Result<CapabilityAck, CapabilityError> {
            Ok(CapabilityAck { committed: vec![] })
        }
        fn emit(&mut self, _events: Vec<Event>) -> Result<Vec<Seq>, StoreError> {
            Ok(vec![])
        }
    }

    struct AlwaysOk {
        name: &'static str,
    }
    impl Machine for AlwaysOk {
        fn name(&self) -> &str {
            self.name
        }
        fn initial(&self) -> String {
            "go".into()
        }
        fn step(
            &mut self,
            _state: &str,
            _ctx: &mut dyn AgentContext,
        ) -> Result<StepResult, MachineError> {
            Ok(StepResult::Done)
        }
    }

    /// Fails the first `fails_left` times it is entered, then succeeds. The counter persists
    /// across re-runs (the machine instance is not recreated on loop-back).
    struct FailsThenOk {
        name: &'static str,
        fails_left: u32,
    }
    impl Machine for FailsThenOk {
        fn name(&self) -> &str {
            self.name
        }
        fn initial(&self) -> String {
            "go".into()
        }
        fn step(
            &mut self,
            _state: &str,
            _ctx: &mut dyn AgentContext,
        ) -> Result<StepResult, MachineError> {
            if self.fails_left > 0 {
                self.fails_left -= 1;
                Ok(StepResult::Failed("not yet".into()))
            } else {
                Ok(StepResult::Done)
            }
        }
    }

    fn count(results: &[(String, PhaseOutcome)], name: &str) -> usize {
        results.iter().filter(|(n, _)| n == name).count()
    }

    #[test]
    fn loop_back_re_runs_until_phase_recovers() {
        let mut plan = WorkflowPlan::with_loopbacks(
            vec![
                Box::new(AlwaysOk { name: "A" }),
                Box::new(FailsThenOk {
                    name: "B",
                    fails_left: 2,
                }),
                Box::new(AlwaysOk { name: "C" }),
            ],
            vec![LoopBack {
                from: "B".into(),
                to: "A".into(),
                max_retries: 3,
            }],
        );
        let mut ctx = NoopCtx { next: 0 };
        let results = Orchestrator::new().run(&mut plan, &mut ctx);

        // B failed twice, recovered on the third entry; the loop reached C and finished.
        assert_eq!(count(&results, "B"), 3);
        assert_eq!(count(&results, "A"), 3); // re-run each time we looped back
        assert_eq!(results.last().unwrap().0, "C");
        assert_eq!(results.last().unwrap().1, PhaseOutcome::Success);
    }

    #[test]
    fn loop_back_gives_up_when_retries_exhausted() {
        let mut plan = WorkflowPlan::with_loopbacks(
            vec![
                Box::new(AlwaysOk { name: "A" }),
                Box::new(FailsThenOk {
                    name: "B",
                    fails_left: 99,
                }),
            ],
            vec![LoopBack {
                from: "B".into(),
                to: "A".into(),
                max_retries: 2,
            }],
        );
        let mut ctx = NoopCtx { next: 0 };
        let results = Orchestrator::new().run(&mut plan, &mut ctx);

        // initial entry + 2 retries = 3 attempts, then no budget -> stop, last is B failed.
        assert_eq!(count(&results, "B"), 3);
        let (last_name, last_outcome) = results.last().unwrap();
        assert_eq!(last_name, "B");
        assert!(matches!(last_outcome, PhaseOutcome::Failed(_)));
    }

    #[test]
    fn linear_plan_stops_on_first_failure() {
        let mut plan = WorkflowPlan::new(vec![
            Box::new(AlwaysOk { name: "A" }),
            Box::new(FailsThenOk {
                name: "B",
                fails_left: 1,
            }),
            Box::new(AlwaysOk { name: "C" }),
        ]);
        let mut ctx = NoopCtx { next: 0 };
        let results = Orchestrator::new().run(&mut plan, &mut ctx);

        // no loop-backs: A ok, B fails, C never runs (Phase-1 behaviour preserved).
        assert_eq!(results.len(), 2);
        assert_eq!(count(&results, "C"), 0);
        assert!(matches!(results[1].1, PhaseOutcome::Failed(_)));
    }
}
