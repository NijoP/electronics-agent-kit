//! Phase instances — the concrete state machines and agents (P7). Each conforms to the
//! `eak-runtime` framework and acts only through the runtime's ports.

pub mod agent;
pub mod bom_planning;
pub mod bom_verification;
pub mod constraint_extraction;
pub mod constraint_verification;
pub mod engineering_analysis;
pub mod erc_verification;
pub mod requirement_planning;
pub mod schematic_planning;

pub use agent::RequirementAgent;
pub use bom_planning::BomPlanningMachine;
pub use bom_verification::BomVerificationMachine;
pub use constraint_extraction::ConstraintExtractionMachine;
pub use constraint_verification::ConstraintVerificationMachine;
pub use engineering_analysis::EngineeringAnalysisMachine;
pub use erc_verification::ErcVerificationMachine;
pub use requirement_planning::RequirementPlanningMachine;
pub use schematic_planning::SchematicPlanningMachine;

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
            Box::new(EngineeringAnalysisMachine::new()),
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

    /// A reasoner that yields a functional power-entry requirement (a connector/source) plus
    /// an electrical load, so Schematic Planning synthesizes a driven power rail.
    struct SourcedReasoner;
    impl ReasoningEngine for SourcedReasoner {
        fn model_id(&self) -> String {
            "sourced".into()
        }
        fn request_judgement(
            &self,
            _req: &ReasoningRequest,
        ) -> Result<ReasoningResponse, ReasoningError> {
            use eak_units::{PhysicalQuantity, Unit};
            Ok(ReasoningResponse {
                candidates: vec![
                    CandidateRequirement {
                        statement: "USB-C connector shall supply 5 V to the board".into(),
                        category: RequirementCategory::Functional,
                        priority: Priority::High,
                        acceptance_criterion: "VBUS present at 5 V".into(),
                        source_hint: "intent: USB-C power entry".into(),
                        confidence: 0.9,
                        rationale: "power entry".into(),
                        targets: vec![],
                    },
                    CandidateRequirement {
                        statement: "Operating power shall not exceed 5 W".into(),
                        category: RequirementCategory::Electrical,
                        priority: Priority::High,
                        acceptance_criterion: "measured input power < 5 W".into(),
                        source_hint: "intent: < 5 W".into(),
                        confidence: 0.9,
                        rationale: "power budget".into(),
                        targets: vec![PhysicalQuantity::new(5.0, Unit::Watt)],
                    },
                ],
                clarifying_questions: vec![],
                raw: "{}".into(),
            })
        }
    }

    #[test]
    fn happy_end_to_end_synthesizes_clean_schematic() {
        let mut core = RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(SourcedReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        );
        core.capture_intent("USB-C powered sensor node, < 5 W", "engineer")
            .unwrap();

        // The full Phase-3 chain, run linearly (each phase succeeds, so no loop-back fires).
        let mut plan = WorkflowPlan::new(vec![
            Box::new(RequirementPlanningMachine::new()),
            Box::new(EngineeringAnalysisMachine::new()),
            Box::new(ConstraintExtractionMachine::new()),
            Box::new(ConstraintVerificationMachine::new()),
            Box::new(SchematicPlanningMachine::new()),
            Box::new(ErcVerificationMachine::new()),
        ]);
        let results = Orchestrator::new().run(&mut plan, &mut core);

        assert_eq!(results.len(), 6);
        assert!(results.iter().all(|(_, o)| *o == PhaseOutcome::Success));
        // Architecture, realization, and connectivity all exist and the ERC is clean.
        assert!(!core.state.functional_blocks.is_empty());
        assert!(!core.state.components.is_empty());
        assert!(!core.state.nets.is_empty());
        assert!(core.state.violations.is_empty());

        // replay identity holds across the whole Phase-3 run.
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
    }

    /// A reasoner whose only candidate is an electrical load with no power-entry wording, so
    /// Schematic Planning realizes a consumer with no source — an undriven power net.
    struct LoadOnlyReasoner;
    impl ReasoningEngine for LoadOnlyReasoner {
        fn model_id(&self) -> String {
            "load-only".into()
        }
        fn request_judgement(
            &self,
            _req: &ReasoningRequest,
        ) -> Result<ReasoningResponse, ReasoningError> {
            Ok(ReasoningResponse {
                candidates: vec![CandidateRequirement {
                    statement: "Microcontroller shall run the sensing firmware".into(),
                    category: RequirementCategory::Electrical,
                    priority: Priority::High,
                    acceptance_criterion: "firmware boots and samples".into(),
                    source_hint: "intent: sensing load".into(),
                    confidence: 0.9,
                    rationale: "compute load".into(),
                    targets: vec![],
                }],
                clarifying_questions: vec![],
                raw: "{}".into(),
            })
        }
    }

    #[test]
    fn erc_waiver_lets_reverification_pass() {
        let mut core = RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(LoadOnlyReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        );
        core.capture_intent("battery-only sensor node", "engineer")
            .unwrap();

        // One linear pass: requirements -> architecture -> schematic -> ERC. The load's power
        // rail has no driver, so ERC fails with one open, blocking violation.
        let mut plan = WorkflowPlan::new(vec![
            Box::new(RequirementPlanningMachine::new()),
            Box::new(EngineeringAnalysisMachine::new()),
            Box::new(SchematicPlanningMachine::new()),
            Box::new(ErcVerificationMachine::new()),
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
                justification: "battery-powered prototype: VBUS net left unpopulated".into(),
                decided_by: "engineer".into(),
            },
        })
        .expect("waiver granted");
        assert_eq!(core.state.violations[0].status, ViolationStatus::Waived);
        assert!(!core.state.violations[0].is_blocking());

        // Re-verify ERC: the undriven net is still found, but the violation is waived, so no
        // new violation is raised and nothing blocks — the phase now passes.
        let mut erc = ErcVerificationMachine::new();
        let outcome = ExecutionEngine::new().run(&mut erc, &mut core);
        assert_eq!(outcome, PhaseOutcome::Success);
        assert_eq!(core.state.violations.len(), 1); // no duplicate raised
        assert_eq!(core.state.waivers.len(), 1);

        // replay identity holds across raise + waive + re-verify.
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
    }

    #[test]
    fn happy_end_to_end_synthesizes_clean_bom() {
        let mut core = RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(SourcedReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        );
        core.capture_intent("USB-C powered sensor node, < 5 W", "engineer")
            .unwrap();

        // The full 8-phase chain, run linearly (each phase succeeds, so no loop-back fires).
        let mut plan = WorkflowPlan::new(vec![
            Box::new(RequirementPlanningMachine::new()),
            Box::new(EngineeringAnalysisMachine::new()),
            Box::new(ConstraintExtractionMachine::new()),
            Box::new(ConstraintVerificationMachine::new()),
            Box::new(SchematicPlanningMachine::new()),
            Box::new(ErcVerificationMachine::new()),
            Box::new(BomPlanningMachine::new()),
            Box::new(BomVerificationMachine::new()),
        ]);
        let results = Orchestrator::new().run(&mut plan, &mut core);

        assert_eq!(results.len(), 8);
        assert!(results.iter().all(|(_, o)| *o == PhaseOutcome::Success));
        // The BOM layer exists and the design verifies clean: a connector (Active) source and
        // an IC (Active) load, both covered, no end-of-life parts.
        assert!(!core.state.parts.is_empty());
        assert!(!core.state.bom_line_items.is_empty());
        // Every line item covers >=1 component and orders a known part.
        for item in &core.state.bom_line_items {
            assert!(!item.components.is_empty());
            assert!(core.state.part(item.part).is_some());
        }
        assert!(core.state.violations.is_empty());

        // byte-identical replay holds across the whole 8-phase run.
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
        assert_eq!(core.state.canonical_json(), replayed.canonical_json());
    }

    /// A reasoner that yields a voltage-regulator block (which drives a downstream rail, so the
    /// ERC is clean) plus an electrical load. The regulator's catalog part is end-of-life, so
    /// the BOM gate flags it while the ERC passes.
    struct RegulatorReasoner;
    impl ReasoningEngine for RegulatorReasoner {
        fn model_id(&self) -> String {
            "regulator".into()
        }
        fn request_judgement(
            &self,
            _req: &ReasoningRequest,
        ) -> Result<ReasoningResponse, ReasoningError> {
            Ok(ReasoningResponse {
                candidates: vec![
                    CandidateRequirement {
                        statement: "Voltage regulator shall provide a 3.3 V rail".into(),
                        category: RequirementCategory::Functional,
                        priority: Priority::High,
                        acceptance_criterion: "3.3 V present at VOUT".into(),
                        source_hint: "intent: 3.3 V regulation".into(),
                        confidence: 0.9,
                        rationale: "power conditioning".into(),
                        targets: vec![],
                    },
                    CandidateRequirement {
                        statement: "Microcontroller shall run the sensing firmware".into(),
                        category: RequirementCategory::Electrical,
                        priority: Priority::High,
                        acceptance_criterion: "firmware boots and samples".into(),
                        source_hint: "intent: sensing load".into(),
                        confidence: 0.9,
                        rationale: "compute load".into(),
                        targets: vec![],
                    },
                ],
                clarifying_questions: vec![],
                raw: "{}".into(),
            })
        }
    }

    #[test]
    fn regulator_design_passes_erc_but_bom_gate_catches_eol_part() {
        let mut core = RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(RegulatorReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        );
        core.capture_intent("3.3 V regulated sensor node", "engineer")
            .unwrap();

        // Run the full 8-phase chain linearly. ERC is clean (the regulator's VOUT drives the
        // rail), but BOM Verification fails because the regulator's catalog part is EOL.
        let mut plan = WorkflowPlan::new(vec![
            Box::new(RequirementPlanningMachine::new()),
            Box::new(EngineeringAnalysisMachine::new()),
            Box::new(ConstraintExtractionMachine::new()),
            Box::new(ConstraintVerificationMachine::new()),
            Box::new(SchematicPlanningMachine::new()),
            Box::new(ErcVerificationMachine::new()),
            Box::new(BomPlanningMachine::new()),
            Box::new(BomVerificationMachine::new()),
        ]);
        let results = Orchestrator::new().run(&mut plan, &mut core);
        assert_eq!(results.len(), 8);

        // The ERC phase (index 5) passed — the rail is driven.
        assert_eq!(results[5].1, PhaseOutcome::Success);
        // The final phase (BOM Verification) failed.
        assert!(matches!(results.last().unwrap().1, PhaseOutcome::Failed(_)));
        assert!(!core.state.parts.is_empty());
        assert!(!core.state.bom_line_items.is_empty());

        // Exactly one open, blocking violation: the EOL regulator line.
        assert_eq!(core.state.violations.len(), 1);
        assert!(core.state.violations[0].is_blocking());
        assert_eq!(core.state.violations[0].rule, "bom-lifecycle");

        // Accept the violation via the only write path — the Capability port (P2).
        let vid = core.state.violations[0].id;
        let wid = core.fresh_id();
        core.invoke(CapabilityRequest::GrantWaiver {
            waiver: Waiver {
                id: wid,
                violation: vid,
                justification: "EOL regulator accepted for prototype bring-up".into(),
                decided_by: "engineer".into(),
            },
        })
        .expect("waiver granted");
        assert_eq!(core.state.violations[0].status, ViolationStatus::Waived);
        assert!(!core.state.violations[0].is_blocking());

        // Re-verify BOM: the EOL part is still found, but the violation is waived, so no new
        // violation is raised and nothing blocks — the phase now passes.
        let mut verify = BomVerificationMachine::new();
        let outcome = ExecutionEngine::new().run(&mut verify, &mut core);
        assert_eq!(outcome, PhaseOutcome::Success);
        assert_eq!(core.state.violations.len(), 1); // no duplicate raised
        assert_eq!(core.state.waivers.len(), 1);

        // replay identity holds across the run + waive + re-verify.
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
    }
}
