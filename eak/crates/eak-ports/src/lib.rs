//! Ports (contracts) for Electronics Agent Kit — Use-case ring.
//!
//! Inner rings DEFINE these contracts; outer-ring adapters IMPLEMENT them
//! (`docs/core/contracts.md`, P1). Phase 1 exposes the two adapter boundaries —
//! the [`EventLog`] (implemented by `eak-store`) and the [`ReasoningEngine`]
//! (implemented by `eak-reasoning`) — plus the [`Event`] type they carry.
//!
//! The kernel-internal protocol surfaces (AgentContext, FSM framework, capability
//! handlers) live in `eak-runtime`: they are not implemented by outer adapters, so by
//! the "a contract lives with the ring that needs it" rule they belong to the kernel.

use eak_domain::{
    Board, BomLineItem, Component, Constraint, Decision, DesignIntent, Evidence, FunctionalBlock,
    Net, Part, Pin, Placement, Priority, ProvenanceLink, Requirement, RequirementCategory,
    Violation, Waiver,
};
use eak_units::PhysicalQuantity;
use serde::{Deserialize, Serialize};

/// Monotonic event sequence number — the address of a fact in history.
pub type Seq = u64;

/// Wall-clock instant (unix epoch milliseconds). Recorded in every event and never
/// re-read on replay (determinism, P4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(pub i64);

// ===================== Event-log boundary (impl: eak-store) =====================

/// An event with its assigned position and recorded time. The unit of provenance and
/// the basis of deterministic replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRecord {
    pub seq: Seq,
    pub timestamp: Timestamp,
    pub event: Event,
}

/// Every design-significant change in Phase 1. Entity-bearing variants are *state
/// deltas* (folded into Engineering State); the rest are audit/provenance markers.
/// Per P5 every committed transition emits at least one event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    // ---- phase lifecycle (audit) ----
    PhaseEntered {
        phase: String,
        state: String,
    },
    PhaseStateChanged {
        phase: String,
        from: String,
        to: String,
    },
    PhaseCompleted {
        phase: String,
        outcome: String,
    },
    PhaseFailed {
        phase: String,
        reason: String,
    },

    // ---- reasoning boundary (provenance) ----
    ReasoningCall {
        request: ReasoningRequest,
        response: ReasoningResponse,
    },

    // ---- state deltas ----
    IntentCaptured {
        intent: DesignIntent,
    },
    EvidenceReferenced {
        evidence: Evidence,
    },
    DecisionCreated {
        decision: Decision,
    },
    RequirementCommitted {
        requirement: Requirement,
    },
    ProvenanceLinked {
        link: ProvenanceLink,
    },

    // ---- Phase 2: verification state deltas ----
    ConstraintCommitted {
        constraint: Constraint,
    },
    ViolationRaised {
        violation: Violation,
    },
    WaiverGranted {
        waiver: Waiver,
    },

    // ---- Phase 2: verification milestones (audit) ----
    ConstraintsExtracted {
        count: usize,
    },
    VerificationCompleted {
        rule_count: usize,
        open_violations: usize,
    },

    // ---- Phase 3: synthesis state deltas ----
    FunctionalBlockCommitted {
        block: FunctionalBlock,
    },
    ComponentCommitted {
        component: Component,
    },
    PinCommitted {
        pin: Pin,
    },
    NetCommitted {
        net: Net,
    },

    // ---- Phase 3 (BOM): bill-of-materials state deltas ----
    PartCommitted {
        part: Part,
    },
    BomLineItemCommitted {
        item: BomLineItem,
    },

    // ---- IR boundary milestones (audit) ----
    RequirementIrProduced {
        schema_version: u32,
        requirement_count: usize,
    },
    EngineeringIrProduced {
        schema_version: u32,
        block_count: usize,
    },
    SchematicIrProduced {
        schema_version: u32,
        net_count: usize,
    },
    BomIrProduced {
        schema_version: u32,
        line_item_count: usize,
    },

    // ---- Phase 3 (PCB): layout state deltas ----
    BoardCommitted {
        board: Board,
    },
    PlacementCommitted {
        placement: Placement,
    },

    // ---- Phase 3 (PCB): IR boundary milestone (audit) ----
    PcbIrProduced {
        schema_version: u32,
        placement_count: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Io(String),
    Serialization(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(m) => write!(f, "store io error: {m}"),
            StoreError::Serialization(m) => write!(f, "store serialization error: {m}"),
        }
    }
}
impl std::error::Error for StoreError {}

/// Append-only, ordered event log (event-sourcing, ADR-0004). The single source of
/// truth; state is its fold. Implemented by `eak-store`.
pub trait EventLog {
    /// Append timestamped events atomically; assigns consecutive [`Seq`]s and returns them.
    fn append(&mut self, events: &[(Timestamp, Event)]) -> Result<Vec<Seq>, StoreError>;
    /// Read the full ordered history (basis of replay and provenance).
    fn read_all(&self) -> Result<Vec<EventRecord>, StoreError>;
    /// The next sequence number that would be assigned.
    fn next_seq(&self) -> Seq;
}

// ===================== Reasoning boundary (impl: eak-reasoning) =====================

/// A structured request for judgement. The prompt is *data the runtime composes*; the
/// schema names the shape the answer must take. `seed`/`temperature`/`model_id` are the
/// decisive parameters recorded for reproducibility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasoningRequest {
    pub model_id: String,
    pub system: String,
    pub prompt: String,
    pub schema_name: String,
    pub temperature: f64,
    pub seed: u64,
}

/// One candidate requirement proposed by the reasoning engine — *judgement only*, not
/// yet validated or committed (the seam is in the agent's deterministic half, P3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateRequirement {
    pub statement: String,
    pub category: RequirementCategory,
    pub priority: Priority,
    pub acceptance_criterion: String,
    pub source_hint: String,
    pub confidence: f64,
    pub rationale: String,
    #[serde(default)]
    pub targets: Vec<PhysicalQuantity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasoningResponse {
    pub candidates: Vec<CandidateRequirement>,
    #[serde(default)]
    pub clarifying_questions: Vec<String>,
    #[serde(default)]
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReasoningError {
    Provider(String),
    Schema(String),
    Unavailable,
}

impl std::fmt::Display for ReasoningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReasoningError::Provider(m) => write!(f, "reasoning provider error: {m}"),
            ReasoningError::Schema(m) => write!(f, "reasoning schema violation: {m}"),
            ReasoningError::Unavailable => write!(f, "reasoning engine unavailable"),
        }
    }
}
impl std::error::Error for ReasoningError {}

/// The single boundary to stochastic judgement (P3). Implemented by `eak-reasoning`
/// (fixture + live Anthropic). Phase 1 uses the synchronous request form; the spec's
/// stream/cancel operations are deferred.
pub trait ReasoningEngine {
    /// Stable identifier of the engine/model in use (e.g. `"fixture"` or
    /// `"anthropic:claude-opus-4-8"`), recorded with each call for reproducibility.
    fn model_id(&self) -> String;
    fn request_judgement(
        &self,
        req: &ReasoningRequest,
    ) -> Result<ReasoningResponse, ReasoningError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_domain::{EntityId, Priority, RequirementCategory, RequirementStatus};

    #[test]
    fn event_roundtrips_through_json() {
        let ev = Event::RequirementCommitted {
            requirement: Requirement {
                id: EntityId(7),
                statement: "Operating power shall not exceed 5 W".into(),
                category: RequirementCategory::Electrical,
                priority: Priority::High,
                acceptance_criterion: "measured power < 5 W".into(),
                status: RequirementStatus::Accepted,
                source: EntityId(1),
                targets: vec![PhysicalQuantity::new(5.0, eak_units::Unit::Watt)],
            },
        };
        let s = serde_json::to_string(&ev).unwrap();
        let back: Event = serde_json::from_str(&s).unwrap();
        assert_eq!(ev, back);
    }
}
