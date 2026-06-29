//! End-to-end verification of the Phase-3 exit criteria over the full 15-phase workflow:
//! Requirement Planning -> Engineering Analysis -> Constraint Extraction -> Constraint
//! Verification -> Schematic Planning -> ERC Verification -> BOM Planning -> BOM Verification
//! -> PCB Floor Planning -> Component Placement -> Routing Planning -> DRC Verification -> DFM
//! Verification -> EMC Analysis -> Manufacturing Generation. A consistent, realizable design runs
//! all fifteen phases clean and is RELEASED — lowered to a Manufacturing IR once the global gate
//! finds no open blocking violation. Seven kinds of
//! fault are each caught at their gate, routed back automatically, and left fully traceable to
//! their cause: an infeasible constraint pair (Constraint Verification), an electrically-invalid
//! power net with consumers but no driver (ERC), a procurement fault — an end-of-life catalog
//! part — at the BOM lifecycle gate, a courtyard that runs off an undersized board outline (DRC
//! out-of-bounds), a trace finer than the fabrication process floor (DRC trace-width), a
//! component inside the board-edge keep-out (DFM edge-clearance), and an electrically-long trace
//! that radiates at the stated operating frequency (EMC antenna-length). The first three faults
//! stop the workflow before any of the PCB phases run; the DRC and EMC faults loop back to
//! Routing Planning, and the DFM fault loops back to Component Placement. The Phase-1/2 guarantees
//! (multi-phase orchestration, full requirement traceability, the correctness loops, byte-identical
//! replay) still hold over the larger workflow.

use eak_cli::{
    replay_cmd, run, run_with, trace_cmd, PhaseOutcome, ReasoningChoice, Relation, RunConfig,
};
use eak_domain::{
    PartLifecycle, Priority, RequirementCategory, ViolationSeverity, ViolationStatus,
};
use eak_ports::{CandidateRequirement, ReasoningEngine, ReasoningResponse};
use eak_reasoning::FixtureEngine;
use eak_units::{PhysicalQuantity, Unit};
use std::path::PathBuf;

fn temp_log(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("eak-cli-it-{}-{}.jsonl", tag, std::process::id()));
    p
}

fn cfg(tag: &str, seed: u64) -> (RunConfig, PathBuf) {
    let log = temp_log(tag);
    let _ = std::fs::remove_file(&log);
    (
        RunConfig {
            intent: "USB-C powered IoT sensor node, < 5 W, < 50x50 mm".into(),
            reasoning: ReasoningChoice::Fixture,
            cassette: None,
            log: log.clone(),
            model: "fixture".into(),
            seed,
            deterministic_clock: true,
        },
        log,
    )
}

/// A reasoning engine returning two power requirements whose targets cannot both hold
/// (<= 5 W and >= 8 W) — drives the infeasible-constraint scenario.
fn contradictory_engine() -> Box<dyn ReasoningEngine> {
    let power_max = CandidateRequirement {
        statement: "Operating power shall not exceed 5 W".into(),
        category: RequirementCategory::Electrical,
        priority: Priority::High,
        acceptance_criterion: "measured input power < 5 W".into(),
        source_hint: "intent: <= 5 W".into(),
        confidence: 0.9,
        rationale: "stated power ceiling".into(),
        targets: vec![PhysicalQuantity::new(5.0, Unit::Watt)],
    };
    let power_min = CandidateRequirement {
        statement: "Operating power shall be at least 8 W".into(),
        category: RequirementCategory::Electrical,
        priority: Priority::High,
        acceptance_criterion: "measured input power > 8 W".into(),
        source_hint: "intent: >= 8 W".into(),
        confidence: 0.9,
        rationale: "stated power floor".into(),
        targets: vec![PhysicalQuantity::new(8.0, Unit::Watt)],
    };
    Box::new(FixtureEngine::single(ReasoningResponse {
        candidates: vec![power_max, power_min],
        clarifying_questions: vec![],
        raw: "{}".into(),
    }))
}

/// A reasoning engine returning ONLY load requirements: electrical, with no power-entry
/// wording (none of "usb"/"power"/"connector"/"supply") and no physical targets. Schematic
/// Planning therefore classifies every block as an IC load — no power source exists — so the
/// power rail (VBUS) ends up with consumers but no driver and ERC flags it as undriven.
fn load_only_engine() -> Box<dyn ReasoningEngine> {
    let sensor = CandidateRequirement {
        statement: "Sensor sampling shall run at 100 Hz".into(),
        category: RequirementCategory::Electrical,
        priority: Priority::High,
        acceptance_criterion: "measured sample rate is 100 Hz".into(),
        source_hint: "intent: sampling".into(),
        confidence: 0.9,
        rationale: "stated sampling rate".into(),
        targets: vec![],
    };
    let logic = CandidateRequirement {
        statement: "Logic core shall operate at 3.3 logic level".into(),
        category: RequirementCategory::Electrical,
        priority: Priority::High,
        acceptance_criterion: "core I/O measured at 3.3 logic level".into(),
        source_hint: "intent: logic".into(),
        confidence: 0.9,
        rationale: "stated logic level".into(),
        targets: vec![],
    };
    Box::new(FixtureEngine::single(ReasoningResponse {
        candidates: vec![sensor, logic],
        clarifying_questions: vec![],
        raw: "{}".into(),
    }))
}

/// A reasoning engine returning a voltage-regulator requirement plus one plain load, both with
/// no physical targets (so no constraints, so the constraint gate is clean). Schematic Planning
/// recognizes the "voltage regulator" wording and realizes a regulator component — its VOUT pin
/// drives the rail, so ERC is clean and the workflow reaches the BOM layer. BOM Planning then
/// resolves that regulator to its catalog part, the deliberately end-of-life LM1117-3.3, so the
/// BOM lifecycle gate fails.
fn regulator_and_load_engine() -> Box<dyn ReasoningEngine> {
    let regulator = CandidateRequirement {
        statement: "Board shall include a 3.3 V voltage regulator".into(),
        category: RequirementCategory::Functional,
        priority: Priority::High,
        acceptance_criterion: "a regulated 3.3 V rail is present and within tolerance".into(),
        source_hint: "intent: 3.3 V rail".into(),
        confidence: 0.9,
        rationale: "the logic core needs a regulated 3.3 V supply".into(),
        targets: vec![],
    };
    let load = CandidateRequirement {
        statement: "Logic core shall operate at 3.3 logic level".into(),
        category: RequirementCategory::Electrical,
        priority: Priority::High,
        acceptance_criterion: "core I/O measured at 3.3 logic level".into(),
        source_hint: "intent: logic".into(),
        confidence: 0.9,
        rationale: "stated logic level".into(),
        targets: vec![],
    };
    Box::new(FixtureEngine::single(ReasoningResponse {
        candidates: vec![regulator, load],
        clarifying_questions: vec![],
        raw: "{}".into(),
    }))
}

/// A reasoning engine for the PCB oversize-board scenario: a USB-C power-entry connector
/// (Functional, recognized as a power source -> a 9 mm courtyard), an enclosure limit that
/// sizes the board (Mechanical, carrying the only length target), and one electrical load
/// (-> a 6 mm IC). The three blocks are realized into three components which Component Placement
/// lays out left-to-right at x = 2, 14, 26 mm (a 2 mm margin and a 12 mm pitch).
///
/// The enclosure target is 24 mm, not the very tightest conceivable outline, by deliberate
/// arithmetic: with a 12 mm placement pitch the third component sits at x = 26 mm and needs a
/// 32 mm-wide board to fit, so any square outline in (20, 32) mm leaves EXACTLY that one
/// component off the board while the first two fit — giving a single, unambiguous
/// `drc-out-of-bounds` violation to trace. (An 8 x 8 mm outline would instead push all three
/// components off the board and raise three separate violations — see
/// `drc_oversize_board_is_caught_routed_back_and_left_traceable`.)
fn oversize_board_engine() -> Box<dyn ReasoningEngine> {
    let usb_c = CandidateRequirement {
        statement: "Device shall be powered over USB-C".into(),
        category: RequirementCategory::Functional,
        priority: Priority::High,
        acceptance_criterion: "the device enumerates and draws power through a USB-C receptacle"
            .into(),
        source_hint: "intent: USB-C power entry".into(),
        confidence: 0.9,
        rationale: "USB-C is the stated power interface".into(),
        targets: vec![],
    };
    let enclosure = CandidateRequirement {
        statement: "Enclosure limits the board to 24 x 24 mm".into(),
        category: RequirementCategory::Mechanical,
        priority: Priority::High,
        acceptance_criterion: "the board outline fits within 24 x 24 mm".into(),
        source_hint: "intent: enclosure".into(),
        confidence: 0.9,
        rationale: "the enclosure caps the usable board area".into(),
        targets: vec![PhysicalQuantity::new(24.0, Unit::Millimetre)],
    };
    let load = CandidateRequirement {
        statement: "Logic core shall operate at a 3.3 V logic level".into(),
        category: RequirementCategory::Electrical,
        priority: Priority::High,
        acceptance_criterion: "core I/O is measured at a 3.3 V logic level".into(),
        source_hint: "intent: logic core".into(),
        confidence: 0.9,
        rationale: "the digital core needs a defined logic level".into(),
        targets: vec![],
    };
    Box::new(FixtureEngine::single(ReasoningResponse {
        candidates: vec![usb_c, enclosure, load],
        clarifying_questions: vec![],
        raw: "{}".into(),
    }))
}

/// A reasoning engine for the routing trace-width scenario: a USB-C power-entry connector (a
/// power source, so the rail is driven and ERC is clean) and one electrical load, both with no
/// targets, plus a Fabrication process requirement whose 0.75 mm length target is the minimum
/// manufacturable trace width. With no Mechanical target the board defaults to a roomy
/// 100 mm square, so the placement geometry is clean; but Routing Planning routes every net at its
/// per-class default (0.50 mm for the power/ground rails) — finer than the 0.75 mm process floor —
/// so DRC's trace-width rule flags each routed track. It is the one fault produced by the routing
/// layer rather than placement.
fn trace_floor_engine() -> Box<dyn ReasoningEngine> {
    let usb_c = CandidateRequirement {
        statement: "Device shall be powered over USB-C".into(),
        category: RequirementCategory::Functional,
        priority: Priority::High,
        acceptance_criterion: "the device draws power through a USB-C receptacle".into(),
        source_hint: "intent: USB-C power entry".into(),
        confidence: 0.9,
        rationale: "USB-C is the stated power interface".into(),
        targets: vec![],
    };
    let load = CandidateRequirement {
        statement: "Logic core shall operate at a 3.3 V logic level".into(),
        category: RequirementCategory::Electrical,
        priority: Priority::High,
        acceptance_criterion: "core I/O is measured at a 3.3 V logic level".into(),
        source_hint: "intent: logic core".into(),
        confidence: 0.9,
        rationale: "the digital core needs a defined logic level".into(),
        targets: vec![],
    };
    let process = CandidateRequirement {
        statement: "Fabrication process supports a 0.75 mm minimum trace width".into(),
        category: RequirementCategory::Fabrication,
        priority: Priority::High,
        acceptance_criterion: "every trace is at least 0.75 mm wide".into(),
        source_hint: "intent: fab process class".into(),
        confidence: 0.9,
        rationale: "the chosen fab process caps how fine a trace can be etched".into(),
        targets: vec![PhysicalQuantity::new(0.75, Unit::Millimetre)],
    };
    Box::new(FixtureEngine::single(ReasoningResponse {
        candidates: vec![usb_c, load, process],
        clarifying_questions: vec![],
        raw: "{}".into(),
    }))
}

/// A reasoning engine for the DFM edge-clearance scenario: a USB-C power-entry connector, an
/// enclosure that sizes the board to a 32.3 mm square, and one electrical load. The three
/// components lay out at x = 2, 14, 26 mm; the trailing courtyard ends at x = 32 mm, so it FITS
/// the 32.3 mm outline (DRC out-of-bounds and trace-width are clean) but sits only 0.3 mm from
/// the right edge — inside the 0.5 mm DFM board-edge keep-out. It is the one fault produced at
/// the manufacturability gate rather than DRC.
fn tight_edge_board_engine() -> Box<dyn ReasoningEngine> {
    let usb_c = CandidateRequirement {
        statement: "Device shall be powered over USB-C".into(),
        category: RequirementCategory::Functional,
        priority: Priority::High,
        acceptance_criterion: "the device draws power through a USB-C receptacle".into(),
        source_hint: "intent: USB-C power entry".into(),
        confidence: 0.9,
        rationale: "USB-C is the stated power interface".into(),
        targets: vec![],
    };
    let enclosure = CandidateRequirement {
        statement: "Enclosure limits the board to 32.3 x 32.3 mm".into(),
        category: RequirementCategory::Mechanical,
        priority: Priority::High,
        acceptance_criterion: "the board outline fits within 32.3 x 32.3 mm".into(),
        source_hint: "intent: enclosure".into(),
        confidence: 0.9,
        rationale: "the enclosure caps the usable board area".into(),
        targets: vec![PhysicalQuantity::new(32.3, Unit::Millimetre)],
    };
    let load = CandidateRequirement {
        statement: "Logic core shall operate at a 3.3 V logic level".into(),
        category: RequirementCategory::Electrical,
        priority: Priority::High,
        acceptance_criterion: "core I/O is measured at a 3.3 V logic level".into(),
        source_hint: "intent: logic core".into(),
        confidence: 0.9,
        rationale: "the digital core needs a defined logic level".into(),
        targets: vec![],
    };
    Box::new(FixtureEngine::single(ReasoningResponse {
        candidates: vec![usb_c, enclosure, load],
        clarifying_questions: vec![],
        raw: "{}".into(),
    }))
}

/// A reasoning engine for the EMC antenna-length scenario: a USB-C power-entry connector (a power
/// source, so the rail is driven and ERC is clean), one electrical load, and a high-speed serial
/// interface carrying a 10 GHz frequency target. With no Mechanical target the board defaults to a
/// roomy 100 mm square, so the placement geometry, DRC, and DFM are all clean; but Routing Planning
/// realizes every net as a centroid-to-centroid track tens of millimetres long — far longer than
/// the electrically-long limit (lambda/10 = 3 mm at 10 GHz) — so EMC Analysis flags each routed
/// track as a radiator. It is the one fault produced at the EMC gate rather than at any earlier
/// gate, and it loops back to Routing Planning.
fn high_speed_engine() -> Box<dyn ReasoningEngine> {
    let usb_c = CandidateRequirement {
        statement: "Device shall be powered over USB-C".into(),
        category: RequirementCategory::Functional,
        priority: Priority::High,
        acceptance_criterion: "the device draws power through a USB-C receptacle".into(),
        source_hint: "intent: USB-C power entry".into(),
        confidence: 0.9,
        rationale: "USB-C is the stated power interface".into(),
        targets: vec![],
    };
    let load = CandidateRequirement {
        statement: "Logic core shall operate at a 3.3 V logic level".into(),
        category: RequirementCategory::Electrical,
        priority: Priority::High,
        acceptance_criterion: "core I/O is measured at a 3.3 V logic level".into(),
        source_hint: "intent: logic core".into(),
        confidence: 0.9,
        rationale: "the digital core needs a defined logic level".into(),
        targets: vec![],
    };
    let high_speed = CandidateRequirement {
        statement: "High-speed serial link shall operate at 10 GHz".into(),
        category: RequirementCategory::Electrical,
        priority: Priority::High,
        acceptance_criterion: "radiated emissions are assessed at the 10 GHz line rate".into(),
        source_hint: "intent: high-speed link".into(),
        confidence: 0.9,
        rationale: "the multi-gigabit serial line sets the emission spectrum".into(),
        targets: vec![PhysicalQuantity::new(10_000.0, Unit::Megahertz)],
    };
    Box::new(FixtureEngine::single(ReasoningResponse {
        candidates: vec![usb_c, load, high_speed],
        clarifying_questions: vec![],
        raw: "{}".into(),
    }))
}

#[test]
fn run_replays_byte_identical_and_runs_fifteen_phases() {
    let (config, log) = cfg("det", 1);
    let report = run(&config).expect("run succeeds");

    // Full Phase-3 workflow on a consistent, realizable design: RP -> Engineering Analysis ->
    // Constraint Extraction -> Constraint Verification -> Schematic Planning -> ERC -> BOM
    // Planning -> BOM Verification -> PCB Floor Planning -> Component Placement -> Routing
    // Planning -> DRC -> DFM -> EMC -> Manufacturing Generation, all OK.
    assert_eq!(report.outcomes.len(), 15);
    assert!(report
        .outcomes
        .iter()
        .all(|(_, o)| matches!(o, PhaseOutcome::Success)));
    // The terminal phase is Manufacturing Generation, and it RELEASED: the global gate found no
    // open blocking violation, so the design lowered to a Manufacturing IR.
    assert_eq!(report.outcomes.last().unwrap().0, "ManufacturingGeneration");

    // Three requirements; two carry targets, so two constraints; no violations.
    assert_eq!(report.state.requirements.len(), 3);
    assert_eq!(report.state.constraints.len(), 2);
    assert!(report.state.violations.is_empty());

    // The synthesis layer was realized: blocks, components, and at least one net exist.
    assert!(!report.state.functional_blocks.is_empty());
    assert!(!report.state.components.is_empty());
    assert!(!report.state.nets.is_empty());

    // The BOM layer landed: every component was sourced to a concrete part through a line item.
    assert!(!report.state.parts.is_empty());
    assert!(!report.state.bom_line_items.is_empty());

    // The PCB layer landed: a board outline exists and every component was placed on it.
    assert!(report.state.board.is_some());
    assert!(!report.state.placements.is_empty());

    // The routing layer landed: every net was realized by exactly one track.
    assert!(!report.state.tracks.is_empty());
    assert_eq!(report.state.tracks.len(), report.state.nets.len());

    // EXIT CRITERION (Phase 1, preserved): history replays to identical state, byte for byte.
    let replayed = replay_cmd(&log).expect("replay succeeds");
    assert_eq!(report.state, replayed);
    assert_eq!(report.state.canonical_json(), replayed.canonical_json());

    let _ = std::fs::remove_file(&log);
}

#[test]
fn infeasible_constraints_are_caught_routed_back_and_left_traceable() {
    let (config, log) = cfg("infeasible", 3);
    let report = run_with(contradictory_engine(), &config).expect("run completes with a failure");

    // The constraint correctness loop fired: verification failed and was routed back to
    // extraction, bounded to 1 initial + 2 retries = 3 verification runs.
    let verify_runs = report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "ConstraintVerification")
        .count();
    assert_eq!(verify_runs, 3);
    assert!(report
        .outcomes
        .iter()
        .any(|(n, o)| n == "ConstraintVerification" && matches!(o, PhaseOutcome::Failed(_))));

    // The workflow never progressed past the failed constraint gate — neither the schematic
    // synthesis, ERC, nor the BOM layer ran. (Engineering Analysis sits *before* the gate, so
    // it did run.)
    assert!(!report
        .outcomes
        .iter()
        .any(|(n, _)| n == "SchematicPlanning"));
    assert!(!report.outcomes.iter().any(|(n, _)| n == "ErcVerification"));
    assert!(!report.outcomes.iter().any(|(n, _)| n == "BomPlanning"));
    assert!(!report.outcomes.iter().any(|(n, _)| n == "BomVerification"));
    assert!(report.state.components.is_empty());
    assert!(report.state.parts.is_empty());
    assert!(report.state.bom_line_items.is_empty());

    // ... and certainly none of the PCB layer ran: no floor plan, no placement, no DRC.
    assert!(!report.outcomes.iter().any(|(n, _)| n == "PcbFloorPlanning"));
    assert!(!report
        .outcomes
        .iter()
        .any(|(n, _)| n == "ComponentPlacement"));
    assert!(!report.outcomes.iter().any(|(n, _)| n == "DrcVerification"));
    assert!(report.state.board.is_none());
    assert!(report.state.placements.is_empty());

    // Exactly one violation (re-verification did not duplicate it), still OPEN and blocking.
    assert_eq!(report.state.violations.len(), 1);
    let v = &report.state.violations[0];
    assert_eq!(v.status, ViolationStatus::Open);
    assert_eq!(v.severity, ViolationSeverity::Error);
    assert!(v.is_blocking());

    // FULLY TRACEABLE: violation -> constraints -> requirements -> design intent, with a
    // provenance link backing every hop.
    let intent = report.state.intent.as_ref().expect("intent captured");
    assert!(!v.subjects.is_empty());
    for cid in &v.subjects {
        let c = report
            .state
            .constraints
            .iter()
            .find(|c| c.id == *cid)
            .expect("violation subject is a known constraint");
        let req = report
            .state
            .requirements
            .iter()
            .find(|r| r.id == c.subject_requirement)
            .expect("constraint is rooted in a requirement");
        assert_eq!(req.source, intent.id);

        assert!(report
            .state
            .links
            .iter()
            .any(|l| l.from == v.id && l.to == *cid && l.relation == Relation::TracesTo));
        assert!(report.state.links.iter().any(|l| l.from == c.id
            && l.to == c.subject_requirement
            && l.relation == Relation::DerivedFrom));
        assert!(report
            .state
            .links
            .iter()
            .any(|l| l.from == req.id && l.to == intent.id && l.relation == Relation::DerivedFrom));
    }

    // Replay identity holds even for a failed, looped-back run.
    let replayed = replay_cmd(&log).expect("replay succeeds");
    assert_eq!(report.state, replayed);

    let _ = std::fs::remove_file(&log);
}

#[test]
fn undriven_power_net_is_caught_routed_back_and_left_traceable() {
    let (config, log) = cfg("erc", 5);
    let report = run_with(load_only_engine(), &config).expect("run completes with a failure");

    // The schematic correctness loop fired: ERC failed and was routed back to Schematic
    // Planning, bounded to 1 initial + 2 retries = 3 ERC runs, every one failing.
    let erc_runs: Vec<&PhaseOutcome> = report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "ErcVerification")
        .map(|(_, o)| o)
        .collect();
    assert_eq!(erc_runs.len(), 3);
    assert!(erc_runs
        .iter()
        .all(|o| matches!(o, PhaseOutcome::Failed(_))));

    // The constraint phases ran clean upstream (these loads carry no targets, so no
    // constraints, so nothing to contradict).
    assert!(report.state.constraints.is_empty());

    // The workflow never progressed past the failed ERC gate — neither the BOM layer nor any
    // PCB phase ran, so no parts were sourced and no board was placed.
    assert!(!report.outcomes.iter().any(|(n, _)| n == "BomPlanning"));
    assert!(!report.outcomes.iter().any(|(n, _)| n == "BomVerification"));
    assert!(!report.outcomes.iter().any(|(n, _)| n == "PcbFloorPlanning"));
    assert!(!report
        .outcomes
        .iter()
        .any(|(n, _)| n == "ComponentPlacement"));
    assert!(!report.outcomes.iter().any(|(n, _)| n == "DrcVerification"));
    assert!(report.state.parts.is_empty());
    assert!(report.state.bom_line_items.is_empty());
    assert!(report.state.board.is_none());
    assert!(report.state.placements.is_empty());

    // The workflow did not reach a clean end: its final phase is the failed ERC gate.
    let (last_name, last_outcome) = report.outcomes.last().expect("at least one phase ran");
    assert_eq!(last_name, "ErcVerification");
    assert!(matches!(last_outcome, PhaseOutcome::Failed(_)));

    // Exactly one violation (loop-back re-verification did not duplicate it), OPEN and
    // blocking — an undriven power net is an error.
    assert_eq!(report.state.violations.len(), 1);
    let v = &report.state.violations[0];
    assert_eq!(v.status, ViolationStatus::Open);
    assert_eq!(v.severity, ViolationSeverity::Error);
    assert!(v.is_blocking());

    // FULLY TRACEABLE down the realization layer: the violation names a net; walk
    // net -> member pins -> component -> functional block -> requirement -> design intent,
    // checking the backing provenance links at each synthesized hop.
    let intent = report.state.intent.as_ref().expect("intent captured");
    assert_eq!(v.subjects.len(), 1, "one undriven net implicated");
    for subject in &v.subjects {
        // Violation -> Net (TracesTo), raised by ERC.
        let net = report
            .state
            .net(*subject)
            .expect("violation subject is a known net");
        assert!(report
            .state
            .links
            .iter()
            .any(|l| l.from == v.id && l.to == net.id && l.relation == Relation::TracesTo));

        // The net is non-trivial: it joins the loads' power pins.
        assert!(!net.members.is_empty(), "the undriven net has member pins");
        for pin_id in &net.members {
            let pin = report
                .state
                .pin(*pin_id)
                .expect("net member is a known pin");
            let component = report
                .state
                .component(pin.component)
                .expect("pin belongs to a known component");
            let block = report
                .state
                .functional_block(component.from_block)
                .expect("component was realized from a known block");

            // Component -> Block (DerivedFrom), recorded by Schematic Planning.
            assert!(report.state.links.iter().any(|l| l.from == component.id
                && l.to == block.id
                && l.relation == Relation::DerivedFrom));

            // Block -> Requirement (DerivedFrom), recorded by Engineering Analysis, and the
            // requirement is rooted in the captured design intent.
            assert!(
                !block.requirements.is_empty(),
                "block realizes a requirement"
            );
            for req_id in &block.requirements {
                let req = report
                    .state
                    .requirement(*req_id)
                    .expect("block realizes a known requirement");
                assert_eq!(req.source, intent.id);
                assert!(report.state.links.iter().any(|l| l.from == block.id
                    && l.to == req.id
                    && l.relation == Relation::DerivedFrom));
            }
        }
    }

    // Replay identity holds even for a failed, looped-back run.
    let replayed = replay_cmd(&log).expect("replay succeeds");
    assert_eq!(report.state, replayed);
    assert_eq!(report.state.canonical_json(), replayed.canonical_json());

    let _ = std::fs::remove_file(&log);
}

#[test]
fn bom_eol_part_is_caught_routed_back_and_left_traceable() {
    let (config, log) = cfg("bom", 9);
    let report =
        run_with(regulator_and_load_engine(), &config).expect("run completes with a failure");

    // The BOM correctness loop fired: BOM verification failed and was routed back to BOM
    // Planning, bounded to 1 initial + 2 retries = 3 BOM-verification runs, every one failing.
    let bom_runs: Vec<&PhaseOutcome> = report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "BomVerification")
        .map(|(_, o)| o)
        .collect();
    assert_eq!(bom_runs.len(), 3);
    assert!(bom_runs
        .iter()
        .all(|o| matches!(o, PhaseOutcome::Failed(_))));

    // The schematic upstream was clean: the regulator drives the rail, so ERC passed and the
    // workflow reached the BOM layer — parts were sourced and line items minted.
    assert!(!report.state.parts.is_empty());
    assert!(!report.state.bom_line_items.is_empty());

    // The workflow never progressed past the failed BOM gate — none of the PCB phases ran, so
    // no board was floor-planned and nothing was placed.
    assert!(!report.outcomes.iter().any(|(n, _)| n == "PcbFloorPlanning"));
    assert!(!report
        .outcomes
        .iter()
        .any(|(n, _)| n == "ComponentPlacement"));
    assert!(!report.outcomes.iter().any(|(n, _)| n == "DrcVerification"));
    assert!(report.state.board.is_none());
    assert!(report.state.placements.is_empty());

    // Exactly one violation (loop-back re-verification did not duplicate it): an end-of-life
    // part, OPEN + Error + blocking, raised by the BOM lifecycle rule.
    assert_eq!(report.state.violations.len(), 1);
    let v = &report.state.violations[0];
    assert_eq!(v.status, ViolationStatus::Open);
    assert_eq!(v.severity, ViolationSeverity::Error);
    assert!(v.is_blocking());
    assert_eq!(v.rule, "bom-lifecycle");

    // FULLY TRACEABLE across the BOM layer: the violation names a BOM line item; resolve it to
    // its end-of-life part AND to the components it covers, then walk each component back
    // through its functional block to the requirement it realizes and on to the design intent.
    let intent = report.state.intent.as_ref().expect("intent captured");
    assert_eq!(v.subjects.len(), 1, "one end-of-life line item implicated");
    for subject in &v.subjects {
        let item = report
            .state
            .bom_line_item(*subject)
            .expect("violation subject is a known BOM line item");

        // The line orders the deliberately end-of-life catalog part (LM1117-3.3).
        let part = report
            .state
            .part(item.part)
            .expect("line item orders a known part");
        assert_eq!(part.lifecycle, PartLifecycle::Eol);

        // ... and binds it to real components, each traceable back to the captured intent.
        assert!(!item.components.is_empty(), "the line covers components");
        for comp_id in &item.components {
            let component = report
                .state
                .component(*comp_id)
                .expect("line covers a known component");
            let block = report
                .state
                .functional_block(component.from_block)
                .expect("component was realized from a known block");
            assert!(
                !block.requirements.is_empty(),
                "block realizes a requirement"
            );
            for req_id in &block.requirements {
                let req = report
                    .state
                    .requirement(*req_id)
                    .expect("block realizes a known requirement");
                assert_eq!(req.source, intent.id);
            }
        }
    }

    // Replay identity holds even for a failed, looped-back BOM run.
    let replayed = replay_cmd(&log).expect("replay succeeds");
    assert_eq!(report.state, replayed);
    assert_eq!(report.state.canonical_json(), replayed.canonical_json());

    let _ = std::fs::remove_file(&log);
}

#[test]
fn drc_oversize_board_is_caught_routed_back_and_left_traceable() {
    let (config, log) = cfg("drc", 11);
    let report = run_with(oversize_board_engine(), &config).expect("run completes with a failure");

    // The PCB correctness loop fired: DRC failed and was routed back to Routing Planning (the
    // canonical loop-back target for clearance/geometry defects), bounded to 1 initial + 2
    // retries = 3 DRC runs, every one failing.
    let drc_runs: Vec<&PhaseOutcome> = report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "DrcVerification")
        .map(|(_, o)| o)
        .collect();
    assert_eq!(drc_runs.len(), 3);
    assert!(drc_runs
        .iter()
        .all(|o| matches!(o, PhaseOutcome::Failed(_))));

    // The realization, BOM, and floor-planning layers ran clean upstream, so the substrate the
    // DRC is checked against exists: a board outline and one placement per component.
    assert!(report.state.board.is_some());
    assert_eq!(report.state.placements.len(), 3);

    // The workflow did not reach a clean end: its final phase is the failed DRC gate.
    let (last_name, last_outcome) = report.outcomes.last().expect("at least one phase ran");
    assert_eq!(last_name, "DrcVerification");
    assert!(matches!(last_outcome, PhaseOutcome::Failed(_)));

    // Exactly one OPEN, blocking, error-severity violation from the out-of-bounds rule
    // (loop-back re-verification never double-raised it): the lone off-board courtyard. With
    // the first two courtyards inside the 24 mm outline and the third (at x = 26 mm) off it,
    // this is the single violation the scenario is built to produce.
    let out_of_bounds: Vec<_> = report
        .state
        .violations
        .iter()
        .filter(|v| v.rule == "drc-out-of-bounds")
        .collect();
    assert_eq!(out_of_bounds.len(), 1);
    let v = out_of_bounds[0];
    assert_eq!(v.status, ViolationStatus::Open);
    assert_eq!(v.severity, ViolationSeverity::Error);
    assert!(v.is_blocking());
    assert_eq!(
        report.state.open_blocking_violations().len(),
        1,
        "the off-board courtyard is the only blocking violation"
    );

    // FULLY TRACEABLE across the PCB layer: the violation names a placement; walk
    // placement -> component -> functional block -> requirement -> design intent, checking the
    // backing provenance link at each synthesized hop.
    let intent = report.state.intent.as_ref().expect("intent captured");
    assert_eq!(v.subjects.len(), 1, "one off-board placement implicated");
    let pid = v.subjects[0];

    // Violation -> Placement (TracesTo), raised by DRC.
    assert!(report
        .state
        .links
        .iter()
        .any(|l| l.from == v.id && l.to == pid && l.relation == Relation::TracesTo));

    let placement = report
        .state
        .placement(pid)
        .expect("violation subject is a known placement");
    let component = report
        .state
        .component(placement.component)
        .expect("placement positions a known component");

    // Placement -> Component (TracesTo), recorded by Component Placement.
    assert!(report
        .state
        .links
        .iter()
        .any(|l| l.from == pid && l.to == component.id && l.relation == Relation::TracesTo));

    let block = report
        .state
        .functional_block(component.from_block)
        .expect("component was realized from a known block");

    // Component -> Block (DerivedFrom), recorded by Schematic Planning.
    assert!(report.state.links.iter().any(|l| l.from == component.id
        && l.to == block.id
        && l.relation == Relation::DerivedFrom));

    // Block -> Requirement (DerivedFrom), recorded by Engineering Analysis, and the requirement
    // is rooted in the captured design intent.
    assert!(
        !block.requirements.is_empty(),
        "block realizes a requirement"
    );
    for req_id in &block.requirements {
        let req = report
            .state
            .requirement(*req_id)
            .expect("block realizes a known requirement");
        assert_eq!(req.source, intent.id);
        assert!(report
            .state
            .links
            .iter()
            .any(|l| l.from == block.id && l.to == req.id && l.relation == Relation::DerivedFrom));
    }

    // Replay identity holds even for a failed, looped-back DRC run.
    let replayed = replay_cmd(&log).expect("replay succeeds");
    assert_eq!(report.state, replayed);
    assert_eq!(report.state.canonical_json(), replayed.canonical_json());

    let _ = std::fs::remove_file(&log);
}

#[test]
fn routing_trace_too_fine_is_caught_routed_back_and_left_traceable() {
    let (config, log) = cfg("trace-width", 13);
    let report = run_with(trace_floor_engine(), &config).expect("run completes with a failure");

    // The PCB correctness loop fired: DRC failed and was routed back to Routing Planning (the
    // canonical loop-back target for routing defects), bounded to 1 initial + 2 retries = 3 DRC
    // runs, every one failing.
    let drc_runs: Vec<&PhaseOutcome> = report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "DrcVerification")
        .map(|(_, o)| o)
        .collect();
    assert_eq!(drc_runs.len(), 3);
    assert!(drc_runs
        .iter()
        .all(|o| matches!(o, PhaseOutcome::Failed(_))));

    // Routing Planning re-ran on each loop-back (idempotent): 1 initial + 2 re-entries.
    let routing_runs = report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "RoutingPlanning")
        .count();
    assert_eq!(routing_runs, 3);

    // The board was placed and routed clean upstream — the substrate DRC is checked against
    // exists: an outline and one track per net.
    assert!(report.state.board.is_some());
    assert!(!report.state.tracks.is_empty());
    assert_eq!(report.state.tracks.len(), report.state.nets.len());

    // The workflow did not reach a clean end: its final phase is the failed DRC gate.
    let (last_name, last_outcome) = report.outcomes.last().expect("at least one phase ran");
    assert_eq!(last_name, "DrcVerification");
    assert!(matches!(last_outcome, PhaseOutcome::Failed(_)));

    // The only blocking violations are trace-width findings — one per routed track — each OPEN,
    // Error-severity, and never double-raised across the loop-back re-verifications.
    let trace_width: Vec<_> = report
        .state
        .violations
        .iter()
        .filter(|v| v.rule == "drc-trace-width")
        .collect();
    assert!(!trace_width.is_empty());
    assert_eq!(trace_width.len(), report.state.tracks.len());
    assert_eq!(
        report.state.open_blocking_violations().len(),
        trace_width.len(),
        "the fine traces are the only blocking violations"
    );
    for v in &trace_width {
        assert_eq!(v.status, ViolationStatus::Open);
        assert_eq!(v.severity, ViolationSeverity::Error);
        assert!(v.is_blocking());
    }

    // FULLY TRACEABLE across the routing layer: the violation names a track; walk
    // track -> net -> member pins -> component -> functional block -> requirement -> design
    // intent, checking the backing provenance link at each synthesized hop.
    let intent = report.state.intent.as_ref().expect("intent captured");
    let v = trace_width[0];
    assert_eq!(v.subjects.len(), 1, "one fine trace implicated");
    let tid = v.subjects[0];

    // Violation -> Track (TracesTo), raised by DRC.
    assert!(report
        .state
        .links
        .iter()
        .any(|l| l.from == v.id && l.to == tid && l.relation == Relation::TracesTo));

    let track = report
        .state
        .track(tid)
        .expect("violation subject is a known track");

    // Track -> Net (TracesTo), recorded by Routing Planning.
    assert!(report
        .state
        .links
        .iter()
        .any(|l| l.from == tid && l.to == track.net && l.relation == Relation::TracesTo));

    let net = report
        .state
        .net(track.net)
        .expect("track realizes a known net");
    assert!(!net.members.is_empty(), "the routed net joins pins");
    for pin_id in &net.members {
        let pin = report
            .state
            .pin(*pin_id)
            .expect("net member is a known pin");
        let component = report
            .state
            .component(pin.component)
            .expect("pin belongs to a known component");
        let block = report
            .state
            .functional_block(component.from_block)
            .expect("component was realized from a known block");
        assert!(
            !block.requirements.is_empty(),
            "block realizes a requirement"
        );
        for req_id in &block.requirements {
            let req = report
                .state
                .requirement(*req_id)
                .expect("block realizes a known requirement");
            assert_eq!(req.source, intent.id);
        }
    }

    // Replay identity holds even for a failed, looped-back routing run.
    let replayed = replay_cmd(&log).expect("replay succeeds");
    assert_eq!(report.state, replayed);
    assert_eq!(report.state.canonical_json(), replayed.canonical_json());

    let _ = std::fs::remove_file(&log);
}

#[test]
fn dfm_edge_too_close_is_caught_routed_back_and_left_traceable() {
    let (config, log) = cfg("dfm", 15);
    let report =
        run_with(tight_edge_board_engine(), &config).expect("run completes with a failure");

    // The DFM correctness loop fired: DFM failed and was routed back to Component Placement (the
    // canonical target — manufacturability defects are usually placement-driven), bounded to
    // 1 initial + 2 retries = 3 DFM runs, every one failing.
    let dfm_runs: Vec<&PhaseOutcome> = report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "DfmVerification")
        .map(|(_, o)| o)
        .collect();
    assert_eq!(dfm_runs.len(), 3);
    assert!(dfm_runs
        .iter()
        .all(|o| matches!(o, PhaseOutcome::Failed(_))));

    // Component Placement re-ran on each loop-back (idempotent): 1 initial + 2 re-entries.
    let placement_runs = report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "ComponentPlacement")
        .count();
    assert_eq!(placement_runs, 3);

    // DRC ran clean upstream every time — the layout fits and the traces are clear; DFM is the
    // only gate that fails.
    assert!(report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "DrcVerification")
        .all(|(_, o)| matches!(o, PhaseOutcome::Success)));
    assert!(report.state.board.is_some());
    assert_eq!(report.state.placements.len(), 3);

    // The workflow did not reach a clean end: its final phase is the failed DFM gate.
    let (last_name, last_outcome) = report.outcomes.last().expect("at least one phase ran");
    assert_eq!(last_name, "DfmVerification");
    assert!(matches!(last_outcome, PhaseOutcome::Failed(_)));

    // Exactly one OPEN, blocking, error-severity violation from the edge-clearance rule
    // (loop-back re-verification never double-raised it): the lone edge-hugging courtyard.
    let edge: Vec<_> = report
        .state
        .violations
        .iter()
        .filter(|v| v.rule == "dfm-edge-clearance")
        .collect();
    assert_eq!(edge.len(), 1);
    let v = edge[0];
    assert_eq!(v.status, ViolationStatus::Open);
    assert_eq!(v.severity, ViolationSeverity::Error);
    assert!(v.is_blocking());
    assert_eq!(
        report.state.open_blocking_violations().len(),
        1,
        "the edge-hugging courtyard is the only blocking violation"
    );

    // FULLY TRACEABLE across the PCB layer: the violation names a placement; walk
    // placement -> component -> functional block -> requirement -> design intent, checking the
    // backing provenance link at each synthesized hop.
    let intent = report.state.intent.as_ref().expect("intent captured");
    assert_eq!(v.subjects.len(), 1, "one edge-hugging placement implicated");
    let pid = v.subjects[0];

    // Violation -> Placement (TracesTo), raised by DFM.
    assert!(report
        .state
        .links
        .iter()
        .any(|l| l.from == v.id && l.to == pid && l.relation == Relation::TracesTo));

    let placement = report
        .state
        .placement(pid)
        .expect("violation subject is a known placement");
    let component = report
        .state
        .component(placement.component)
        .expect("placement positions a known component");

    // Placement -> Component (TracesTo), recorded by Component Placement.
    assert!(report
        .state
        .links
        .iter()
        .any(|l| l.from == pid && l.to == component.id && l.relation == Relation::TracesTo));

    let block = report
        .state
        .functional_block(component.from_block)
        .expect("component was realized from a known block");
    assert!(
        !block.requirements.is_empty(),
        "block realizes a requirement"
    );
    for req_id in &block.requirements {
        let req = report
            .state
            .requirement(*req_id)
            .expect("block realizes a known requirement");
        assert_eq!(req.source, intent.id);
        assert!(report
            .state
            .links
            .iter()
            .any(|l| l.from == block.id && l.to == req.id && l.relation == Relation::DerivedFrom));
    }

    // Replay identity holds even for a failed, looped-back DFM run.
    let replayed = replay_cmd(&log).expect("replay succeeds");
    assert_eq!(report.state, replayed);
    assert_eq!(report.state.canonical_json(), replayed.canonical_json());

    let _ = std::fs::remove_file(&log);
}

#[test]
fn emc_antenna_trace_is_caught_routed_back_and_left_traceable() {
    let (config, log) = cfg("emc", 17);
    let report = run_with(high_speed_engine(), &config).expect("run completes with a failure");

    // The EMC correctness loop fired: EMC Analysis failed and was routed back to Routing Planning
    // (the canonical target — emissions/coupling are routing-dominated), bounded to 1 initial + 2
    // retries = 3 EMC runs, every one failing.
    let emc_runs: Vec<&PhaseOutcome> = report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "EmcAnalysis")
        .map(|(_, o)| o)
        .collect();
    assert_eq!(emc_runs.len(), 3);
    assert!(emc_runs
        .iter()
        .all(|o| matches!(o, PhaseOutcome::Failed(_))));

    // Routing Planning re-ran on each loop-back (idempotent): 1 initial + 2 re-entries.
    let routing_runs = report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "RoutingPlanning")
        .count();
    assert_eq!(routing_runs, 3);

    // DRC and DFM ran clean upstream on every pass — the layout fits, the traces are wide enough
    // and clear of the edge. Per-phase gating means the open EMC violations never fail DRC or DFM
    // when they re-run on the loop-back; EMC is the only gate that fails.
    assert!(report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "DrcVerification")
        .all(|(_, o)| matches!(o, PhaseOutcome::Success)));
    assert!(report
        .outcomes
        .iter()
        .filter(|(n, _)| n == "DfmVerification")
        .all(|(_, o)| matches!(o, PhaseOutcome::Success)));

    // The board was placed and routed clean upstream — the substrate EMC is analyzed against
    // exists: an outline and one track per net.
    assert!(report.state.board.is_some());
    assert!(!report.state.tracks.is_empty());
    assert_eq!(report.state.tracks.len(), report.state.nets.len());

    // The workflow did not reach a clean end: its final phase is the failed EMC gate.
    let (last_name, last_outcome) = report.outcomes.last().expect("at least one phase ran");
    assert_eq!(last_name, "EmcAnalysis");
    assert!(matches!(last_outcome, PhaseOutcome::Failed(_)));

    // The only blocking violations are antenna-length findings — one per routed track — each OPEN,
    // Error-severity, and never double-raised across the loop-back re-verifications.
    let antenna: Vec<_> = report
        .state
        .violations
        .iter()
        .filter(|v| v.rule == "emc-antenna-length")
        .collect();
    assert!(!antenna.is_empty());
    assert_eq!(antenna.len(), report.state.tracks.len());
    assert_eq!(
        report.state.open_blocking_violations().len(),
        antenna.len(),
        "the electrically-long traces are the only blocking violations"
    );
    for v in &antenna {
        assert_eq!(v.status, ViolationStatus::Open);
        assert_eq!(v.severity, ViolationSeverity::Error);
        assert!(v.is_blocking());
    }

    // FULLY TRACEABLE across the routing layer: the violation names a track; walk
    // track -> net -> member pins -> component -> functional block -> requirement -> design intent,
    // checking the backing provenance link at each synthesized hop.
    let intent = report.state.intent.as_ref().expect("intent captured");
    let v = antenna[0];
    assert_eq!(
        v.subjects.len(),
        1,
        "one electrically-long trace implicated"
    );
    let tid = v.subjects[0];

    // Violation -> Track (TracesTo), raised by EMC Analysis.
    assert!(report
        .state
        .links
        .iter()
        .any(|l| l.from == v.id && l.to == tid && l.relation == Relation::TracesTo));

    let track = report
        .state
        .track(tid)
        .expect("violation subject is a known track");

    // Track -> Net (TracesTo), recorded by Routing Planning.
    assert!(report
        .state
        .links
        .iter()
        .any(|l| l.from == tid && l.to == track.net && l.relation == Relation::TracesTo));

    let net = report
        .state
        .net(track.net)
        .expect("track realizes a known net");
    assert!(!net.members.is_empty(), "the routed net joins pins");
    for pin_id in &net.members {
        let pin = report
            .state
            .pin(*pin_id)
            .expect("net member is a known pin");
        let component = report
            .state
            .component(pin.component)
            .expect("pin belongs to a known component");
        let block = report
            .state
            .functional_block(component.from_block)
            .expect("component was realized from a known block");
        assert!(
            !block.requirements.is_empty(),
            "block realizes a requirement"
        );
        for req_id in &block.requirements {
            let req = report
                .state
                .requirement(*req_id)
                .expect("block realizes a known requirement");
            assert_eq!(req.source, intent.id);
        }
    }

    // Replay identity holds even for a failed, looped-back EMC run.
    let replayed = replay_cmd(&log).expect("replay succeeds");
    assert_eq!(report.state, replayed);
    assert_eq!(report.state.canonical_json(), replayed.canonical_json());

    let _ = std::fs::remove_file(&log);
}

#[test]
fn every_requirement_is_fully_traceable() {
    let (config, log) = cfg("trace", 2);
    let report = run(&config).expect("run succeeds");
    let state = &report.state;
    let intent = state.intent.as_ref().expect("intent captured");

    for r in &state.requirements {
        // rooted in the design intent (source + a DerivedFrom link).
        assert_eq!(r.source, intent.id);
        assert!(state
            .links
            .iter()
            .any(|l| l.from == r.id && l.to == intent.id && l.relation == Relation::DerivedFrom));
        // justified by a decision.
        assert!(state
            .links
            .iter()
            .any(|l| l.from == r.id && l.relation == Relation::JustifiedBy));
        // accepted requirements are testable (domain invariant).
        assert!(r.validate().is_ok());

        // the rendered trace reaches the design intent.
        let chain = trace_cmd(&log, &r.id.short()).expect("trace renders");
        assert!(chain.contains("Design Intent"));
        assert!(chain.contains("Decision"));
    }

    let _ = std::fs::remove_file(&log);
}

#[test]
fn two_runs_with_same_seed_are_identical() {
    let (c1, l1) = cfg("rep1", 7);
    let (c2, l2) = cfg("rep2", 7);
    let r1 = run(&c1).expect("run 1");
    let r2 = run(&c2).expect("run 2");
    // determinism of the run itself (seeded ids + logical clock) across the 15-phase workflow.
    assert_eq!(r1.state, r2.state);
    let _ = std::fs::remove_file(&l1);
    let _ = std::fs::remove_file(&l2);
}

#[test]
#[ignore = "requires ANTHROPIC_API_KEY and a build with --features live"]
fn live_run_then_replay_is_identical() {
    if !cfg!(feature = "live") {
        eprintln!("skipping: built without --features live");
        return;
    }
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("skipping: ANTHROPIC_API_KEY not set");
        return;
    }
    let log = temp_log("live");
    let _ = std::fs::remove_file(&log);
    let config = RunConfig {
        intent: "USB-C powered IoT sensor node, < 5 W, < 50x50 mm".into(),
        reasoning: ReasoningChoice::Live,
        cassette: None,
        log: log.clone(),
        model: "claude-opus-4-8".into(),
        seed: 1,
        deterministic_clock: false,
    };
    let report = run(&config).expect("live run succeeds");
    assert!(!report.state.requirements.is_empty());
    let replayed = replay_cmd(&log).expect("replay succeeds");
    assert_eq!(report.state, replayed);
    let _ = std::fs::remove_file(&log);
}
