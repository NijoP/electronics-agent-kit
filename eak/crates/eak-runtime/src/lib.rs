//! The Engineering Runtime — Electronics Agent Kit's deterministic kernel (Use-case ring).
//!
//! This crate is the *mechanism* + *policy* of the architecture (P7): shared state, the
//! commit path, the FSM framework, the execution engine, orchestration, and replay. It
//! depends only on inner rings (`eak-ports`, `eak-domain`) — never on an adapter — which
//! the [`dependency_rule`] guard test enforces at build time (P1).

pub mod clock;
pub mod fsm;
pub mod orchestrator;
pub mod protocol;
pub mod replay;
mod runtime_core;
pub mod state;

pub use clock::{Clock, IdSource, LogicalClock, SeededIdSource, SystemClock};
pub use fsm::{ExecutionEngine, Machine, MachineError, PhaseOutcome, StateKind, StepResult};
pub use orchestrator::{LoopBack, Orchestrator, WorkflowPlan};
pub use protocol::{
    Agent, AgentActivation, AgentContext, AgentOutcome, Autonomy, Budget, CapabilityAck,
    CapabilityError, CapabilityRequest,
};
pub use replay::replay;
pub use runtime_core::RuntimeCore;
pub use state::EngineeringState;

#[cfg(test)]
mod dependency_rule {
    /// The kernel must depend only on inner rings, never on an adapter or instance crate.
    /// If a future change adds such a dependency, this test fails the build (P1).
    #[test]
    fn kernel_has_no_outward_dependencies() {
        let manifest = include_str!("../Cargo.toml");
        for forbidden in [
            "eak-store",
            "eak-reasoning",
            "eak-phases",
            "eak-cli",
            "eak-engines",
            "eak-compiler",
        ] {
            assert!(
                !manifest.contains(forbidden),
                "dependency rule violated: eak-runtime must not depend on {forbidden}"
            );
        }
    }
}

#[cfg(test)]
mod kernel_tests {
    use super::*;
    use eak_domain::{
        Board, BoardSide, BomLineItem, Component, ComponentClass, Decision, EntityId,
        FunctionalBlock, Net, NetClass, Part, PartLifecycle, Pin, PinElectricalType, Placement,
        Priority, Requirement, RequirementCategory, RequirementStatus, Track,
    };
    use eak_ports::{
        Event, EventLog, EventRecord, ReasoningEngine, ReasoningError, ReasoningRequest,
        ReasoningResponse, Seq, StoreError, Timestamp,
    };
    use eak_units::{PhysicalQuantity, Unit};

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

    struct NullReasoner;
    impl ReasoningEngine for NullReasoner {
        fn model_id(&self) -> String {
            "null".into()
        }
        fn request_judgement(
            &self,
            _req: &ReasoningRequest,
        ) -> Result<ReasoningResponse, ReasoningError> {
            Ok(ReasoningResponse {
                candidates: vec![],
                clarifying_questions: vec![],
                raw: String::new(),
            })
        }
    }

    #[test]
    fn commit_then_replay_reconstructs_identical_state() {
        let mut core = RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(NullReasoner),
            Box::new(SeededIdSource::new(42)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        );
        core.capture_intent("USB-C powered IoT sensor node, < 5 W", "engineer")
            .unwrap();
        let src = core.state.intent.as_ref().unwrap().id;
        let rid = core.fresh_id();
        let did = core.fresh_id();
        let req = Requirement {
            id: rid,
            statement: "Operating power shall not exceed 5 W".into(),
            category: RequirementCategory::Electrical,
            priority: Priority::High,
            acceptance_criterion: "measured power < 5 W".into(),
            status: RequirementStatus::Accepted,
            source: src,
            targets: vec![],
        };
        let dec = Decision {
            id: did,
            subject: rid,
            rationale: "derived from intent".into(),
            decider: "test".into(),
            reasoning_call_seq: None,
            evidence: vec![],
            confidence: 1.0,
        };
        core.invoke(CapabilityRequest::CreateRequirement {
            requirement: req,
            decision: dec,
            evidence: vec![],
            links: vec![],
        })
        .unwrap();

        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
        assert_eq!(core.state.canonical_json(), replayed.canonical_json());
        assert_eq!(core.state.requirements.len(), 1);
    }

    fn new_core() -> RuntimeCore {
        RuntimeCore::new(
            Box::new(MemLog { records: vec![] }),
            Box::new(NullReasoner),
            Box::new(SeededIdSource::new(7)),
            Box::new(LogicalClock::new()),
            Autonomy::Autonomous,
        )
    }

    #[test]
    fn phase3_synthesis_commits_fold_and_replay_byte_identically() {
        let mut core = new_core();
        // Commit a real requirement first: the seam now enforces that a block's referenced
        // requirements exist, so the synthesis chain must start from committed intent.
        core.capture_intent("USB-C powered IoT sensor node, 5 V rail", "engineer")
            .unwrap();
        let src = core.state.intent.as_ref().unwrap().id;
        let rid = core.fresh_id();
        let did = core.fresh_id();
        core.invoke(CapabilityRequest::CreateRequirement {
            requirement: Requirement {
                id: rid,
                statement: "Device shall regulate to 5 V".into(),
                category: RequirementCategory::Electrical,
                priority: Priority::High,
                acceptance_criterion: "rail measures 5 V".into(),
                status: RequirementStatus::Accepted,
                source: src,
                targets: vec![],
            },
            decision: Decision {
                id: did,
                subject: rid,
                rationale: "from intent".into(),
                decider: "test".into(),
                reasoning_call_seq: None,
                evidence: vec![],
                confidence: 1.0,
            },
            evidence: vec![],
            links: vec![],
        })
        .unwrap();
        let block = FunctionalBlock {
            id: core.fresh_id(),
            name: "5V regulation".into(),
            function: "step USB-C VBUS down to 5 V".into(),
            requirements: vec![rid],
        };
        let bid = block.id;
        core.invoke(CapabilityRequest::CreateFunctionalBlock {
            block,
            links: vec![],
        })
        .unwrap();

        let comp = Component {
            id: core.fresh_id(),
            refdes: "U1".into(),
            class: ComponentClass::Regulator,
            value: None,
            from_block: bid,
        };
        let cid = comp.id;
        let pin = Pin {
            id: core.fresh_id(),
            component: cid,
            designation: "VOUT".into(),
            electrical_type: PinElectricalType::PowerOut,
        };
        let pid = pin.id;
        core.invoke(CapabilityRequest::RealizeComponent {
            component: comp,
            pins: vec![pin],
            links: vec![],
        })
        .unwrap();

        let nid = core.fresh_id();
        core.invoke(CapabilityRequest::CreateNet {
            net: Net {
                id: nid,
                name: "+5V".into(),
                class: NetClass::Power,
                members: vec![pid],
            },
            links: vec![],
        })
        .unwrap();

        assert_eq!(core.state.functional_blocks.len(), 1);
        assert_eq!(core.state.components.len(), 1);
        assert_eq!(core.state.pins.len(), 1);
        assert_eq!(core.state.nets.len(), 1);
        assert!(core.state.functional_block(bid).is_some());
        assert!(core.state.component(cid).is_some());
        assert!(core.state.pin(pid).is_some());

        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
        assert_eq!(core.state.canonical_json(), replayed.canonical_json());
    }

    /// Drive the full chain down into the BOM layer: requirement -> block -> component, then
    /// a [`Part`] and a [`BomLineItem`] binding the part to that real component. Asserts the
    /// fold landed and that replay reconstructs byte-identical state (the Phase-1 exit
    /// criterion, extended to the BOM deltas).
    #[test]
    fn phase3_bom_commits_fold_and_replay_byte_identically() {
        let mut core = new_core();
        core.capture_intent("USB-C powered IoT sensor node, 3.3 V rail", "engineer")
            .unwrap();
        let src = core.state.intent.as_ref().unwrap().id;
        let rid = core.fresh_id();
        let did = core.fresh_id();
        core.invoke(CapabilityRequest::CreateRequirement {
            requirement: Requirement {
                id: rid,
                statement: "Device shall regulate to 3.3 V".into(),
                category: RequirementCategory::Electrical,
                priority: Priority::High,
                acceptance_criterion: "rail measures 3.3 V".into(),
                status: RequirementStatus::Accepted,
                source: src,
                targets: vec![],
            },
            decision: Decision {
                id: did,
                subject: rid,
                rationale: "from intent".into(),
                decider: "test".into(),
                reasoning_call_seq: None,
                evidence: vec![],
                confidence: 1.0,
            },
            evidence: vec![],
            links: vec![],
        })
        .unwrap();

        let block = FunctionalBlock {
            id: core.fresh_id(),
            name: "3V3 regulation".into(),
            function: "step VBUS down to 3.3 V".into(),
            requirements: vec![rid],
        };
        let bid = block.id;
        core.invoke(CapabilityRequest::CreateFunctionalBlock {
            block,
            links: vec![],
        })
        .unwrap();

        let comp = Component {
            id: core.fresh_id(),
            refdes: "U1".into(),
            class: ComponentClass::Regulator,
            value: None,
            from_block: bid,
        };
        let cid = comp.id;
        let pin = Pin {
            id: core.fresh_id(),
            component: cid,
            designation: "VOUT".into(),
            electrical_type: PinElectricalType::PowerOut,
        };
        core.invoke(CapabilityRequest::RealizeComponent {
            component: comp,
            pins: vec![pin],
            links: vec![],
        })
        .unwrap();

        // The BOM layer: a concrete part, then a line binding it to the real component.
        let part = Part {
            id: core.fresh_id(),
            mpn: "LM1117-3.3".into(),
            manufacturer: "Texas Instruments".into(),
            lifecycle: PartLifecycle::Eol,
            datasheet: "https://ti.com/lm1117".into(),
        };
        let part_id = part.id;
        core.invoke(CapabilityRequest::CreatePart {
            part,
            links: vec![],
        })
        .unwrap();

        let item = BomLineItem {
            id: core.fresh_id(),
            part: part_id,
            components: vec![cid],
            quantity: 1,
        };
        let item_id = item.id;
        core.invoke(CapabilityRequest::CreateBomLineItem {
            item,
            links: vec![],
        })
        .unwrap();

        assert_eq!(core.state.parts.len(), 1);
        assert_eq!(core.state.bom_line_items.len(), 1);
        assert!(core.state.part(part_id).is_some());
        assert!(core.state.bom_line_item(item_id).is_some());

        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
        assert_eq!(core.state.canonical_json(), replayed.canonical_json());
    }

    /// Drive the full chain down into the PCB layer: requirement -> block -> component, then
    /// a [`Board`] outline and a [`Placement`] of that real component on it. Asserts the fold
    /// landed (board + placement) and that replay reconstructs byte-identical state (the
    /// Phase-1 exit criterion, extended to the PCB deltas).
    #[test]
    fn phase3_pcb_commits_fold_and_replay_byte_identically() {
        let mut core = new_core();
        core.capture_intent(
            "USB-C powered IoT sensor node, fits a 50x40 mm board",
            "engineer",
        )
        .unwrap();
        let src = core.state.intent.as_ref().unwrap().id;
        let rid = core.fresh_id();
        let did = core.fresh_id();
        core.invoke(CapabilityRequest::CreateRequirement {
            requirement: Requirement {
                id: rid,
                statement: "Device shall fit a 50x40 mm outline".into(),
                category: RequirementCategory::Electrical,
                priority: Priority::High,
                acceptance_criterion: "board <= 50x40 mm".into(),
                status: RequirementStatus::Accepted,
                source: src,
                targets: vec![],
            },
            decision: Decision {
                id: did,
                subject: rid,
                rationale: "from intent".into(),
                decider: "test".into(),
                reasoning_call_seq: None,
                evidence: vec![],
                confidence: 1.0,
            },
            evidence: vec![],
            links: vec![],
        })
        .unwrap();

        let block = FunctionalBlock {
            id: core.fresh_id(),
            name: "3V3 regulation".into(),
            function: "step VBUS down to 3.3 V".into(),
            requirements: vec![rid],
        };
        let bid = block.id;
        core.invoke(CapabilityRequest::CreateFunctionalBlock {
            block,
            links: vec![],
        })
        .unwrap();

        let comp = Component {
            id: core.fresh_id(),
            refdes: "U1".into(),
            class: ComponentClass::Regulator,
            value: None,
            from_block: bid,
        };
        let cid = comp.id;
        let pin = Pin {
            id: core.fresh_id(),
            component: cid,
            designation: "VOUT".into(),
            electrical_type: PinElectricalType::PowerOut,
        };
        core.invoke(CapabilityRequest::RealizeComponent {
            component: comp,
            pins: vec![pin],
            links: vec![],
        })
        .unwrap();

        // The PCB layer: the board outline must precede any placement (seam ordering, P5).
        let board = Board {
            id: core.fresh_id(),
            width: PhysicalQuantity::new(50.0, Unit::Millimetre),
            height: PhysicalQuantity::new(40.0, Unit::Millimetre),
            layers: 2,
        };
        let board_id = board.id;
        core.invoke(CapabilityRequest::CreateBoard {
            board,
            links: vec![],
        })
        .unwrap();

        let placement = Placement {
            id: core.fresh_id(),
            component: cid,
            x: PhysicalQuantity::new(5.0, Unit::Millimetre),
            y: PhysicalQuantity::new(5.0, Unit::Millimetre),
            width: PhysicalQuantity::new(10.0, Unit::Millimetre),
            height: PhysicalQuantity::new(8.0, Unit::Millimetre),
            side: BoardSide::Top,
        };
        let placement_id = placement.id;
        core.invoke(CapabilityRequest::PlaceComponent {
            placement,
            links: vec![],
        })
        .unwrap();

        assert!(core.state.board.is_some());
        assert_eq!(core.state.placements.len(), 1);
        assert!(core.state.board().is_some());
        assert_eq!(core.state.board().unwrap().id, board_id);
        assert!(core.state.placement(placement_id).is_some());

        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
        assert_eq!(core.state.canonical_json(), replayed.canonical_json());
    }

    /// The BOM seam mirrors the synthesis seam: a line with zero quantity, an empty component
    /// list, an unknown part, or an unknown component is rejected before the commit path.
    #[test]
    fn bom_line_item_handler_rejects_untraceable_proposals_at_the_seam() {
        let mut core = new_core();
        // Pre-mint every id (fresh_id borrows the core mutably, so it cannot run inside an
        // `invoke` argument).
        let fake_part = core.fresh_id();
        let fake_comp = core.fresh_id();
        let pid = core.fresh_id();
        let (id1, id2, id3, id4) = (
            core.fresh_id(),
            core.fresh_id(),
            core.fresh_id(),
            core.fresh_id(),
        );

        // A committed part to isolate the component-integrity check below.
        core.invoke(CapabilityRequest::CreatePart {
            part: Part {
                id: pid,
                mpn: "RC0402FR-0710KL".into(),
                manufacturer: "Yageo".into(),
                lifecycle: PartLifecycle::Active,
                datasheet: "https://yageo.com/rc0402".into(),
            },
            links: vec![],
        })
        .unwrap();

        // Zero quantity.
        let err = core
            .invoke(CapabilityRequest::CreateBomLineItem {
                item: BomLineItem {
                    id: id1,
                    part: pid,
                    components: vec![fake_comp],
                    quantity: 0,
                },
                links: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // No components covered.
        let err = core
            .invoke(CapabilityRequest::CreateBomLineItem {
                item: BomLineItem {
                    id: id2,
                    part: pid,
                    components: vec![],
                    quantity: 1,
                },
                links: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // Unknown part.
        let err = core
            .invoke(CapabilityRequest::CreateBomLineItem {
                item: BomLineItem {
                    id: id3,
                    part: fake_part,
                    components: vec![fake_comp],
                    quantity: 1,
                },
                links: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // Known part but unknown component.
        let err = core
            .invoke(CapabilityRequest::CreateBomLineItem {
                item: BomLineItem {
                    id: id4,
                    part: pid,
                    components: vec![fake_comp],
                    quantity: 1,
                },
                links: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // Nothing landed in the BOM line-item store.
        assert!(core.state.bom_line_items.is_empty());
    }

    #[test]
    fn phase3_handlers_reject_untraceable_proposals_at_the_seam() {
        let mut core = new_core();
        let (id1, id2, id3) = (core.fresh_id(), core.fresh_id(), core.fresh_id());

        // A block realizing no requirement is untraceable to intent (P3).
        let err = core
            .invoke(CapabilityRequest::CreateFunctionalBlock {
                block: FunctionalBlock {
                    id: id1,
                    name: "orphan".into(),
                    function: "f".into(),
                    requirements: vec![],
                },
                links: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // A component minted from no block has no upstream trace.
        let err = core
            .invoke(CapabilityRequest::RealizeComponent {
                component: Component {
                    id: id2,
                    refdes: "R1".into(),
                    class: ComponentClass::Resistor,
                    value: None,
                    from_block: EntityId::NULL,
                },
                pins: vec![],
                links: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // A net joining no pins carries no connectivity.
        let err = core
            .invoke(CapabilityRequest::CreateNet {
                net: Net {
                    id: id3,
                    name: "GND".into(),
                    class: NetClass::Ground,
                    members: vec![],
                },
                links: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // Nothing was committed: every proposal was rejected before the commit path.
        assert!(core.state.functional_blocks.is_empty());
        assert!(core.state.components.is_empty());
        assert!(core.state.nets.is_empty());
    }

    #[test]
    fn phase3_pcb_handlers_reject_untraceable_proposals_at_the_seam() {
        let mut core = new_core();

        // Placing a component before any board outline exists is rejected (board precedes
        // placement — there is nothing to fit against).
        let (pid0, cid0) = (core.fresh_id(), core.fresh_id());
        let err = core
            .invoke(CapabilityRequest::PlaceComponent {
                placement: Placement {
                    id: pid0,
                    component: cid0,
                    x: PhysicalQuantity::new(0.0, Unit::Millimetre),
                    y: PhysicalQuantity::new(0.0, Unit::Millimetre),
                    width: PhysicalQuantity::new(1.0, Unit::Millimetre),
                    height: PhysicalQuantity::new(1.0, Unit::Millimetre),
                    side: BoardSide::Top,
                },
                links: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // Commit a board, then reject a second — a design has exactly one outline.
        let board0 = core.fresh_id();
        core.invoke(CapabilityRequest::CreateBoard {
            board: Board {
                id: board0,
                width: PhysicalQuantity::new(50.0, Unit::Millimetre),
                height: PhysicalQuantity::new(50.0, Unit::Millimetre),
                layers: 2,
            },
            links: vec![],
        })
        .unwrap();
        let board1 = core.fresh_id();
        let err = core
            .invoke(CapabilityRequest::CreateBoard {
                board: Board {
                    id: board1,
                    width: PhysicalQuantity::new(20.0, Unit::Millimetre),
                    height: PhysicalQuantity::new(20.0, Unit::Millimetre),
                    layers: 2,
                },
                links: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // Placing an unrealized component is rejected (referential integrity).
        let (pid1, cid1) = (core.fresh_id(), core.fresh_id());
        let err = core
            .invoke(CapabilityRequest::PlaceComponent {
                placement: Placement {
                    id: pid1,
                    component: cid1,
                    x: PhysicalQuantity::new(1.0, Unit::Millimetre),
                    y: PhysicalQuantity::new(1.0, Unit::Millimetre),
                    width: PhysicalQuantity::new(1.0, Unit::Millimetre),
                    height: PhysicalQuantity::new(1.0, Unit::Millimetre),
                    side: BoardSide::Top,
                },
                links: vec![],
            })
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // Build a real requirement -> block -> component, place it once, then reject a second
        // placement of the same component (a component is placed exactly once).
        core.capture_intent("intent", "engineer").unwrap();
        let src = core.state.intent.as_ref().unwrap().id;
        let rid = core.fresh_id();
        let did = core.fresh_id();
        core.invoke(CapabilityRequest::CreateRequirement {
            requirement: Requirement {
                id: rid,
                statement: "Device shall do a thing".into(),
                category: RequirementCategory::Functional,
                priority: Priority::High,
                acceptance_criterion: "it does the thing".into(),
                status: RequirementStatus::Accepted,
                source: src,
                targets: vec![],
            },
            decision: Decision {
                id: did,
                subject: rid,
                rationale: "from intent".into(),
                decider: "test".into(),
                reasoning_call_seq: None,
                evidence: vec![],
                confidence: 1.0,
            },
            evidence: vec![],
            links: vec![],
        })
        .unwrap();
        let block_id = core.fresh_id();
        core.invoke(CapabilityRequest::CreateFunctionalBlock {
            block: FunctionalBlock {
                id: block_id,
                name: "blk".into(),
                function: "f".into(),
                requirements: vec![rid],
            },
            links: vec![],
        })
        .unwrap();
        let comp_id = core.fresh_id();
        let pin_id = core.fresh_id();
        core.invoke(CapabilityRequest::RealizeComponent {
            component: Component {
                id: comp_id,
                refdes: "U1".into(),
                class: ComponentClass::Ic,
                value: None,
                from_block: block_id,
            },
            pins: vec![Pin {
                id: pin_id,
                component: comp_id,
                designation: "VDD".into(),
                electrical_type: PinElectricalType::PowerIn,
            }],
            links: vec![],
        })
        .unwrap();

        let place = |id: EntityId| CapabilityRequest::PlaceComponent {
            placement: Placement {
                id,
                component: comp_id,
                x: PhysicalQuantity::new(2.0, Unit::Millimetre),
                y: PhysicalQuantity::new(2.0, Unit::Millimetre),
                width: PhysicalQuantity::new(3.0, Unit::Millimetre),
                height: PhysicalQuantity::new(3.0, Unit::Millimetre),
                side: BoardSide::Top,
            },
            links: vec![],
        };
        let p1 = core.fresh_id();
        core.invoke(place(p1)).unwrap();
        let p2 = core.fresh_id();
        let err = core.invoke(place(p2)).unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // Exactly one placement landed, on the single committed board.
        assert_eq!(core.state.placements.len(), 1);
        assert!(core.state.board.is_some());
    }

    /// The routing seam mirrors the placement seam: a track is rejected before any board
    /// exists, when it realizes an unknown net, and on a second route of an already-routed net;
    /// a well-formed track for a committed net on a committed board commits exactly once.
    #[test]
    fn phase3_routing_handler_rejects_untraceable_proposals_at_the_seam() {
        let mut core = new_core();
        let mm = |v: f64| PhysicalQuantity::new(v, Unit::Millimetre);
        let track = |id: EntityId, net: EntityId| CapabilityRequest::RouteNet {
            track: Track {
                id,
                net,
                layer: BoardSide::Top,
                width: mm(0.25),
                x1: mm(1.0),
                y1: mm(1.0),
                x2: mm(9.0),
                y2: mm(1.0),
            },
            links: vec![],
        };

        // Build a real requirement -> block -> component (+ pin) -> net to route.
        core.capture_intent("intent", "engineer").unwrap();
        let src = core.state.intent.as_ref().unwrap().id;
        let rid = core.fresh_id();
        let did = core.fresh_id();
        core.invoke(CapabilityRequest::CreateRequirement {
            requirement: Requirement {
                id: rid,
                statement: "Device shall do a thing".into(),
                category: RequirementCategory::Functional,
                priority: Priority::High,
                acceptance_criterion: "it does the thing".into(),
                status: RequirementStatus::Accepted,
                source: src,
                targets: vec![],
            },
            decision: Decision {
                id: did,
                subject: rid,
                rationale: "from intent".into(),
                decider: "test".into(),
                reasoning_call_seq: None,
                evidence: vec![],
                confidence: 1.0,
            },
            evidence: vec![],
            links: vec![],
        })
        .unwrap();
        let block_id = core.fresh_id();
        core.invoke(CapabilityRequest::CreateFunctionalBlock {
            block: FunctionalBlock {
                id: block_id,
                name: "blk".into(),
                function: "f".into(),
                requirements: vec![rid],
            },
            links: vec![],
        })
        .unwrap();
        let comp_id = core.fresh_id();
        let pin_id = core.fresh_id();
        core.invoke(CapabilityRequest::RealizeComponent {
            component: Component {
                id: comp_id,
                refdes: "U1".into(),
                class: ComponentClass::Ic,
                value: None,
                from_block: block_id,
            },
            pins: vec![Pin {
                id: pin_id,
                component: comp_id,
                designation: "VDD".into(),
                electrical_type: PinElectricalType::PowerIn,
            }],
            links: vec![],
        })
        .unwrap();
        let net_id = core.fresh_id();
        core.invoke(CapabilityRequest::CreateNet {
            net: Net {
                id: net_id,
                name: "VBUS".into(),
                class: NetClass::Power,
                members: vec![pin_id],
            },
            links: vec![],
        })
        .unwrap();

        // Routing before any board outline exists is rejected (board precedes routing).
        let t0 = core.fresh_id();
        let err = core.invoke(track(t0, net_id)).unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // Commit the board, then routing a phantom net is rejected (referential integrity).
        let board_id = core.fresh_id();
        core.invoke(CapabilityRequest::CreateBoard {
            board: Board {
                id: board_id,
                width: mm(50.0),
                height: mm(50.0),
                layers: 2,
            },
            links: vec![],
        })
        .unwrap();
        let t1 = core.fresh_id();
        let phantom = core.fresh_id();
        let err = core.invoke(track(t1, phantom)).unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // Routing the real net once succeeds; a second route of the same net is rejected.
        let t2 = core.fresh_id();
        core.invoke(track(t2, net_id)).unwrap();
        let t3 = core.fresh_id();
        let err = core.invoke(track(t3, net_id)).unwrap_err();
        assert!(matches!(err, CapabilityError::Rejected(_)));

        // Exactly one track landed; replay reconstructs byte-identical state.
        assert_eq!(core.state.tracks.len(), 1);
        let replayed = replay(core.log()).unwrap();
        assert_eq!(core.state, replayed);
        assert_eq!(core.state.canonical_json(), replayed.canonical_json());
    }
}
