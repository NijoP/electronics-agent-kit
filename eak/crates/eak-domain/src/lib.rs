//! Engineering domain model — the Phase-1 entity subset (Entities ring).
//!
//! Phase 1 (Requirement Planning) needs exactly five entities plus one first-class
//! relationship: [`DesignIntent`], [`Requirement`], [`Decision`], [`Evidence`], and
//! [`ProvenanceLink`]. Downstream entities (Component, Net, Constraint, ...) are NOT
//! modelled in Phase 1. See `docs/foundation/engineering-domain-model.md`.

use eak_units::PhysicalQuantity;
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

/// A violated domain invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyStatement,
    /// A named, required text field was blank (carries the field's human label, so the
    /// rejection message is accurate for whichever entity raised it).
    EmptyField(&'static str),
    AcceptedRequirementNeedsCriterion,
    AcceptedRequirementNeedsSource,
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DomainError::EmptyStatement => write!(f, "requirement statement is empty"),
            DomainError::EmptyField(field) => write!(f, "{field} must not be empty"),
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
}
