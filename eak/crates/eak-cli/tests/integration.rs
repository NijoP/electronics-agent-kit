//! End-to-end verification of the Phase-3 exit criteria over the full 6-phase workflow:
//! Requirement Planning -> Engineering Analysis -> Constraint Extraction -> Constraint
//! Verification -> Schematic Planning -> ERC Verification. A consistent, realizable design
//! runs all six phases clean; an electrically-invalid one (a power net with consumers but no
//! driver) is caught by ERC, routed back to Schematic Planning automatically, and left fully
//! traceable to its cause. The Phase-1/2 guarantees (multi-phase orchestration, full
//! requirement traceability, the constraint correctness loop, byte-identical replay) still
//! hold over the larger workflow.

use eak_cli::{
    replay_cmd, run, run_with, trace_cmd, PhaseOutcome, ReasoningChoice, Relation, RunConfig,
};
use eak_domain::{Priority, RequirementCategory, ViolationSeverity, ViolationStatus};
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

#[test]
fn run_replays_byte_identical_and_runs_six_phases() {
    let (config, log) = cfg("det", 1);
    let report = run(&config).expect("run succeeds");

    // Full Phase-3 workflow on a consistent, realizable design: RP -> Engineering Analysis ->
    // Constraint Extraction -> Constraint Verification -> Schematic Planning -> ERC, all OK.
    assert_eq!(report.outcomes.len(), 6);
    assert!(report
        .outcomes
        .iter()
        .all(|(_, o)| matches!(o, PhaseOutcome::Success)));

    // Three requirements; two carry targets, so two constraints; no violations.
    assert_eq!(report.state.requirements.len(), 3);
    assert_eq!(report.state.constraints.len(), 2);
    assert!(report.state.violations.is_empty());

    // The synthesis layer was realized: blocks, components, and at least one net exist.
    assert!(!report.state.functional_blocks.is_empty());
    assert!(!report.state.components.is_empty());
    assert!(!report.state.nets.is_empty());

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
    // synthesis nor ERC ran. (Engineering Analysis sits *before* the gate, so it did run.)
    assert!(!report
        .outcomes
        .iter()
        .any(|(n, _)| n == "SchematicPlanning"));
    assert!(!report.outcomes.iter().any(|(n, _)| n == "ErcVerification"));
    assert!(report.state.components.is_empty());

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
    // determinism of the run itself (seeded ids + logical clock) across the 6-phase workflow.
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
