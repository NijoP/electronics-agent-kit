//! Phase instances — the concrete state machines and agents (P7). Each conforms to the
//! `eak-runtime` framework and acts only through the runtime's ports.

pub mod agent;
pub mod bom_planning;
pub mod bom_verification;
pub mod component_placement;
pub mod constraint_extraction;
pub mod constraint_verification;
pub mod dfm_verification;
pub mod drc_verification;
pub mod emc_analysis;
pub mod engineering_analysis;
pub mod erc_verification;
pub mod pcb_floor_planning;
pub mod requirement_planning;
pub mod routing_planning;
pub mod schematic_planning;

pub use agent::RequirementAgent;
pub use bom_planning::BomPlanningMachine;
pub use bom_verification::BomVerificationMachine;
pub use component_placement::ComponentPlacementMachine;
pub use constraint_extraction::ConstraintExtractionMachine;
pub use constraint_verification::ConstraintVerificationMachine;
pub use dfm_verification::DfmVerificationMachine;
pub use drc_verification::DrcVerificationMachine;
pub use emc_analysis::EmcAnalysisMachine;
pub use engineering_analysis::EngineeringAnalysisMachine;
pub use erc_verification::ErcVerificationMachine;
pub use pcb_floor_planning::PcbFloorPlanningMachine;
pub use requirement_planning::RequirementPlanningMachine;
pub use routing_planning::RoutingPlanningMachine;
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

    #[test]
    fn happy_end_to_end_lays_out_clean_pcb() {
        let mut core = RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(SourcedReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        );
        core.capture_intent("USB-C powered sensor node, < 5 W", "engineer")
            .unwrap();

        // The full 14-phase chain, run linearly (each phase succeeds, so no loop-back fires).
        let mut plan = WorkflowPlan::new(vec![
            Box::new(RequirementPlanningMachine::new()),
            Box::new(EngineeringAnalysisMachine::new()),
            Box::new(ConstraintExtractionMachine::new()),
            Box::new(ConstraintVerificationMachine::new()),
            Box::new(SchematicPlanningMachine::new()),
            Box::new(ErcVerificationMachine::new()),
            Box::new(BomPlanningMachine::new()),
            Box::new(BomVerificationMachine::new()),
            Box::new(PcbFloorPlanningMachine::new()),
            Box::new(ComponentPlacementMachine::new()),
            Box::new(RoutingPlanningMachine::new()),
            Box::new(DrcVerificationMachine::new()),
            Box::new(DfmVerificationMachine::new()),
            Box::new(EmcAnalysisMachine::new()),
        ]);
        let results = Orchestrator::new().run(&mut plan, &mut core);

        assert_eq!(results.len(), 14);
        assert!(results.iter().all(|(_, o)| *o == PhaseOutcome::Success));
        // The PCB layer exists: an outline plus one placement per realized component, all
        // within bounds, non-overlapping, and clear of the board-edge keep-out, so both the
        // placement DRC and the DFM gate are clean.
        assert!(core.state.board.is_some());
        assert!(!core.state.placements.is_empty());
        assert_eq!(core.state.placements.len(), core.state.components.len());
        // The routing layer exists: every net is realized by exactly one track, and with no
        // process floor stated the trace-width DRC is clean too. With no operating frequency
        // stated, the EMC antenna-length analysis is silent as well.
        assert!(!core.state.tracks.is_empty());
        assert_eq!(core.state.tracks.len(), core.state.nets.len());
        assert!(core.state.violations.is_empty());

        // byte-identical replay holds across the whole 14-phase run.
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
        assert_eq!(core.state.canonical_json(), replayed.canonical_json());
    }

    /// A reasoner that yields a USB-C connector (the ERC power source), a tight mechanical
    /// board-size limit, and a downstream load. The connector drives the load's rail so the ERC
    /// is clean and both catalog parts are Active so the BOM is clean — but the outline is too
    /// small to fit the whole component row, so the last courtyard runs off the board edge and
    /// DRC flags exactly one out-of-bounds placement.
    struct OversizeReasoner;
    impl ReasoningEngine for OversizeReasoner {
        fn model_id(&self) -> String {
            "oversize".into()
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
                    // The first length-dimensioned target sizes the (square) outline. 24 mm is
                    // wide enough for the connector + first load but not the whole 12 mm-pitch
                    // row, so the trailing courtyard overhangs the edge.
                    CandidateRequirement {
                        statement: "Enclosure limits the board to a 24 mm square outline".into(),
                        category: RequirementCategory::Mechanical,
                        priority: Priority::High,
                        acceptance_criterion: "outline <= 24 mm on each side".into(),
                        source_hint: "intent: enclosure size".into(),
                        confidence: 0.9,
                        rationale: "mechanical envelope".into(),
                        targets: vec![PhysicalQuantity::new(24.0, Unit::Millimetre)],
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
    fn drc_oversize_waiver_lets_reverification_pass() {
        let mut core = RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(OversizeReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        );
        core.capture_intent("USB-C sensor node in a tight enclosure", "engineer")
            .unwrap();

        // Run the full 11-phase chain linearly. ERC and BOM are clean (a driven rail, Active
        // parts), but the outline is too small for the whole component row, so DRC fails with
        // one out-of-bounds courtyard and the linear plan stops there.
        let mut plan = WorkflowPlan::new(vec![
            Box::new(RequirementPlanningMachine::new()),
            Box::new(EngineeringAnalysisMachine::new()),
            Box::new(ConstraintExtractionMachine::new()),
            Box::new(ConstraintVerificationMachine::new()),
            Box::new(SchematicPlanningMachine::new()),
            Box::new(ErcVerificationMachine::new()),
            Box::new(BomPlanningMachine::new()),
            Box::new(BomVerificationMachine::new()),
            Box::new(PcbFloorPlanningMachine::new()),
            Box::new(ComponentPlacementMachine::new()),
            Box::new(RoutingPlanningMachine::new()),
            Box::new(DrcVerificationMachine::new()),
        ]);
        let results = Orchestrator::new().run(&mut plan, &mut core);
        assert_eq!(results.len(), 12);

        // ERC (index 5) and BOM Verification (index 7) both passed.
        assert_eq!(results[5].1, PhaseOutcome::Success);
        assert_eq!(results[7].1, PhaseOutcome::Success);
        // Routing Planning (index 10) ran clean — it routes the placed (if off-board) nets.
        assert_eq!(results[10].1, PhaseOutcome::Success);
        // The final phase (DRC Verification) failed on the off-board courtyard.
        assert_eq!(results.last().unwrap().0, "DrcVerification");
        assert!(matches!(results.last().unwrap().1, PhaseOutcome::Failed(_)));

        // Exactly one open, blocking violation: the courtyard off the board edge.
        assert_eq!(core.state.violations.len(), 1);
        assert!(core.state.violations[0].is_blocking());
        assert_eq!(core.state.violations[0].rule, "drc-out-of-bounds");

        // Accept the violation via the only write path — the Capability port (P2).
        let vid = core.state.violations[0].id;
        let wid = core.fresh_id();
        core.invoke(CapabilityRequest::GrantWaiver {
            waiver: Waiver {
                id: wid,
                violation: vid,
                justification: "courtyard overhang accepted for prototype bring-up".into(),
                decided_by: "engineer".into(),
            },
        })
        .expect("waiver granted");
        assert_eq!(core.state.violations[0].status, ViolationStatus::Waived);
        assert!(!core.state.violations[0].is_blocking());

        // Re-verify DRC: the overhang is still found, but the violation is waived, so no new
        // violation is raised and nothing blocks — the phase now passes.
        let mut drc = DrcVerificationMachine::new();
        let outcome = ExecutionEngine::new().run(&mut drc, &mut core);
        assert_eq!(outcome, PhaseOutcome::Success);
        assert_eq!(core.state.violations.len(), 1); // no duplicate raised
        assert_eq!(core.state.waivers.len(), 1);

        // replay identity holds across the run + waive + re-verify.
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
    }

    /// A reasoner that yields a driven, manufacturable design (a USB-C source + a load, so ERC
    /// and BOM are clean and the default 100 mm board fits the layout) plus a Regulatory
    /// fabrication-process requirement carrying a 0.5 mm trace-width floor. The router routes
    /// every net at the 0.25 mm default, finer than the 0.5 mm process floor, so DRC's
    /// trace-width rule flags each routed track.
    struct TraceFloorReasoner;
    impl ReasoningEngine for TraceFloorReasoner {
        fn model_id(&self) -> String {
            "trace-floor".into()
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
                        statement: "Microcontroller shall run the sensing firmware".into(),
                        category: RequirementCategory::Electrical,
                        priority: Priority::High,
                        acceptance_criterion: "firmware boots and samples".into(),
                        source_hint: "intent: sensing load".into(),
                        confidence: 0.9,
                        rationale: "compute load".into(),
                        targets: vec![],
                    },
                    // A Regulatory requirement whose length target is the fabrication process's
                    // minimum trace width — the floor DRC's trace-width rule checks against.
                    CandidateRequirement {
                        statement: "Fabrication process supports a 0.5 mm minimum trace width"
                            .into(),
                        category: RequirementCategory::Regulatory,
                        priority: Priority::High,
                        acceptance_criterion: "every trace is at least 0.5 mm wide".into(),
                        source_hint: "intent: fab process class".into(),
                        confidence: 0.9,
                        rationale: "process floor".into(),
                        targets: vec![PhysicalQuantity::new(0.5, Unit::Millimetre)],
                    },
                ],
                clarifying_questions: vec![],
                raw: "{}".into(),
            })
        }
    }

    #[test]
    fn drc_trace_width_waiver_lets_reverification_pass() {
        let mut core = RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(TraceFloorReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        );
        core.capture_intent("USB-C sensor node on a coarse fab process", "engineer")
            .unwrap();

        // Run the full 12-phase chain linearly. ERC and BOM are clean (a driven rail, Active
        // parts) and the placement geometry fits the default board, but every routed trace is
        // finer than the 0.5 mm process floor, so DRC fails on the trace-width rule.
        let mut plan = WorkflowPlan::new(vec![
            Box::new(RequirementPlanningMachine::new()),
            Box::new(EngineeringAnalysisMachine::new()),
            Box::new(ConstraintExtractionMachine::new()),
            Box::new(ConstraintVerificationMachine::new()),
            Box::new(SchematicPlanningMachine::new()),
            Box::new(ErcVerificationMachine::new()),
            Box::new(BomPlanningMachine::new()),
            Box::new(BomVerificationMachine::new()),
            Box::new(PcbFloorPlanningMachine::new()),
            Box::new(ComponentPlacementMachine::new()),
            Box::new(RoutingPlanningMachine::new()),
            Box::new(DrcVerificationMachine::new()),
        ]);
        let results = Orchestrator::new().run(&mut plan, &mut core);
        assert_eq!(results.len(), 12);
        assert_eq!(results.last().unwrap().0, "DrcVerification");
        assert!(matches!(results.last().unwrap().1, PhaseOutcome::Failed(_)));

        // Every blocking violation is a trace-width finding — one per routed track — and no
        // other rule fired (placement geometry is clean).
        let tw: Vec<_> = core
            .state
            .violations
            .iter()
            .filter(|v| v.rule == "drc-trace-width")
            .collect();
        assert!(!tw.is_empty());
        assert_eq!(tw.len(), core.state.tracks.len());
        assert!(tw.iter().all(|v| v.is_blocking()));
        assert_eq!(core.state.open_blocking_violations().len(), tw.len());
        // Each violation names a routed track (the traceability anchor back to its net).
        for v in &tw {
            assert_eq!(v.subjects.len(), 1);
            assert!(core.state.track(v.subjects[0]).is_some());
        }

        // Accept every trace-width violation via the only write path — the Capability port (P2).
        let vids: Vec<_> = tw.iter().map(|v| v.id).collect();
        for vid in vids {
            let wid = core.fresh_id();
            core.invoke(CapabilityRequest::GrantWaiver {
                waiver: Waiver {
                    id: wid,
                    violation: vid,
                    justification: "fine traces accepted for prototype bring-up".into(),
                    decided_by: "engineer".into(),
                },
            })
            .expect("waiver granted");
        }
        assert!(core.state.open_blocking_violations().is_empty());

        // Re-verify DRC: the fine traces are still found, but the violations are waived, so no
        // new violation is raised and nothing blocks — the phase now passes.
        let raised_before = core.state.violations.len();
        let mut drc = DrcVerificationMachine::new();
        let outcome = ExecutionEngine::new().run(&mut drc, &mut core);
        assert_eq!(outcome, PhaseOutcome::Success);
        assert_eq!(core.state.violations.len(), raised_before); // no duplicates raised

        // replay identity holds across the run + waive + re-verify.
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
    }

    /// A reasoner like [`OversizeReasoner`] but with a 32.3 mm enclosure: the third component's
    /// courtyard ends at x = 32 mm, so it *fits* the 32.3 mm outline (DRC out-of-bounds passes)
    /// yet sits only 0.3 mm from the right edge — inside the 0.5 mm DFM board-edge keep-out. The
    /// rail is driven (clean ERC) and both parts are Active (clean BOM), so the design reaches
    /// DFM clean and fails only there: the one fault produced at the manufacturability gate.
    struct EdgeClearanceReasoner;
    impl ReasoningEngine for EdgeClearanceReasoner {
        fn model_id(&self) -> String {
            "edge-clearance".into()
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
                    // 32.3 mm square outline: wide enough for the whole 12 mm-pitch row (the last
                    // courtyard ends at 32 mm) but leaving only 0.3 mm of edge margin.
                    CandidateRequirement {
                        statement: "Enclosure limits the board to a 32.3 mm square outline".into(),
                        category: RequirementCategory::Mechanical,
                        priority: Priority::High,
                        acceptance_criterion: "outline <= 32.3 mm on each side".into(),
                        source_hint: "intent: enclosure size".into(),
                        confidence: 0.9,
                        rationale: "mechanical envelope".into(),
                        targets: vec![PhysicalQuantity::new(32.3, Unit::Millimetre)],
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
    fn dfm_edge_clearance_waiver_lets_reverification_pass() {
        let mut core = RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(EdgeClearanceReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        );
        core.capture_intent("USB-C sensor node in a snug enclosure", "engineer")
            .unwrap();

        // Run the 13-phase chain through DFM linearly (EMC is exercised separately below). ERC,
        // BOM, and DRC are clean (a driven rail, Active parts, every courtyard inside the outline),
        // but the trailing component hugs the board edge, so DFM fails on the edge-clearance rule
        // and the linear plan stops there.
        let mut plan = WorkflowPlan::new(vec![
            Box::new(RequirementPlanningMachine::new()),
            Box::new(EngineeringAnalysisMachine::new()),
            Box::new(ConstraintExtractionMachine::new()),
            Box::new(ConstraintVerificationMachine::new()),
            Box::new(SchematicPlanningMachine::new()),
            Box::new(ErcVerificationMachine::new()),
            Box::new(BomPlanningMachine::new()),
            Box::new(BomVerificationMachine::new()),
            Box::new(PcbFloorPlanningMachine::new()),
            Box::new(ComponentPlacementMachine::new()),
            Box::new(RoutingPlanningMachine::new()),
            Box::new(DrcVerificationMachine::new()),
            Box::new(DfmVerificationMachine::new()),
        ]);
        let results = Orchestrator::new().run(&mut plan, &mut core);
        assert_eq!(results.len(), 13);

        // DRC (index 11) passed — the layout fits and the traces are clear.
        assert_eq!(results[11].1, PhaseOutcome::Success);
        // The final phase (DFM Verification) failed.
        assert_eq!(results.last().unwrap().0, "DfmVerification");
        assert!(matches!(results.last().unwrap().1, PhaseOutcome::Failed(_)));

        // Exactly one open, blocking violation: the edge-hugging courtyard.
        assert_eq!(core.state.violations.len(), 1);
        assert!(core.state.violations[0].is_blocking());
        assert_eq!(core.state.violations[0].rule, "dfm-edge-clearance");
        // It names a placement (the traceability anchor back through the component to intent).
        assert_eq!(core.state.violations[0].subjects.len(), 1);
        assert!(core
            .state
            .placement(core.state.violations[0].subjects[0])
            .is_some());

        // Accept the violation via the only write path — the Capability port (P2).
        let vid = core.state.violations[0].id;
        let wid = core.fresh_id();
        core.invoke(CapabilityRequest::GrantWaiver {
            waiver: Waiver {
                id: wid,
                violation: vid,
                justification: "edge keep-out accepted for prototype bring-up".into(),
                decided_by: "engineer".into(),
            },
        })
        .expect("waiver granted");
        assert_eq!(core.state.violations[0].status, ViolationStatus::Waived);
        assert!(!core.state.violations[0].is_blocking());

        // Re-verify DFM: the edge-hugging courtyard is still found, but the violation is waived,
        // so no new violation is raised and nothing blocks — the phase now passes.
        let mut dfm = DfmVerificationMachine::new();
        let outcome = ExecutionEngine::new().run(&mut dfm, &mut core);
        assert_eq!(outcome, PhaseOutcome::Success);
        assert_eq!(core.state.violations.len(), 1); // no duplicate raised
        assert_eq!(core.state.waivers.len(), 1);

        // replay identity holds across the run + waive + re-verify.
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
    }

    /// A reasoner that yields a driven, manufacturable design (a USB-C source + a load, so ERC and
    /// BOM are clean and the default 100 mm board fits the layout, so DRC and DFM are clean too)
    /// plus a high-speed-link requirement carrying a 10 GHz frequency target. Routing realizes every
    /// net as a centroid-to-centroid track far longer than the lambda/10 electrically-long limit
    /// (3 mm at 10 GHz), so EMC Analysis flags each routed track.
    struct HighSpeedReasoner;
    impl ReasoningEngine for HighSpeedReasoner {
        fn model_id(&self) -> String {
            "high-speed".into()
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
                        statement: "Microcontroller shall run the sensing firmware".into(),
                        category: RequirementCategory::Electrical,
                        priority: Priority::High,
                        acceptance_criterion: "firmware boots and samples".into(),
                        source_hint: "intent: sensing load".into(),
                        confidence: 0.9,
                        rationale: "compute load".into(),
                        targets: vec![],
                    },
                    CandidateRequirement {
                        statement: "High-speed serial link shall operate at 10 GHz".into(),
                        category: RequirementCategory::Electrical,
                        priority: Priority::High,
                        acceptance_criterion: "radiated emissions assessed at the 10 GHz line rate"
                            .into(),
                        source_hint: "intent: high-speed link".into(),
                        confidence: 0.9,
                        rationale: "multi-gigabit serial line sets the emission spectrum".into(),
                        targets: vec![PhysicalQuantity::new(10_000.0, Unit::Megahertz)],
                    },
                ],
                clarifying_questions: vec![],
                raw: "{}".into(),
            })
        }
    }

    #[test]
    fn emc_antenna_length_waiver_lets_reverification_pass() {
        let mut core = RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(HighSpeedReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        );
        core.capture_intent(
            "USB-C sensor node with a 10 GHz high-speed link",
            "engineer",
        )
        .unwrap();

        // Run the full 14-phase chain linearly. ERC, BOM, DRC, and DFM are clean (a driven rail,
        // Active parts, a roomy default board), but every routed trace is electrically long at
        // 10 GHz, so EMC Analysis fails on the antenna-length rule and the linear plan stops there.
        let mut plan = WorkflowPlan::new(vec![
            Box::new(RequirementPlanningMachine::new()),
            Box::new(EngineeringAnalysisMachine::new()),
            Box::new(ConstraintExtractionMachine::new()),
            Box::new(ConstraintVerificationMachine::new()),
            Box::new(SchematicPlanningMachine::new()),
            Box::new(ErcVerificationMachine::new()),
            Box::new(BomPlanningMachine::new()),
            Box::new(BomVerificationMachine::new()),
            Box::new(PcbFloorPlanningMachine::new()),
            Box::new(ComponentPlacementMachine::new()),
            Box::new(RoutingPlanningMachine::new()),
            Box::new(DrcVerificationMachine::new()),
            Box::new(DfmVerificationMachine::new()),
            Box::new(EmcAnalysisMachine::new()),
        ]);
        let results = Orchestrator::new().run(&mut plan, &mut core);
        assert_eq!(results.len(), 14);
        assert_eq!(results.last().unwrap().0, "EmcAnalysis");
        assert!(matches!(results.last().unwrap().1, PhaseOutcome::Failed(_)));

        // Every blocking violation is an antenna-length finding — one per routed track — and no
        // other rule fired (the rest of the design is clean). Each names a routed track, the
        // traceability anchor back through its net to intent.
        let antenna: Vec<_> = core
            .state
            .violations
            .iter()
            .filter(|v| v.rule == "emc-antenna-length")
            .collect();
        assert!(!antenna.is_empty());
        assert_eq!(antenna.len(), core.state.tracks.len());
        assert!(antenna.iter().all(|v| v.is_blocking()));
        assert_eq!(core.state.open_blocking_violations().len(), antenna.len());
        for v in &antenna {
            assert_eq!(v.subjects.len(), 1);
            assert!(core.state.track(v.subjects[0]).is_some());
        }

        // Accept every antenna-length violation via the only write path — the Capability port (P2).
        let vids: Vec<_> = antenna.iter().map(|v| v.id).collect();
        for vid in vids {
            let wid = core.fresh_id();
            core.invoke(CapabilityRequest::GrantWaiver {
                waiver: Waiver {
                    id: wid,
                    violation: vid,
                    justification: "long traces accepted for prototype bring-up".into(),
                    decided_by: "engineer".into(),
                },
            })
            .expect("waiver granted");
        }
        assert!(core.state.open_blocking_violations().is_empty());

        // Re-verify EMC: the long traces are still found, but the violations are waived, so no new
        // violation is raised and nothing blocks — the phase now passes. (The tracks still exist,
        // so re-verification genuinely re-runs the rule rather than passing on an empty board.)
        assert!(!core.state.tracks.is_empty());
        let raised_before = core.state.violations.len();
        let mut emc = EmcAnalysisMachine::new();
        let outcome = ExecutionEngine::new().run(&mut emc, &mut core);
        assert_eq!(outcome, PhaseOutcome::Success);
        assert_eq!(core.state.violations.len(), raised_before); // no duplicates raised

        // replay identity holds across the run + waive + re-verify.
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
    }
}
