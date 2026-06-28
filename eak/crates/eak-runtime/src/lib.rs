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
pub use orchestrator::{Orchestrator, WorkflowPlan};
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
    use eak_domain::{Decision, Priority, Requirement, RequirementCategory, RequirementStatus};
    use eak_ports::{
        Event, EventLog, EventRecord, ReasoningEngine, ReasoningError, ReasoningRequest,
        ReasoningResponse, Seq, StoreError, Timestamp,
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
}
