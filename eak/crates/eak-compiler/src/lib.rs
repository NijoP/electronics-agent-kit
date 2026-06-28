//! Intermediate Representations — typed projections of canonical state at phase
//! boundaries (P6, ADR-0005). The IR is never a rival source of truth; it is derived.
//!
//! Phase 1 owns the first IR ([`RequirementIr`]) and the first lowering
//! ([`lower_to_engineering_ir`], transformation P1) — the latter a clearly-marked STUB.

use eak_domain::{DesignIntent, EntityId, ProvenanceLink, Requirement, RequirementStatus};
use serde::{Deserialize, Serialize};

pub const REQUIREMENT_IR_SCHEMA_VERSION: u32 = 1;
pub const ENGINEERING_IR_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrError {
    OrphanRequirement(EntityId),
    UntestableAccepted(EntityId),
}
impl std::fmt::Display for IrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IrError::OrphanRequirement(id) => {
                write!(f, "requirement {} is not rooted in a source", id.short())
            }
            IrError::UntestableAccepted(id) => {
                write!(
                    f,
                    "accepted requirement {} lacks an acceptance criterion",
                    id.short()
                )
            }
        }
    }
}
impl std::error::Error for IrError {}

/// The first IR: the design's intent and requirements at the boundary out of Requirement
/// Planning. A projection of the domain model's intent layer; carries NO engineering
/// content (invariant 5). See `docs/compiler/ir/requirement-ir.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequirementIr {
    pub schema_version: u32,
    pub intent: DesignIntent,
    pub requirements: Vec<Requirement>,
    pub provenance: Vec<ProvenanceLink>,
}

impl RequirementIr {
    /// Project canonical state into the Requirement IR, enforcing its invariants.
    pub fn project(
        intent: &DesignIntent,
        requirements: &[Requirement],
        links: &[ProvenanceLink],
    ) -> Result<Self, IrError> {
        for r in requirements {
            // invariant 1: every requirement is rooted.
            if r.source.is_null() {
                return Err(IrError::OrphanRequirement(r.id));
            }
            // invariant 2: every accepted requirement is testable.
            if r.status == RequirementStatus::Accepted && !r.is_testable() {
                return Err(IrError::UntestableAccepted(r.id));
            }
        }
        Ok(Self {
            schema_version: REQUIREMENT_IR_SCHEMA_VERSION,
            intent: intent.clone(),
            requirements: requirements.to_vec(),
            provenance: links.to_vec(),
        })
    }
}

/// A trivial functional block in the stub Engineering IR — carries the requirement it
/// derives from so traceability survives the lowering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionalBlockStub {
    pub label: String,
    pub from_requirement: EntityId,
}

/// The second IR (STUB form): produced by the Engineering Analysis stub to prove the
/// lowering seam. Real topology/functional-block synthesis is a later phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineeringIr {
    pub schema_version: u32,
    pub requirement_ir_schema_version: u32,
    pub blocks: Vec<FunctionalBlockStub>,
    pub requirements: Vec<Requirement>,
}

/// Transformation P1 (STUB): one functional block per requirement, preserving the
/// requirement id. Proves the Requirement IR -> Engineering IR seam without real analysis.
pub fn lower_to_engineering_ir(req_ir: &RequirementIr) -> EngineeringIr {
    let blocks = req_ir
        .requirements
        .iter()
        .map(|r| FunctionalBlockStub {
            label: format!("block-for-{}", r.id.short()),
            from_requirement: r.id,
        })
        .collect();
    EngineeringIr {
        schema_version: ENGINEERING_IR_SCHEMA_VERSION,
        requirement_ir_schema_version: req_ir.schema_version,
        blocks,
        requirements: req_ir.requirements.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_domain::{Priority, RequirementCategory};

    fn intent() -> DesignIntent {
        DesignIntent {
            id: EntityId(1),
            statement: "x".into(),
            structured_summary: "x".into(),
            source: "engineer".into(),
        }
    }
    fn req(id: u128, status: RequirementStatus, crit: &str, src: EntityId) -> Requirement {
        Requirement {
            id: EntityId(id),
            statement: "s".into(),
            category: RequirementCategory::Functional,
            priority: Priority::Medium,
            acceptance_criterion: crit.into(),
            status,
            source: src,
            targets: vec![],
        }
    }

    #[test]
    fn project_rejects_orphan() {
        let r = req(2, RequirementStatus::Proposed, "", EntityId::NULL);
        assert!(matches!(
            RequirementIr::project(&intent(), &[r], &[]),
            Err(IrError::OrphanRequirement(_))
        ));
    }

    #[test]
    fn project_rejects_untestable_accepted() {
        let r = req(2, RequirementStatus::Accepted, "", EntityId(1));
        assert!(matches!(
            RequirementIr::project(&intent(), &[r], &[]),
            Err(IrError::UntestableAccepted(_))
        ));
    }

    #[test]
    fn lowering_preserves_traceability() {
        let r = req(2, RequirementStatus::Accepted, "crit", EntityId(1));
        let ir = RequirementIr::project(&intent(), std::slice::from_ref(&r), &[]).unwrap();
        let eng = lower_to_engineering_ir(&ir);
        assert_eq!(eng.blocks.len(), 1);
        assert_eq!(eng.blocks[0].from_requirement, r.id);
    }
}
