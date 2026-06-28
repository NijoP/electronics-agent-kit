//! `eak` — composition root + CLI driver (the Frameworks & Drivers ring).
//!
//! This is the only place concrete technology is chosen and wired (the file event log,
//! the reasoning adapter, the seeded id source, the clock). The command logic lives in
//! library functions so it is testable without spawning a process; `main.rs` is a thin
//! shell over [`run_cli`].

use eak_domain::RelationType;
use eak_phases::{
    BomPlanningMachine, BomVerificationMachine, ComponentPlacementMachine,
    ConstraintExtractionMachine, ConstraintVerificationMachine, DrcVerificationMachine,
    EngineeringAnalysisMachine, ErcVerificationMachine, PcbFloorPlanningMachine,
    RequirementPlanningMachine, RoutingPlanningMachine, SchematicPlanningMachine,
};
use eak_ports::ReasoningEngine;
use eak_reasoning::{Cassette, FixtureEngine};
use eak_runtime::{
    replay, Autonomy, Clock, LogicalClock, LoopBack, Orchestrator, RuntimeCore, SeededIdSource,
    SystemClock, WorkflowPlan,
};
use eak_store::FileEventLog;
use std::path::{Path, PathBuf};

pub use eak_domain::{EntityId, RelationType as Relation, RequirementStatus};
pub use eak_runtime::{EngineeringState, PhaseOutcome};

const DEFAULT_CASSETTE: &str = include_str!("../fixtures/default_cassette.json");

#[derive(Debug, Clone, Copy)]
pub enum ReasoningChoice {
    Fixture,
    Live,
}

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub intent: String,
    pub reasoning: ReasoningChoice,
    pub cassette: Option<PathBuf>,
    pub log: PathBuf,
    pub model: String,
    pub seed: u64,
    /// Use a logical (counter) clock for a fully reproducible run (tests).
    pub deterministic_clock: bool,
}

pub struct RunReport {
    pub outcomes: Vec<(String, PhaseOutcome)>,
    pub state: EngineeringState,
    pub log_path: PathBuf,
}

#[derive(Debug)]
pub enum CliError {
    Msg(String),
}
impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Msg(m) => write!(f, "{m}"),
        }
    }
}
impl std::error::Error for CliError {}
impl From<eak_ports::StoreError> for CliError {
    fn from(e: eak_ports::StoreError) -> Self {
        CliError::Msg(e.to_string())
    }
}

fn build_reasoning(cfg: &RunConfig) -> Result<Box<dyn ReasoningEngine>, CliError> {
    match cfg.reasoning {
        ReasoningChoice::Fixture => {
            let engine = match &cfg.cassette {
                Some(path) => {
                    FixtureEngine::load(path).map_err(|e| CliError::Msg(e.to_string()))?
                }
                None => {
                    let cassette: Cassette = serde_json::from_str(DEFAULT_CASSETTE)
                        .map_err(|e| CliError::Msg(e.to_string()))?;
                    FixtureEngine::from_cassette(cassette)
                }
            };
            Ok(Box::new(engine))
        }
        ReasoningChoice::Live => {
            #[cfg(feature = "live")]
            {
                let engine = eak_reasoning::AnthropicEngine::from_env(cfg.model.clone())
                    .map_err(|e| CliError::Msg(e.to_string()))?;
                Ok(Box::new(engine))
            }
            #[cfg(not(feature = "live"))]
            {
                let _ = cfg;
                Err(CliError::Msg(
                    "live reasoning requires building with --features live".into(),
                ))
            }
        }
    }
}

/// Run the default workflow on a design intent, building the reasoning engine from `cfg`.
/// Starts a fresh event log at `cfg.log`.
pub fn run(cfg: &RunConfig) -> Result<RunReport, CliError> {
    let reasoning = build_reasoning(cfg)?;
    run_with(reasoning, cfg)
}

/// Run the default workflow with a caller-supplied reasoning engine. Tests use this to
/// inject a fixture that drives a specific scenario (consistent / contradictory / waived)
/// without going through cassette files.
pub fn run_with(
    reasoning: Box<dyn ReasoningEngine>,
    cfg: &RunConfig,
) -> Result<RunReport, CliError> {
    let _ = std::fs::remove_file(&cfg.log); // a run starts a fresh project history
    let log = FileEventLog::open(&cfg.log)?;
    let ids = Box::new(SeededIdSource::new(cfg.seed));
    let clock: Box<dyn Clock> = if cfg.deterministic_clock {
        Box::new(LogicalClock::new())
    } else {
        Box::new(SystemClock)
    };

    let mut core = RuntimeCore::new(Box::new(log), reasoning, ids, clock, Autonomy::Autonomous);
    core.capture_intent(&cfg.intent, "engineer")?;

    let mut plan = default_workflow();
    let outcomes = Orchestrator::new().run(&mut plan, &mut core);

    Ok(RunReport {
        outcomes,
        state: core.state.clone(),
        log_path: cfg.log.clone(),
    })
}

/// The default Phase-3 workflow: Requirement Planning -> Engineering Analysis ->
/// Constraint Extraction -> Constraint Verification -> Schematic Planning -> ERC
/// Verification -> BOM Planning -> BOM Verification -> PCB Floor Planning ->
/// Component Placement -> Routing Planning -> DRC Verification. Only Requirement Planning
/// reasons (P3); every other phase here is deterministic, so a run replays bit-identically (P4).
///
/// Four correctness-loop edges bound the self-correction: a failed constraint verification
/// routes back to extraction, a failed ERC routes back to schematic planning, a failed
/// BOM verification routes back to BOM planning, and a failed DRC routes back to routing
/// planning (clearance/geometry defects are routing defects — the canonical loop-back target;
/// each capped at `max_retries` 2). Because extraction, schematic planning, BOM planning, and
/// routing planning are *idempotent* (a re-entry produces the identical artifact), the loop is a
/// no-op recovery for a design that is genuinely infeasible: it deterministically exhausts the
/// retries and then surfaces the open, blocking violation rather than looping forever (P13). The
/// loop-back is the seam where a future reasoning-assisted re-synthesis (or a human waiver
/// between passes) can actually change the artifact and clear the violation.
fn default_workflow() -> WorkflowPlan {
    WorkflowPlan::with_loopbacks(
        vec![
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
        ],
        vec![
            LoopBack {
                from: "ConstraintVerification".into(),
                to: "ConstraintExtraction".into(),
                max_retries: 2,
            },
            LoopBack {
                from: "ErcVerification".into(),
                to: "SchematicPlanning".into(),
                max_retries: 2,
            },
            LoopBack {
                from: "BomVerification".into(),
                to: "BomPlanning".into(),
                max_retries: 2,
            },
            LoopBack {
                from: "DrcVerification".into(),
                to: "RoutingPlanning".into(),
                max_retries: 2,
            },
        ],
    )
}

/// Replay an event log into a reconstructed [`EngineeringState`] (no model, no clock).
pub fn replay_cmd(log_path: &Path) -> Result<EngineeringState, CliError> {
    let log = FileEventLog::open(log_path)?;
    let state = replay(&log)?;
    Ok(state)
}

/// Render the provenance chain for a requirement (by short or full hex id).
pub fn trace_cmd(log_path: &Path, requirement: &str) -> Result<String, CliError> {
    let log = FileEventLog::open(log_path)?;
    let state = replay(&log)?;
    let req = state
        .requirements
        .iter()
        .find(|r| r.id.to_hex() == requirement || r.id.short() == requirement)
        .ok_or_else(|| CliError::Msg(format!("requirement {requirement} not found")))?;

    let mut out = String::new();
    out.push_str(&format!(
        "Requirement {} [{:?} / {:?}]\n  \"{}\"\n  acceptance: {}\n",
        req.id.short(),
        req.category,
        req.status,
        req.statement,
        req.acceptance_criterion
    ));

    if let Some(link) = state
        .links
        .iter()
        .find(|l| l.from == req.id && l.relation == RelationType::JustifiedBy)
    {
        if let Some(dec) = state.decision(link.to) {
            out.push_str(&format!(
                "  +- Decision {} by {} (confidence {:.2}, reasoning call #{})\n     rationale: {}\n",
                dec.id.short(),
                dec.decider,
                dec.confidence,
                dec.reasoning_call_seq
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "none".into()),
                dec.rationale
            ));
            for evid in &dec.evidence {
                if let Some(ev) = state.evidence_item(*evid) {
                    out.push_str(&format!(
                        "     +- Evidence {} ({:?}) from {}\n",
                        ev.id.short(),
                        ev.kind,
                        ev.source
                    ));
                }
            }
        }
    }

    if let Some(intent) = &state.intent {
        if req.source == intent.id {
            out.push_str(&format!(
                "  +- derives from Design Intent {}: \"{}\"\n",
                intent.id.short(),
                intent.statement
            ));
        }
    }

    Ok(out)
}

// ----------------------------- CLI plumbing -----------------------------

use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "eak",
    version,
    about = "Electronics Agent Kit — Phase 1 runtime CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run Requirement Planning (+ Engineering Analysis stub) on a design intent.
    Run {
        #[arg(long)]
        intent: String,
        #[arg(long, default_value = "fixture")]
        reasoning: String,
        #[arg(long)]
        cassette: Option<PathBuf>,
        #[arg(long, default_value = "eak-events.jsonl")]
        log: PathBuf,
        #[arg(long, default_value = "claude-opus-4-8")]
        model: String,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long)]
        show_state: bool,
        /// Use a logical clock so the run is fully reproducible.
        #[arg(long)]
        deterministic: bool,
    },
    /// Replay an event log and print the reconstructed state.
    Replay {
        #[arg(long)]
        log: PathBuf,
        #[arg(long)]
        show_state: bool,
    },
    /// Print the provenance chain for a requirement (by short or full id).
    Trace {
        #[arg(long)]
        log: PathBuf,
        requirement: String,
    },
}

fn print_run(report: &RunReport, show_state: bool) {
    println!("Run complete. Event log: {}", report.log_path.display());
    for (phase, outcome) in &report.outcomes {
        let status = match outcome {
            PhaseOutcome::Success => "OK".to_string(),
            PhaseOutcome::Failed(r) => format!("FAILED: {r}"),
        };
        println!("  phase {phase:<22} {status}");
    }
    println!(
        "  requirements: {}  decisions: {}  evidence: {}  provenance links: {}",
        report.state.requirements.len(),
        report.state.decisions.len(),
        report.state.evidence.len(),
        report.state.links.len()
    );
    for r in &report.state.requirements {
        println!("    - [{}] {}", r.id.short(), r.statement);
    }
    if show_state {
        println!("\n{}", report.state.canonical_json());
    }
}

fn print_replay(state: &EngineeringState, show_state: bool) {
    println!(
        "Replayed: {} requirements, {} decisions, {} evidence, {} links",
        state.requirements.len(),
        state.decisions.len(),
        state.evidence.len(),
        state.links.len()
    );
    if show_state {
        println!("\n{}", state.canonical_json());
    }
}

pub fn run_cli() -> ExitCode {
    let cli = Cli::parse();
    let result: Result<(), CliError> = match cli.command {
        Command::Run {
            intent,
            reasoning,
            cassette,
            log,
            model,
            seed,
            show_state,
            deterministic,
        } => {
            let choice = match reasoning.as_str() {
                "fixture" => ReasoningChoice::Fixture,
                "live" => ReasoningChoice::Live,
                other => {
                    eprintln!("error: unknown --reasoning '{other}' (use fixture|live)");
                    return ExitCode::FAILURE;
                }
            };
            let cfg = RunConfig {
                intent,
                reasoning: choice,
                cassette,
                log,
                model,
                seed,
                deterministic_clock: deterministic,
            };
            run(&cfg).map(|report| print_run(&report, show_state))
        }
        Command::Replay { log, show_state } => {
            replay_cmd(&log).map(|state| print_replay(&state, show_state))
        }
        Command::Trace { log, requirement } => trace_cmd(&log, &requirement).map(|s| print!("{s}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
