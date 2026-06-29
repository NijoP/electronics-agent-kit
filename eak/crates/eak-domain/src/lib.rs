//! Engineering domain model — the Phase-1 entity subset (Entities ring).
//!
//! Phase 1 (Requirement Planning) needs exactly five entities plus one first-class
//! relationship: [`DesignIntent`], [`Requirement`], [`Decision`], [`Evidence`], and
//! [`ProvenanceLink`]. Downstream entities (Component, Net, Constraint, ...) are NOT
//! modelled in Phase 1. See `docs/foundation/engineering-domain-model.md`.

use eak_units::{PhysicalQuantity, Unit};
use serde::{Deserialize, Serialize};

/// Opaque, immutable identity (domain-model modelling principle 1). Carries no meaning;
/// referenced by value, never by name or position. `EntityId(0)` is reserved as the null
/// sentinel and is never minted by the runtime's id source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntityId(pub u128);

impl EntityId {
    pub const NULL: EntityId = EntityId(0);

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    pub fn to_hex(self) -> String {
        format!("{:032x}", self.0)
    }

    /// Short 8-hex-digit form for human-facing traces.
    pub fn short(self) -> String {
        format!("{:08x}", self.0 as u32)
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequirementCategory {
    Functional,
    Electrical,
    Mechanical,
    Thermal,
    Regulatory,
    /// Fabrication/assembly *process* limits — what the chosen fab and assembly flow can build
    /// (minimum trace width, drill sizes, layer count, panelization). Distinct from `Regulatory`
    /// (external standards/compliance): a process floor is a property of the manufacturer, not a
    /// regulation. The trace-width DRC floor and future fab/process rules read from this category.
    Fabrication,
    Cost,
    Schedule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RequirementStatus {
    Proposed,
    Accepted,
    Satisfied,
    Violated,
    Waived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Priority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationType {
    DerivedFrom,
    JustifiedBy,
    BasedOnReasoning,
    Supports,
    TracesTo,
    Supersedes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EvidenceKind {
    DesignIntentSource,
    StandardClause,
    PriorDesign,
    DatasheetParameter,
    ReviewNote,
}

/// The originating goal, preserved verbatim and as a structured summary. Never deleted,
/// only refined (domain-model entity lifecycle).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignIntent {
    pub id: EntityId,
    pub statement: String,
    pub structured_summary: String,
    pub source: String,
}

/// A single testable statement the design must satisfy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Requirement {
    pub id: EntityId,
    pub statement: String,
    pub category: RequirementCategory,
    pub priority: Priority,
    pub acceptance_criterion: String,
    pub status: RequirementStatus,
    /// The DesignIntent (or external standard entity) this requirement is rooted in.
    pub source: EntityId,
    /// Typed physical targets within the requirement (P9).
    pub targets: Vec<PhysicalQuantity>,
}

/// The justification for a design-significant change (P5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub id: EntityId,
    pub subject: EntityId,
    pub rationale: String,
    pub decider: String,
    /// Sequence number of the recorded reasoning call this decision relied on, if any.
    pub reasoning_call_seq: Option<u64>,
    pub evidence: Vec<EntityId>,
    pub confidence: f64,
}

/// A fact supporting a [`Decision`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub id: EntityId,
    pub kind: EvidenceKind,
    pub content_reference: String,
    pub source: String,
    pub reliability: f64,
}

/// A first-class, addressed relationship ("X relation Y") — the edges of the
/// provenance graph (shared-state-model.md identity rule 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceLink {
    pub id: EntityId,
    pub from: EntityId,
    pub to: EntityId,
    pub relation: RelationType,
}

// ===================== Phase 2: verification entities =====================
//
// Phase 2 adds the machine-checkable layer on top of the Phase-1 intent layer: a
// [`Constraint`] is a typed bound derived from a [`Requirement`]'s physical target; a
// [`Violation`] is a first-class, addressed breach of a verification rule; a [`Waiver`]
// is the recorded decision to accept a violation. See
// `docs/engineering/constraint-engine.md` and `docs/engineering/verification-engine.md`.

/// Comparison sense of a [`Constraint`]'s bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConstraintKind {
    /// The subject value must not exceed the bound (e.g. "power <= 5 W").
    Max,
    /// The subject value must be at least the bound (e.g. "power >= 8 W").
    Min,
    /// The subject value must equal the bound.
    Equal,
}

/// Lifecycle of a [`Constraint`]. Constraints are never deleted, only superseded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConstraintStatus {
    Active,
    Superseded,
}

/// A machine-checkable bound on a [`Requirement`]'s physical target (P9). Derived from a
/// requirement, never authored directly; carries the unit so verification is dimensionally
/// unambiguous.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    pub id: EntityId,
    pub statement: String,
    /// The requirement this constraint bounds.
    pub subject_requirement: EntityId,
    pub kind: ConstraintKind,
    pub bound: PhysicalQuantity,
    /// The entity (usually the subject requirement) this constraint derives from.
    pub source: EntityId,
    pub status: ConstraintStatus,
}

impl Constraint {
    pub fn is_active(&self) -> bool {
        self.status == ConstraintStatus::Active
    }

    /// Domain invariant: a constraint carries a non-empty statement (reuses
    /// [`DomainError::EmptyStatement`]). The null-subject check lives in the capability
    /// handler, so no new error variant is needed.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.statement.trim().is_empty() {
            return Err(DomainError::EmptyStatement);
        }
        Ok(())
    }
}

/// How serious a [`Violation`] is. Only [`ViolationSeverity::Error`] can block a workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViolationSeverity {
    Error,
    Warning,
    Info,
}

/// Lifecycle of a [`Violation`]. An [`Open`](ViolationStatus::Open) error blocks; a
/// [`Waived`](ViolationStatus::Waived) or [`Resolved`](ViolationStatus::Resolved) one does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViolationStatus {
    Open,
    Waived,
    Resolved,
}

/// A detected breach of a verification rule, made first-class so it is addressed and fully
/// traceable to its cause via [`Violation::subjects`] + provenance links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    pub id: EntityId,
    /// Stable identifier of the rule that raised it (e.g. `"constraint-consistency"`).
    pub rule: String,
    pub severity: ViolationSeverity,
    /// The entities implicated (constraints, requirements, ...). The traceability anchor.
    pub subjects: Vec<EntityId>,
    pub message: String,
    pub status: ViolationStatus,
}

impl Violation {
    /// A violation blocks a workflow iff it is an unaddressed error (P13: failures are
    /// explicit, never silently dropped).
    pub fn is_blocking(&self) -> bool {
        self.severity == ViolationSeverity::Error && self.status == ViolationStatus::Open
    }
}

/// A recorded decision to accept a [`Violation`] rather than fix it (P5: every
/// design-significant change is justified; P10: the human/agent who decided is named).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiver {
    pub id: EntityId,
    pub violation: EntityId,
    pub justification: String,
    pub decided_by: String,
}

// ===================== Phase 3: schematic entities =====================
//
// Phase 3 adds the realization layer beneath the Phase-2 verification layer: a
// [`FunctionalBlock`] groups the requirements it realizes; a [`Component`] is a concrete
// part minted from a block; a [`Pin`] is an addressed terminal of a component; a [`Net`]
// is a first-class electrical connection between pins. Each entity carries the upstream
// trace it derives from (P3), so the schematic stays explainable back to intent. See
// `docs/engineering/schematic-model.md`.

/// A unit of design intent realized as part of the architecture: it names a function and
/// the requirements (P3) it is responsible for satisfying. Components are minted from a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionalBlock {
    pub id: EntityId,
    pub name: String,
    pub function: String,
    /// The requirements this block is responsible for realizing.
    pub requirements: Vec<EntityId>,
}

impl FunctionalBlock {
    /// Domain invariant: a block carries a non-empty name. Requirement-link integrity
    /// (each id exists) is re-checked at the capability seam (P3).
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.name.trim().is_empty() {
            return Err(DomainError::EmptyField("functional block name"));
        }
        Ok(())
    }
}

/// The coarse kind of a [`Component`]. Drives ERC expectations (e.g. a regulator is a
/// power source, a connector may be a sink).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComponentClass {
    Connector,
    Regulator,
    Ic,
    Resistor,
    Capacitor,
}

/// The electrical role of a [`Pin`]. Drives ERC drive/sink analysis (P9): a power net must
/// have a driver ([`PowerOut`](PinElectricalType::PowerOut)); two outputs on one net contend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PinElectricalType {
    PowerIn,
    PowerOut,
    Input,
    Output,
    Bidirectional,
    Passive,
    Ground,
    NoConnect,
}

/// The electrical class of a [`Net`]. Drives ERC: power and ground nets demand a driver,
/// signal nets are checked for contention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NetClass {
    Power,
    Ground,
    Signal,
}

/// A concrete part realizing some of a [`FunctionalBlock`]'s function. Minted from a block,
/// so it is always traceable back to the intent it serves (P3). The optional `value` is a
/// typed physical quantity (e.g. a resistor's 10 kΩ), hence `Component` is not `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub id: EntityId,
    pub refdes: String,
    pub class: ComponentClass,
    pub value: Option<PhysicalQuantity>,
    /// The functional block this component was realized from.
    pub from_block: EntityId,
}

impl Component {
    /// Domain invariant: a component carries a non-empty reference designator. The
    /// block-link existence check lives at the capability seam (P3).
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.refdes.trim().is_empty() {
            return Err(DomainError::EmptyField("component reference designator"));
        }
        Ok(())
    }
}

/// An addressed terminal of a [`Component`]. Referenced by id from [`Net::members`], never
/// by position, so connectivity survives renumbering (domain-model identity rule).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    pub id: EntityId,
    pub component: EntityId,
    pub designation: String,
    pub electrical_type: PinElectricalType,
}

/// A first-class electrical connection joining a set of [`Pin`]s. Made addressable so ERC
/// findings can name the offending net and trace it (P3, P13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Net {
    pub id: EntityId,
    pub name: String,
    pub class: NetClass,
    /// The pin ids joined by this net.
    pub members: Vec<EntityId>,
}

impl Net {
    /// Domain invariant: a net carries a non-empty name. Member-pin integrity (each id
    /// exists) is re-checked at the capability seam (P3).
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.name.trim().is_empty() {
            return Err(DomainError::EmptyField("net name"));
        }
        Ok(())
    }
}

// ===================== Phase 3 (increment 2): BOM entities =====================
//
// The bill-of-materials layer binds the abstract [`Component`]s of the schematic to
// concrete, orderable [`Part`]s. A [`Part`] is a manufacturer part number with its
// lifecycle state; a [`BomLineItem`] is the first-class binding of one part to the set
// of components it realizes, with a build quantity. Lifecycle is carried so the BOM gate
// can flag end-of-life parts (P13: procurement risk is surfaced, never silently shipped).
// See `docs/engineering/bom-model.md`.

/// Procurement lifecycle of a [`Part`]. An [`Eol`](PartLifecycle::Eol) part can no longer be
/// sourced and must block; an [`Nrnd`](PartLifecycle::Nrnd) (not-recommended-for-new-designs)
/// part is a warning the designer should heed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PartLifecycle {
    Active,
    Nrnd,
    Eol,
}

/// A concrete, orderable part identified by its manufacturer part number. Bound to the
/// abstract [`Component`]s it realizes through a [`BomLineItem`], so the BOM stays traceable
/// back to the schematic (P3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub id: EntityId,
    pub mpn: String,
    pub manufacturer: String,
    pub lifecycle: PartLifecycle,
    pub datasheet: String,
}

impl Part {
    /// Domain invariant: a part carries a non-empty manufacturer part number — without it
    /// the part cannot be ordered (P13). Manufacturer/datasheet completeness is a softer
    /// concern checked downstream, not a hard domain invariant.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.mpn.trim().is_empty() {
            return Err(DomainError::EmptyField("manufacturer part number"));
        }
        Ok(())
    }
}

/// A first-class binding of one [`Part`] to the [`Component`]s it realizes, with a build
/// quantity. Made addressable so BOM findings can name the offending line and trace it back
/// to both the part and its components (P3, P13). Quantity/component-link integrity is
/// re-checked at the capability seam.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BomLineItem {
    pub id: EntityId,
    /// The part this line orders.
    pub part: EntityId,
    /// The component ids this line covers.
    pub components: Vec<EntityId>,
    pub quantity: u32,
}

impl BomLineItem {
    /// Domain invariants: a line covers at least one component, its quantity equals the
    /// number of components it covers, and it lists no component twice. Part/component
    /// existence and cross-line single-sourcing are re-checked at the capability seam (P3).
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.components.is_empty() {
            return Err(DomainError::EmptyField("BOM line item components"));
        }
        if self.quantity as usize != self.components.len() {
            return Err(DomainError::Inconsistent(
                "BOM line item quantity must equal the number of components it covers",
            ));
        }
        for (i, a) in self.components.iter().enumerate() {
            if self.components[i + 1..].contains(a) {
                return Err(DomainError::Inconsistent(
                    "BOM line item lists a component more than once",
                ));
            }
        }
        Ok(())
    }
}

// ===================== Phase 3 (increment 3): PCB entities =====================
//
// The PCB layer places the abstract schematic onto a physical substrate: a [`Board`] is
// the rectangular outline (with a layer count) the design must fit within; a [`Placement`]
// binds one [`Component`] to a position, courtyard extent, and [`BoardSide`] on that board.
// Physical values are typed [`PhysicalQuantity`]s (P9), compared via `si_magnitude()` so
// DRC checks (out-of-bounds, courtyard overlap) are dimensionally unambiguous. Referential
// integrity (component exists, board precedes placement) is re-checked at the capability
// seam (P3). See `docs/engineering/pcb-model.md`.

/// Which copper side of the [`Board`] a [`Placement`] sits on. Two courtyards only collide
/// when they share a side, so this drives the courtyard-overlap DRC rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BoardSide {
    Top,
    Bottom,
}

/// The role a copper [`Layer`] plays in the stack: a `Signal` layer carries routed tracks; a
/// `Plane` layer is a solid copper pour (a power/ground reference). The role drives impedance
/// and return-path reasoning (see `engineering-science/pcb/ground-plane.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayerRole {
    Signal,
    Plane,
}

/// One copper layer plus the dielectric beneath it in a [`LayerStack`]. `copper_thickness`
/// and `dielectric_height` are Length [`PhysicalQuantity`]s (P9, e.g. 35µm = 1oz copper on a
/// 1.6mm FR-4 core); `dielectric_er` (ε_r, ≥ 1.0) and `loss_tangent` (tan δ, ≥ 0) are
/// dimensionless ratios — modelled as plain f64 like the existing confidence/reliability
/// fields, not a new [`eak_units::Dimension`]. Carries `PhysicalQuantity` + f64, so `Layer`
/// is not `Eq` (exactly like [`Board`]/[`Track`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub role: LayerRole,
    pub copper_thickness: PhysicalQuantity,
    pub dielectric_height: PhysicalQuantity,
    pub dielectric_er: f64,
    pub loss_tangent: f64,
}

/// The board's copper/dielectric build-up, ordered top→bottom. Replaces a bare layer count so
/// there is a single source of truth (no count/stack drift) and impedance/return-path
/// reasoning has real material data to work from. See `engineering-science/pcb/stackup.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerStack {
    pub layers: Vec<Layer>,
}

impl LayerStack {
    /// The single canonical default: a 2-layer 1.6mm FR-4 board — a top `Signal` layer and a
    /// bottom `Plane` (ground reference), both 35µm (1oz) copper on the same FR-4 dielectric
    /// (ε_r 4.5, tan δ 0.02). A deterministic constant (P4): it is never sized from
    /// requirements or reasoning. Replaces every former `layers: 2`.
    pub fn standard_two_layer() -> Self {
        let copper_thickness = PhysicalQuantity::new(0.035, Unit::Millimetre); // 35µm = 1oz
        let dielectric_height = PhysicalQuantity::new(1.6, Unit::Millimetre); // FR-4 core
        let dielectric_er = 4.5;
        let loss_tangent = 0.02;
        Self {
            layers: vec![
                Layer {
                    role: LayerRole::Signal,
                    copper_thickness,
                    dielectric_height,
                    dielectric_er,
                    loss_tangent,
                },
                Layer {
                    role: LayerRole::Plane,
                    copper_thickness,
                    dielectric_height,
                    dielectric_er,
                    loss_tangent,
                },
            ],
        }
    }

    /// Domain invariants: the stack has at least one layer; every layer has positive, finite
    /// copper thickness and dielectric height (compared via `si_magnitude()`, P9); a finite
    /// ε_r ≥ 1.0; and a finite tan δ ≥ 0.0.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.layers.is_empty() {
            return Err(DomainError::Inconsistent(
                "board layer stack must have at least one layer",
            ));
        }
        for layer in &self.layers {
            if !layer.copper_thickness.si_magnitude().is_finite()
                || layer.copper_thickness.si_magnitude() <= 0.0
                || !layer.dielectric_height.si_magnitude().is_finite()
                || layer.dielectric_height.si_magnitude() <= 0.0
            {
                return Err(DomainError::Inconsistent(
                    "layer copper thickness and dielectric height must be positive and finite",
                ));
            }
            if !layer.dielectric_er.is_finite() || layer.dielectric_er < 1.0 {
                return Err(DomainError::Inconsistent(
                    "layer dielectric relative permittivity must be finite and at least 1.0",
                ));
            }
            if !layer.loss_tangent.is_finite() || layer.loss_tangent < 0.0 {
                return Err(DomainError::Inconsistent(
                    "layer loss tangent must be finite and non-negative",
                ));
            }
        }
        Ok(())
    }
}

/// The physical board outline the design must fit within: a rectangle of `width` x `height`
/// with a typed [`LayerStack`] build-up. Dimensions are typed [`PhysicalQuantity`]s (P9), so
/// placement DRC stays dimensionally unambiguous; hence `Board` is not `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub id: EntityId,
    pub width: PhysicalQuantity,
    pub height: PhysicalQuantity,
    pub stack: LayerStack,
}

impl Board {
    /// The number of copper layers in the stack — a convenience accessor for any count reader
    /// (the [`LayerStack`] is the single source of truth).
    pub fn layers(&self) -> u32 {
        self.stack.layers.len() as u32
    }

    /// Domain invariants: the outline has positive dimensions and a well-formed layer stack.
    /// Dimensions are compared via `si_magnitude()` so the check is unit-independent (P9).
    pub fn validate(&self) -> Result<(), DomainError> {
        if !self.width.si_magnitude().is_finite()
            || !self.height.si_magnitude().is_finite()
            || self.width.si_magnitude() <= 0.0
            || self.height.si_magnitude() <= 0.0
        {
            return Err(DomainError::Inconsistent(
                "board dimensions must be positive and finite",
            ));
        }
        self.stack.validate()?;
        Ok(())
    }
}

/// The placement of one [`Component`] on a [`Board`]: an origin (`x`, `y`), a courtyard
/// extent (`width` x `height`), and the [`BoardSide`] it occupies. Positions and extents are
/// typed [`PhysicalQuantity`]s (P9), so `Placement` is not `Eq`. Component-link and
/// board-precedence integrity are re-checked at the capability seam (P3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub id: EntityId,
    pub component: EntityId,
    pub x: PhysicalQuantity,
    pub y: PhysicalQuantity,
    pub width: PhysicalQuantity,
    pub height: PhysicalQuantity,
    pub side: BoardSide,
}

impl Placement {
    /// Domain invariant: the courtyard has positive extent — a zero-area footprint cannot be
    /// checked for overlap or fit. Extents are compared via `si_magnitude()` (P9).
    pub fn validate(&self) -> Result<(), DomainError> {
        if !self.width.si_magnitude().is_finite()
            || !self.height.si_magnitude().is_finite()
            || self.width.si_magnitude() <= 0.0
            || self.height.si_magnitude() <= 0.0
        {
            return Err(DomainError::Inconsistent(
                "placement courtyard must be positive and finite",
            ));
        }
        Ok(())
    }
}

// =================== Phase 3 (increment 4): routing entities ===================
//
// The routing layer realizes the abstract schematic [`Net`]s physically as copper: a
// [`Track`] binds one [`Net`] to a copper segment of a given `width` on one [`BoardSide`]
// layer, running from (`x1`,`y1`) to (`x2`,`y2`). One track realizes one net
// (net-realization completeness — the routing invariant), so a track is always traceable
// back through its net to the schematic and on to intent (P3). Physical values are typed
// [`PhysicalQuantity`]s (P9), compared via `si_magnitude()` so trace-width DRC stays
// dimensionally unambiguous. Net-link and net-existence integrity are re-checked at the
// capability seam (P3). See `docs/state-machines/routing-planning.md`.

/// A copper realization of one [`Net`]: a trace of `width` on one [`BoardSide`] layer,
/// running from (`x1`,`y1`) to (`x2`,`y2`). Positions and width are typed
/// [`PhysicalQuantity`]s (P9), so `Track` is not `Eq`. Net-link integrity is re-checked at
/// the capability seam (P3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: EntityId,
    /// The net this track realizes.
    pub net: EntityId,
    pub layer: BoardSide,
    /// The copper width of the trace (P9).
    pub width: PhysicalQuantity,
    pub x1: PhysicalQuantity,
    pub y1: PhysicalQuantity,
    pub x2: PhysicalQuantity,
    pub y2: PhysicalQuantity,
}

impl Track {
    /// Domain invariants: a trace has a positive, finite copper width — a zero/negative-width
    /// trace carries no copper and cannot be DRC-checked — and finite endpoints. Width is
    /// compared via `si_magnitude()` so the check is unit-independent (P9). Net-link existence
    /// is re-checked at the capability seam (P3).
    pub fn validate(&self) -> Result<(), DomainError> {
        if !self.width.si_magnitude().is_finite() || self.width.si_magnitude() <= 0.0 {
            return Err(DomainError::Inconsistent(
                "track width must be positive and finite",
            ));
        }
        if !self.x1.si_magnitude().is_finite()
            || !self.y1.si_magnitude().is_finite()
            || !self.x2.si_magnitude().is_finite()
            || !self.y2.si_magnitude().is_finite()
        {
            return Err(DomainError::Inconsistent("track endpoints must be finite"));
        }
        Ok(())
    }
}

/// A violated domain invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyStatement,
    /// A named, required text field was blank (carries the field's human label, so the
    /// rejection message is accurate for whichever entity raised it).
    EmptyField(&'static str),
    /// An entity's fields are internally inconsistent (carries a human explanation).
    Inconsistent(&'static str),
    AcceptedRequirementNeedsCriterion,
    AcceptedRequirementNeedsSource,
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DomainError::EmptyStatement => write!(f, "requirement statement is empty"),
            DomainError::EmptyField(field) => write!(f, "{field} must not be empty"),
            DomainError::Inconsistent(msg) => write!(f, "{msg}"),
            DomainError::AcceptedRequirementNeedsCriterion => {
                write!(f, "accepted requirement lacks an acceptance criterion")
            }
            DomainError::AcceptedRequirementNeedsSource => {
                write!(f, "accepted requirement lacks a source")
            }
        }
    }
}
impl std::error::Error for DomainError {}

impl Requirement {
    pub fn is_testable(&self) -> bool {
        !self.acceptance_criterion.trim().is_empty()
    }

    /// Domain invariants (engineering-domain-model Requirement invariant; requirement-ir
    /// invariant 2): an *accepted* Requirement is testable and rooted in a source.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.statement.trim().is_empty() {
            return Err(DomainError::EmptyStatement);
        }
        if self.status == RequirementStatus::Accepted {
            if !self.is_testable() {
                return Err(DomainError::AcceptedRequirementNeedsCriterion);
            }
            if self.source.is_null() {
                return Err(DomainError::AcceptedRequirementNeedsSource);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(status: RequirementStatus, crit: &str, source: EntityId) -> Requirement {
        Requirement {
            id: EntityId(1),
            statement: "Operating power shall not exceed 5 W".into(),
            category: RequirementCategory::Electrical,
            priority: Priority::High,
            acceptance_criterion: crit.into(),
            status,
            source,
            targets: vec![],
        }
    }

    #[test]
    fn accepted_requirement_needs_criterion() {
        let r = req(RequirementStatus::Accepted, "", EntityId(2));
        assert_eq!(
            r.validate(),
            Err(DomainError::AcceptedRequirementNeedsCriterion)
        );
    }

    #[test]
    fn accepted_requirement_needs_source() {
        let r = req(
            RequirementStatus::Accepted,
            "measured power < 5 W",
            EntityId::NULL,
        );
        assert_eq!(
            r.validate(),
            Err(DomainError::AcceptedRequirementNeedsSource)
        );
    }

    #[test]
    fn well_formed_accepted_requirement_validates() {
        let r = req(
            RequirementStatus::Accepted,
            "measured power < 5 W",
            EntityId(2),
        );
        assert!(r.validate().is_ok());
        assert!(r.is_testable());
    }

    #[test]
    fn proposed_requirement_may_lack_criterion() {
        let r = req(RequirementStatus::Proposed, "", EntityId(2));
        assert!(r.validate().is_ok());
    }

    #[test]
    fn entity_id_null_is_reserved() {
        assert!(EntityId::NULL.is_null());
        assert!(!EntityId(1).is_null());
    }

    #[test]
    fn constraint_rejects_empty_statement() {
        let c = Constraint {
            id: EntityId(1),
            statement: "   ".into(),
            subject_requirement: EntityId(2),
            kind: ConstraintKind::Max,
            bound: PhysicalQuantity::new(5.0, eak_units::Unit::Watt),
            source: EntityId(2),
            status: ConstraintStatus::Active,
        };
        assert_eq!(c.validate(), Err(DomainError::EmptyStatement));
    }

    #[test]
    fn well_formed_constraint_validates_and_is_active() {
        let c = Constraint {
            id: EntityId(1),
            statement: "power <= 5 W".into(),
            subject_requirement: EntityId(2),
            kind: ConstraintKind::Max,
            bound: PhysicalQuantity::new(5.0, eak_units::Unit::Watt),
            source: EntityId(2),
            status: ConstraintStatus::Active,
        };
        assert!(c.validate().is_ok());
        assert!(c.is_active());
    }

    #[test]
    fn only_open_error_violations_block() {
        let mut v = Violation {
            id: EntityId(1),
            rule: "constraint-consistency".into(),
            severity: ViolationSeverity::Error,
            subjects: vec![EntityId(2), EntityId(3)],
            message: "contradictory bounds".into(),
            status: ViolationStatus::Open,
        };
        assert!(v.is_blocking());
        v.status = ViolationStatus::Waived;
        assert!(!v.is_blocking());
        v.status = ViolationStatus::Open;
        v.severity = ViolationSeverity::Warning;
        assert!(!v.is_blocking());
    }

    #[test]
    fn functional_block_rejects_blank_name() {
        let b = FunctionalBlock {
            id: EntityId(1),
            name: "   ".into(),
            function: "5 V rail".into(),
            requirements: vec![EntityId(2)],
        };
        assert_eq!(
            b.validate(),
            Err(DomainError::EmptyField("functional block name"))
        );
    }

    #[test]
    fn well_formed_functional_block_validates() {
        let b = FunctionalBlock {
            id: EntityId(1),
            name: "Power Supply".into(),
            function: "step 12 V down to 5 V".into(),
            requirements: vec![EntityId(2)],
        };
        assert!(b.validate().is_ok());
    }

    #[test]
    fn component_rejects_blank_refdes() {
        let c = Component {
            id: EntityId(1),
            refdes: "  ".into(),
            class: ComponentClass::Regulator,
            value: None,
            from_block: EntityId(2),
        };
        assert_eq!(
            c.validate(),
            Err(DomainError::EmptyField("component reference designator"))
        );
    }

    #[test]
    fn well_formed_component_validates() {
        let c = Component {
            id: EntityId(1),
            refdes: "U1".into(),
            class: ComponentClass::Resistor,
            value: Some(PhysicalQuantity::new(10_000.0, eak_units::Unit::Ohm)),
            from_block: EntityId(2),
        };
        assert!(c.validate().is_ok());
    }

    #[test]
    fn net_rejects_blank_name() {
        let n = Net {
            id: EntityId(1),
            name: "".into(),
            class: NetClass::Power,
            members: vec![EntityId(2), EntityId(3)],
        };
        assert_eq!(n.validate(), Err(DomainError::EmptyField("net name")));
    }

    #[test]
    fn well_formed_net_validates() {
        let n = Net {
            id: EntityId(1),
            name: "+5V".into(),
            class: NetClass::Power,
            members: vec![EntityId(2), EntityId(3)],
        };
        assert!(n.validate().is_ok());
    }

    #[test]
    fn part_rejects_blank_mpn() {
        let p = Part {
            id: EntityId(1),
            mpn: "   ".into(),
            manufacturer: "Texas Instruments".into(),
            lifecycle: PartLifecycle::Active,
            datasheet: "https://ti.com/lm1117".into(),
        };
        assert_eq!(
            p.validate(),
            Err(DomainError::EmptyField("manufacturer part number"))
        );
    }

    #[test]
    fn well_formed_part_validates() {
        let p = Part {
            id: EntityId(1),
            mpn: "LM1117-3.3".into(),
            manufacturer: "Texas Instruments".into(),
            lifecycle: PartLifecycle::Eol,
            datasheet: "https://ti.com/lm1117".into(),
        };
        assert!(p.validate().is_ok());
    }

    fn mm(v: f64) -> PhysicalQuantity {
        PhysicalQuantity::new(v, eak_units::Unit::Millimetre)
    }

    #[test]
    fn board_rejects_non_positive_dimensions() {
        let zero_width = Board {
            id: EntityId(1),
            width: mm(0.0),
            height: mm(50.0),
            stack: LayerStack::standard_two_layer(),
        };
        assert_eq!(
            zero_width.validate(),
            Err(DomainError::Inconsistent(
                "board dimensions must be positive and finite"
            ))
        );
        let negative_height = Board {
            id: EntityId(1),
            width: mm(50.0),
            height: mm(-1.0),
            stack: LayerStack::standard_two_layer(),
        };
        assert_eq!(
            negative_height.validate(),
            Err(DomainError::Inconsistent(
                "board dimensions must be positive and finite"
            ))
        );
    }

    #[test]
    fn board_rejects_empty_stack() {
        let b = Board {
            id: EntityId(1),
            width: mm(50.0),
            height: mm(50.0),
            stack: LayerStack { layers: vec![] },
        };
        assert_eq!(
            b.validate(),
            Err(DomainError::Inconsistent(
                "board layer stack must have at least one layer"
            ))
        );
    }

    #[test]
    fn well_formed_board_validates() {
        let b = Board {
            id: EntityId(1),
            width: mm(50.0),
            height: mm(40.0),
            stack: LayerStack::standard_two_layer(),
        };
        assert!(b.validate().is_ok());
        // The convenience accessor reflects the stack's layer count.
        assert_eq!(b.layers(), 2);
    }

    #[test]
    fn standard_two_layer_stack_validates() {
        let stack = LayerStack::standard_two_layer();
        assert!(stack.validate().is_ok());
        assert_eq!(stack.layers.len(), 2);
        assert_eq!(stack.layers[0].role, LayerRole::Signal);
        assert_eq!(stack.layers[1].role, LayerRole::Plane);
    }

    #[test]
    fn layer_stack_rejects_empty() {
        let stack = LayerStack { layers: vec![] };
        assert_eq!(
            stack.validate(),
            Err(DomainError::Inconsistent(
                "board layer stack must have at least one layer"
            ))
        );
    }

    fn good_layer() -> Layer {
        Layer {
            role: LayerRole::Signal,
            copper_thickness: mm(0.035),
            dielectric_height: mm(1.6),
            dielectric_er: 4.5,
            loss_tangent: 0.02,
        }
    }

    #[test]
    fn layer_stack_rejects_non_positive_copper_thickness() {
        let stack = LayerStack {
            layers: vec![Layer {
                copper_thickness: mm(0.0),
                ..good_layer()
            }],
        };
        assert_eq!(
            stack.validate(),
            Err(DomainError::Inconsistent(
                "layer copper thickness and dielectric height must be positive and finite"
            ))
        );
    }

    #[test]
    fn layer_stack_rejects_non_positive_dielectric_height() {
        let stack = LayerStack {
            layers: vec![Layer {
                dielectric_height: mm(-1.0),
                ..good_layer()
            }],
        };
        assert_eq!(
            stack.validate(),
            Err(DomainError::Inconsistent(
                "layer copper thickness and dielectric height must be positive and finite"
            ))
        );
    }

    #[test]
    fn layer_stack_rejects_permittivity_below_one() {
        let stack = LayerStack {
            layers: vec![Layer {
                dielectric_er: 0.5,
                ..good_layer()
            }],
        };
        assert_eq!(
            stack.validate(),
            Err(DomainError::Inconsistent(
                "layer dielectric relative permittivity must be finite and at least 1.0"
            ))
        );
    }

    #[test]
    fn layer_stack_rejects_negative_loss_tangent() {
        let stack = LayerStack {
            layers: vec![Layer {
                loss_tangent: -0.01,
                ..good_layer()
            }],
        };
        assert_eq!(
            stack.validate(),
            Err(DomainError::Inconsistent(
                "layer loss tangent must be finite and non-negative"
            ))
        );
    }

    #[test]
    fn placement_rejects_non_positive_courtyard() {
        let zero_width = Placement {
            id: EntityId(1),
            component: EntityId(2),
            x: mm(1.0),
            y: mm(1.0),
            width: mm(0.0),
            height: mm(5.0),
            side: BoardSide::Top,
        };
        assert_eq!(
            zero_width.validate(),
            Err(DomainError::Inconsistent(
                "placement courtyard must be positive and finite"
            ))
        );
        let negative_height = Placement {
            id: EntityId(1),
            component: EntityId(2),
            x: mm(1.0),
            y: mm(1.0),
            width: mm(5.0),
            height: mm(-2.0),
            side: BoardSide::Bottom,
        };
        assert_eq!(
            negative_height.validate(),
            Err(DomainError::Inconsistent(
                "placement courtyard must be positive and finite"
            ))
        );
    }

    #[test]
    fn well_formed_placement_validates() {
        let p = Placement {
            id: EntityId(1),
            component: EntityId(2),
            x: mm(10.0),
            y: mm(10.0),
            width: mm(5.0),
            height: mm(5.0),
            side: BoardSide::Top,
        };
        assert!(p.validate().is_ok());
    }

    fn track(width: f64) -> Track {
        Track {
            id: EntityId(1),
            net: EntityId(2),
            layer: BoardSide::Top,
            width: mm(width),
            x1: mm(1.0),
            y1: mm(1.0),
            x2: mm(9.0),
            y2: mm(1.0),
        }
    }

    #[test]
    fn track_rejects_non_positive_width() {
        assert_eq!(
            track(0.0).validate(),
            Err(DomainError::Inconsistent(
                "track width must be positive and finite"
            ))
        );
        assert_eq!(
            track(-0.2).validate(),
            Err(DomainError::Inconsistent(
                "track width must be positive and finite"
            ))
        );
    }

    #[test]
    fn track_rejects_non_finite_endpoint() {
        let mut t = track(0.25);
        t.x2 = mm(f64::INFINITY);
        assert_eq!(
            t.validate(),
            Err(DomainError::Inconsistent("track endpoints must be finite"))
        );
    }

    #[test]
    fn well_formed_track_validates() {
        assert!(track(0.25).validate().is_ok());
    }
}
