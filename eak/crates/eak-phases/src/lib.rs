//! Phase instances — the concrete state machines and agents (P7). Each conforms to the
//! `eak-runtime` framework and acts only through the runtime's ports.

pub mod agent;
pub mod constraint_extraction;
pub mod constraint_verification;
pub mod engineering_analysis;
pub mod requirement_planning;

pub use agent::RequirementAgent;
pub use constraint_extraction::ConstraintExtractionMachine;
pub use constraint_verification::ConstraintVerificationMachine;
pub use engineering_analysis::EngineeringAnalysisStub;
pub use requirement_planning::RequirementPlanningMachine;

#[cfg(test)]
mod tests {
    use super::*;
    use eak_domain::{Priority, RequirementCategory, ViolationStatus, Waiver};
    use eak_ports::{
        CandidateRequirement, Event, EventLog, EventRecord, ReasoningEngine, ReasoningError,
        ReasoningRequest, ReasoningResponse, Seq, StoreError, Timestamp,
    };
    use eak_runtime::{
        replay, AgentContext, Autonomy, CapabilityRequest, ExecutionEngine, LogicalClock,
        Orchestrator, PhaseOutcome, RuntimeCore, SeededIdSource, WorkflowPlan,
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

    /// A reasoner whose candidates carry typed targets, so Constraint Extraction has
    /// something to derive constraints from.
    struct TargetedReasoner;
    impl ReasoningEngine for TargetedReasoner {
        fn model_id(&self) -> String {
            "targeted".into()
        }
        fn request_judgement(
            &self,
            _req: &ReasoningRequest,
        ) -> Result<ReasoningResponse, ReasoningError> {
            use eak_units::{PhysicalQuantity, Unit};
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
                        targets: vec![PhysicalQuantity::new(5.0, Unit::Watt)],
                    },
                    CandidateRequirement {
                        statement: "Board outline shall fit within 50 mm".into(),
                        category: RequirementCategory::Mechanical,
                        priority: Priority::High,
                        acceptance_criterion: "outline <= 50 mm".into(),
                        source_hint: "intent: < 50 mm".into(),
                        confidence: 0.9,
                        rationale: "stated size".into(),
                        targets: vec![PhysicalQuantity::new(50.0, Unit::Millimetre)],
                    },
                ],
                clarifying_questions: vec![],
                raw: "{}".into(),
            })
        }
    }

    #[test]
    fn extraction_then_verification_pass_for_consistent_design() {
        let mut core = RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(TargetedReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        );
        core.capture_intent("USB-C IoT sensor, < 5 W, < 50 mm", "engineer")
            .unwrap();
        let mut plan = WorkflowPlan::new(vec![
            Box::new(RequirementPlanningMachine::new()),
            Box::new(ConstraintExtractionMachine::new()),
            Box::new(ConstraintVerificationMachine::new()),
        ]);
        let results = Orchestrator::new().run(&mut plan, &mut core);

        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|(_, o)| *o == PhaseOutcome::Success));
        assert_eq!(core.state.requirements.len(), 2);
        assert_eq!(core.state.constraints.len(), 2);
        assert!(core.state.violations.is_empty());

        // every constraint is rooted in its requirement (the trace anchor).
        for c in &core.state.constraints {
            assert!(core.state.requirement(c.subject_requirement).is_some());
            assert!(core
                .state
                .links
                .iter()
                .any(|l| l.from == c.id && l.to == c.subject_requirement));
        }

        // replay identity still holds with the Phase-2 event types.
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
    }

    /// A reasoner whose two power requirements cannot both hold (<= 5 W and >= 8 W).
    struct ContradictoryReasoner;
    impl ReasoningEngine for ContradictoryReasoner {
        fn model_id(&self) -> String {
            "contradictory".into()
        }
        fn request_judgement(
            &self,
            _req: &ReasoningRequest,
        ) -> Result<ReasoningResponse, ReasoningError> {
            use eak_units::{PhysicalQuantity, Unit};
            Ok(ReasoningResponse {
                candidates: vec![
                    CandidateRequirement {
                        statement: "Operating power shall not exceed 5 W".into(),
                        category: RequirementCategory::Electrical,
                        priority: Priority::High,
                        acceptance_criterion: "measured input power < 5 W".into(),
                        source_hint: "intent: <= 5 W".into(),
                        confidence: 0.9,
                        rationale: "power ceiling".into(),
                        targets: vec![PhysicalQuantity::new(5.0, Unit::Watt)],
                    },
                    CandidateRequirement {
                        statement: "Operating power shall be at least 8 W".into(),
                        category: RequirementCategory::Electrical,
                        priority: Priority::High,
                        acceptance_criterion: "measured input power > 8 W".into(),
                        source_hint: "intent: >= 8 W".into(),
                        confidence: 0.9,
                        rationale: "power floor".into(),
                        targets: vec![PhysicalQuantity::new(8.0, Unit::Watt)],
                    },
                ],
                clarifying_questions: vec![],
                raw: "{}".into(),
            })
        }
    }

    #[test]
    fn waiver_lets_reverification_pass() {
        let mut core = RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(ContradictoryReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        );
        core.capture_intent("contradictory power budget", "engineer")
            .unwrap();

        // One linear pass: requirements -> constraints -> verification. The contradiction is
        // caught and verification fails with one open, blocking violation.
        let mut plan = WorkflowPlan::new(vec![
            Box::new(RequirementPlanningMachine::new()),
            Box::new(ConstraintExtractionMachine::new()),
            Box::new(ConstraintVerificationMachine::new()),
        ]);
        let results = Orchestrator::new().run(&mut plan, &mut core);
        assert!(matches!(results.last().unwrap().1, PhaseOutcome::Failed(_)));
        assert_eq!(core.state.violations.len(), 1);
        assert!(core.state.violations[0].is_blocking());

        // Accept the violation via the only write path — the Capability port (P2).
        let vid = core.state.violations[0].id;
        let wid = core.fresh_id();
        core.invoke(CapabilityRequest::GrantWaiver {
            waiver: Waiver {
                id: wid,
                violation: vid,
                justification: "accepted for prototype bring-up".into(),
                decided_by: "engineer".into(),
            },
        })
        .expect("waiver granted");
        assert_eq!(core.state.violations[0].status, ViolationStatus::Waived);
        assert!(!core.state.violations[0].is_blocking());

        // Re-verify: the contradiction is still found, but the violation is waived, so no new
        // violation is raised and nothing blocks — the phase now passes.
        let mut verify = ConstraintVerificationMachine::new();
        let outcome = ExecutionEngine::new().run(&mut verify, &mut core);
        assert_eq!(outcome, PhaseOutcome::Success);
        assert_eq!(core.state.violations.len(), 1); // no duplicate raised
        assert_eq!(core.state.waivers.len(), 1);

        // replay identity holds across raise + waive + re-verify.
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
    }
}
