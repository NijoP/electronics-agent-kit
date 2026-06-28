//! Phase instances — the concrete state machines and agents (P7). Each conforms to the
//! `eak-runtime` framework and acts only through the runtime's ports.

pub mod agent;
pub mod engineering_analysis;
pub mod requirement_planning;

pub use agent::RequirementAgent;
pub use engineering_analysis::EngineeringAnalysisStub;
pub use requirement_planning::RequirementPlanningMachine;

#[cfg(test)]
mod tests {
    use super::*;
    use eak_domain::{Priority, RequirementCategory};
    use eak_ports::{
        CandidateRequirement, Event, EventLog, EventRecord, ReasoningEngine, ReasoningError,
        ReasoningRequest, ReasoningResponse, Seq, StoreError, Timestamp,
    };
    use eak_runtime::{
        replay, Autonomy, ExecutionEngine, LogicalClock, Orchestrator, PhaseOutcome, RuntimeCore,
        SeededIdSource, WorkflowPlan,
    };

    struct MemLog {
        records: Vec<EventRecord>,
    }
    impl EventLog for MemLog {
        fn append(&mut self, events: &[(Timestamp, Event)]) -> Result<Vec<Seq>, StoreError> {
            let mut seqs = Vec::new();
            for (ts, ev) in events {
                let seq = self.records.len() as u64;
                self.records.push(EventRecord {
                    seq,
                    timestamp: *ts,
                    event: ev.clone(),
                });
                seqs.push(seq);
            }
            Ok(seqs)
        }
        fn read_all(&self) -> Result<Vec<EventRecord>, StoreError> {
            Ok(self.records.clone())
        }
        fn next_seq(&self) -> Seq {
            self.records.len() as u64
        }
    }

    struct CannedReasoner;
    impl ReasoningEngine for CannedReasoner {
        fn model_id(&self) -> String {
            "canned".into()
        }
        fn request_judgement(
            &self,
            _req: &ReasoningRequest,
        ) -> Result<ReasoningResponse, ReasoningError> {
            Ok(ReasoningResponse {
                candidates: vec![
                    CandidateRequirement {
                        statement: "Operating power shall not exceed 5 W".into(),
                        category: RequirementCategory::Electrical,
                        priority: Priority::High,
                        acceptance_criterion: "measured input power < 5 W".into(),
                        source_hint: "intent: < 5 W".into(),
                        confidence: 0.9,
                        rationale: "stated power budget".into(),
                        targets: vec![],
                    },
                    CandidateRequirement {
                        statement: "Board outline shall fit within 50 x 50 mm".into(),
                        category: RequirementCategory::Mechanical,
                        priority: Priority::High,
                        acceptance_criterion: "outline <= 50 mm on each side".into(),
                        source_hint: "intent: < 50x50 mm".into(),
                        confidence: 0.9,
                        rationale: "stated size constraint".into(),
                        targets: vec![],
                    },
                ],
                clarifying_questions: vec![],
                raw: "{}".into(),
            })
        }
    }

    fn core() -> RuntimeCore {
        RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(CannedReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        )
    }

    #[test]
    fn requirement_planning_runs_and_replays() {
        let mut core = core();
        core.capture_intent("USB-C IoT sensor, < 5 W, < 50x50 mm", "engineer")
            .unwrap();
        let mut machine = RequirementPlanningMachine::new();
        let outcome = ExecutionEngine::new().run(&mut machine, &mut core);
        assert_eq!(outcome, PhaseOutcome::Success);
        assert_eq!(core.state.requirements.len(), 2);
        for r in &core.state.requirements {
            assert!(r.validate().is_ok());
        }
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
    }

    #[test]
    fn orchestrator_sequences_both_phases() {
        let mut core = core();
        core.capture_intent("USB-C IoT sensor, < 5 W, < 50x50 mm", "engineer")
            .unwrap();
        let mut plan = WorkflowPlan::new(vec![
            Box::new(RequirementPlanningMachine::new()),
            Box::new(EngineeringAnalysisStub::new()),
        ]);
        let results = Orchestrator::new().run(&mut plan, &mut core);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|(_, o)| *o == PhaseOutcome::Success));
        // multi-phase + replay identity end to end.
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
    }
}
