//! The Agent Runtime Protocol (`docs/core/agent-runtime-protocol.md`).
//!
//! The execution engine hands an agent's deterministic half an [`AgentContext`]: the only
//! surface through which it may read state, reason (P3), mint ids, and propose mutations
//! via a [`CapabilityRequest`] (P2). Agents never touch state or a model directly.

use eak_domain::{
    Board, BomLineItem, Component, Constraint, Decision, DesignIntent, EntityId, Evidence,
    FunctionalBlock, Net, Part, Pin, Placement, ProvenanceLink, Requirement, Track, Violation,
    Waiver,
};
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
    /// Commit a [`Constraint`] derived from a requirement, with its provenance links
    /// (Phase 2). The runtime re-validates (non-empty statement + non-null subject).
    CreateConstraint {
        constraint: Constraint,
        links: Vec<ProvenanceLink>,
    },
    /// Raise a [`Violation`] found by the verification engine, with links to the entities
    /// it implicates so it is traceable to its cause.
    RaiseViolation {
        violation: Violation,
        links: Vec<ProvenanceLink>,
    },
    /// Accept an existing violation rather than fix it. The runtime checks the target
    /// violation exists; folding the event flips it to `Waived`.
    GrantWaiver { waiver: Waiver },
    /// Commit a [`FunctionalBlock`] together with its provenance links (Phase 3). The
    /// runtime re-validates (non-empty name + at least one realized requirement) at the seam.
    CreateFunctionalBlock {
        block: FunctionalBlock,
        links: Vec<ProvenanceLink>,
    },
    /// Realize a [`Component`] with its [`Pin`]s and provenance links (Phase 3). The runtime
    /// re-validates the component (non-empty refdes + a non-null originating block), then
    /// commits the component, one event per pin, and the links — one atomic realization.
    RealizeComponent {
        component: Component,
        pins: Vec<Pin>,
        links: Vec<ProvenanceLink>,
    },
    /// Commit a [`Net`] joining pins, with its provenance links (Phase 3). The runtime
    /// re-validates (non-empty name + at least one member pin) at the seam.
    CreateNet {
        net: Net,
        links: Vec<ProvenanceLink>,
    },
    /// Commit a concrete [`Part`] with its provenance links (Phase 3 BOM). The runtime
    /// re-validates the part (non-empty manufacturer part number) at the seam.
    CreatePart {
        part: Part,
        links: Vec<ProvenanceLink>,
    },
    /// Commit a [`BomLineItem`] binding a part to the components it realizes, with its
    /// provenance links (Phase 3 BOM). The runtime re-checks quantity/membership and the
    /// referential integrity of the part and every covered component at the seam.
    CreateBomLineItem {
        item: BomLineItem,
        links: Vec<ProvenanceLink>,
    },
    /// Commit the single [`Board`] outline the design must fit within, with its provenance
    /// links (Phase 3 PCB). The runtime re-validates the outline and rejects a second board
    /// at the seam — a design has exactly one outline.
    CreateBoard {
        board: Board,
        links: Vec<ProvenanceLink>,
    },
    /// Place one [`Component`] on the board, with its provenance links (Phase 3 PCB). The
    /// runtime re-validates the courtyard, checks the component exists, requires the board to
    /// exist first, and rejects a second placement of the same component at the seam.
    PlaceComponent {
        placement: Placement,
        links: Vec<ProvenanceLink>,
    },
    /// Route one [`Net`] as a copper [`Track`], with its provenance links (Phase 3 routing).
    /// The runtime re-validates the trace (positive width, finite endpoints), checks the net
    /// exists, and requires the board to exist first. A net is realized by a DAISY-CHAIN of one
    /// or more tracks (one segment per consecutive member pad), so the seam permits several
    /// tracks per net; idempotency is the Routing Planning machine's concern, not the seam's.
    RouteNet {
        track: Track,
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
    /// Phase 2: read the committed constraints (verification input).
    fn constraints(&self) -> Vec<Constraint>;
    /// Phase 2: read the raised violations (so a re-verify can skip duplicates).
    fn violations(&self) -> Vec<Violation>;
    /// Phase 3: read the committed functional blocks (synthesis input).
    fn functional_blocks(&self) -> Vec<FunctionalBlock>;
    /// Phase 3: read the realized components.
    fn components(&self) -> Vec<Component>;
    /// Phase 3: read the realized pins.
    fn pins(&self) -> Vec<Pin>;
    /// Phase 3: read the committed nets (ERC + schematic IR input).
    fn nets(&self) -> Vec<Net>;
    /// Phase 3 (BOM): read the committed parts (BOM verification + IR input).
    fn parts(&self) -> Vec<Part>;
    /// Phase 3 (BOM): read the committed BOM line items.
    fn bom_line_items(&self) -> Vec<BomLineItem>;
    /// Phase 3 (PCB): read the committed board outline, if one exists yet.
    fn board(&self) -> Option<Board>;
    /// Phase 3 (PCB): read the committed component placements (DRC + PCB IR input).
    fn placements(&self) -> Vec<Placement>;
    /// Phase 3 (routing): read the committed tracks (DRC + PCB IR input).
    fn tracks(&self) -> Vec<Track>;
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
