//! End-to-end verification of the Phase-2 exit criterion: a design that violates a
//! constraint is caught and routed back automatically, and the violation is fully traceable
//! to its cause — plus the Phase-1 guarantees (multi-phase orchestration, full requirement
//! traceability, byte-identical replay) still hold over the larger 4-phase workflow.

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
/// (<= 5 W and >= 8 W) — drives the infeasible scenario.
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

#[test]
fn run_replays_byte_identical_and_runs_four_phases() {
    let (config, log) = cfg("det", 1);
    let report = run(&config).expect("run succeeds");

    // Phase 2 workflow: RP -> Constraint Extraction -> Constraint Verification ->
    // Engineering Analysis (stub), all OK on a consistent design.
    assert_eq!(report.outcomes.len(), 4);
    assert!(report
        .outcomes
        .iter()
        .all(|(_, o)| matches!(o, PhaseOutcome::Success)));

    // three requirements; two carry targets, so two constraints; no violations.
    assert_eq!(report.state.requirements.len(), 3);
    assert_eq!(report.state.constraints.len(), 2);
    assert!(report.state.violations.is_empty());

    // EXIT CRITERION (Phase 1, preserved): history replays to identical state, byte for byte.
    let replayed = replay_cmd(&log).expect("replay succeeds");
    assert_eq!(report.state, replayed);
    assert_eq!(report.state.canonical_json(), replayed.canonical_json());

    let _ = std::fs::remove_file(&log);
}

#[test]
fn infeasible_design_is_caught_routed_back_and_left_traceable() {
    let (config, log) = cfg("infeasible", 3);
    let report = run_with(contradictory_engine(), &config).expect("run completes with a failure");

    // The correctness loop fired: verification failed and was routed back to extraction,
    // bounded to 1 initial + 2 retries = 3 verification runs.
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

    // Engineering Analysis never ran — the workflow did not complete past the failed gate.
    assert!(!report
        .outcomes
        .iter()
        .any(|(n, _)| n == "EngineeringAnalysis"));

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
    // determinism of the run itself (seeded ids + logical clock) across the 4-phase workflow.
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
