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

/// A violated domain invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyStatement,
    AcceptedRequirementNeedsCriterion,
    AcceptedRequirementNeedsSource,
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            DomainError::EmptyStatement => "requirement statement is empty",
            DomainError::AcceptedRequirementNeedsCriterion => {
                "accepted requirement lacks an acceptance criterion"
            }
            DomainError::AcceptedRequirementNeedsSource => "accepted requirement lacks a source",
        };
        write!(f, "{msg}")
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
}
