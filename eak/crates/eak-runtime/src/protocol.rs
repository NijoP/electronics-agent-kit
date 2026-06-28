//! The Agent Runtime Protocol (`docs/core/agent-runtime-protocol.md`).
//!
//! The execution engine hands an agent's deterministic half an [`AgentContext`]: the only
//! surface through which it may read state, reason (P3), mint ids, and propose mutations
//! via a [`CapabilityRequest`] (P2). Agents never touch state or a model directly.

use eak_domain::{Decision, DesignIntent, EntityId, Evidence, ProvenanceLink, Requirement};
use eak_ports::{Event, ReasoningError, ReasoningRequest, ReasoningResponse, Seq, StoreError};

/// Autonomy level (P10). Phase 1 exercises `Autonomous`; `Supervised` is modelled but its
/// human-approval path is deferred to a later phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Autonomy {
    Autonomous,
    Supervised,
}

/// What the execution engine tells an agent when it activates it.
#[derive(Debug, Clone)]
pub struct AgentActivation {
    pub phase: String,
    pub goal: String,
    pub budget: Budget,
}

#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub max_reasoning_calls: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentOutcome {
    Success { committed: usize },
    NeedsHuman(String),
    Failed(String),
}

/// A proposed mutation. The only way an agent acts on the world (Capability port). The
/// runtime validates, records, and applies it — the agent never mutates state itself.
#[derive(Debug, Clone, PartialEq)]
pub enum CapabilityRequest {
    /// Commit a requirement together with its justifying decision, supporting evidence,
    /// and provenance links — one atomic engineering act with its provenance attached.
    CreateRequirement {
        requirement: Requirement,
        decision: Decision,
        evidence: Vec<Evidence>,
        links: Vec<ProvenanceLink>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityError {
    Rejected(String),
}
impl std::fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapabilityError::Rejected(m) => write!(f, "capability rejected: {m}"),
        }
    }
}
impl std::error::Error for CapabilityError {}

#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityAck {
    pub committed: Vec<Seq>,
}

/// The protocol surface the runtime provides and agents consume.
pub trait AgentContext {
    fn autonomy(&self) -> Autonomy;
    fn fresh_id(&mut self) -> EntityId;
    fn design_intent(&self) -> Option<DesignIntent>;
    fn requirements(&self) -> Vec<Requirement>;
    fn provenance_links(&self) -> Vec<ProvenanceLink>;
    /// Call the reasoning engine, record the call (returning its event [`Seq`]), and
    /// return the judgement. Recording here is what makes replay deterministic (P4).
    fn reason(&mut self, req: ReasoningRequest)
        -> Result<(Seq, ReasoningResponse), ReasoningError>;
    /// Propose a validated mutation (the only write path for an agent).
    fn invoke(&mut self, req: CapabilityRequest) -> Result<CapabilityAck, CapabilityError>;
    /// Emit trusted audit / input events (phase lifecycle, captured intent, IR markers).
    fn emit(&mut self, events: Vec<Event>) -> Result<Vec<Seq>, StoreError>;
}

/// A phase's driving agent — the *instance* half of P8 (the reasoning adapter is inside
/// the impl, reached only through [`AgentContext::reason`]).
pub trait Agent {
    fn name(&self) -> &str;
    fn activate(
        &mut self,
        ctx: &mut dyn AgentContext,
        activation: &AgentActivation,
    ) -> AgentOutcome;
}
