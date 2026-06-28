//! Intermediate Representations — typed projections of canonical state at phase
//! boundaries (P6, ADR-0005). The IR is never a rival source of truth; it is derived.
//!
//! Phase 1 owns the first IR ([`RequirementIr`]); Phase 3 adds the [`EngineeringIr`] and
//! [`SchematicIr`] projections (transformation P1) at the engineering and schematic seams.

use eak_domain::{
    Component, Constraint, DesignIntent, EntityId, FunctionalBlock, Net, Pin, ProvenanceLink,
    Requirement, RequirementStatus,
};
use serde::{Deserialize, Serialize};

pub const REQUIREMENT_IR_SCHEMA_VERSION: u32 = 1;
pub const ENGINEERING_IR_SCHEMA_VERSION: u32 = 1;
pub const SCHEMATIC_IR_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrError {
    OrphanRequirement(EntityId),
    UntestableAccepted(EntityId),
    BlockWithoutRequirement(EntityId),
    OrphanComponent(EntityId),
    UnknownNetMember(EntityId),
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
            IrError::BlockWithoutRequirement(id) => {
                write!(
                    f,
                    "functional block {} does not trace to a known requirement",
                    id.short()
                )
            }
            IrError::OrphanComponent(id) => {
                write!(
                    f,
                    "component {} is not minted from a known functional block",
                    id.short()
                )
            }
            IrError::UnknownNetMember(id) => {
                write!(f, "net references unknown pin {}", id.short())
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

/// The second IR: the engineering architecture at the boundary out of Engineering Analysis —
/// the [`FunctionalBlock`]s realizing the requirements, the [`Constraint`]s bounding them, and
/// the requirements they trace to. A projection of canonical state (P6); never authored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineeringIr {
    pub schema_version: u32,
    pub requirement_ir_schema_version: u32,
    pub blocks: Vec<FunctionalBlock>,
    pub constraints: Vec<Constraint>,
    pub requirements: Vec<Requirement>,
}

impl EngineeringIr {
    /// Project canonical state into the Engineering IR (transformation P1), enforcing
    /// traceability (P3): every block realizes at least one requirement, and every
    /// requirement a block references must exist upstream in `req_ir` — no dangling trace.
    pub fn project(
        req_ir: &RequirementIr,
        blocks: &[FunctionalBlock],
        constraints: &[Constraint],
    ) -> Result<Self, IrError> {
        for b in blocks {
            // invariant: a block with no requirement realizes nothing traceable (P3).
            if b.requirements.is_empty() {
                return Err(IrError::BlockWithoutRequirement(b.id));
            }
            // invariant: every referenced requirement is rooted in the Requirement IR (P3).
            for rid in &b.requirements {
                if !req_ir.requirements.iter().any(|r| r.id == *rid) {
                    return Err(IrError::BlockWithoutRequirement(b.id));
                }
            }
        }
        Ok(Self {
            schema_version: ENGINEERING_IR_SCHEMA_VERSION,
            requirement_ir_schema_version: req_ir.schema_version,
            blocks: blocks.to_vec(),
            constraints: constraints.to_vec(),
            requirements: req_ir.requirements.clone(),
        })
    }
}

/// The third IR: the schematic at the boundary out of Schematic Planning — the
/// [`Component`]s minted from the blocks, their [`Pin`]s, and the [`Net`]s joining them.
/// A projection of canonical state (P6); never a rival source of truth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchematicIr {
    pub schema_version: u32,
    pub engineering_ir_schema_version: u32,
    pub blocks: Vec<FunctionalBlock>,
    pub components: Vec<Component>,
    pub pins: Vec<Pin>,
    pub nets: Vec<Net>,
}

impl SchematicIr {
    /// Project canonical state into the Schematic IR (transformation P1), enforcing
    /// connectivity integrity: every component is minted from a block that exists upstream
    /// (P3), and every net member names an existing pin — no dangling connectivity (P13).
    pub fn project(
        eng_ir: &EngineeringIr,
        components: &[Component],
        pins: &[Pin],
        nets: &[Net],
    ) -> Result<Self, IrError> {
        for c in components {
            // invariant: a component traces back to a real functional block (P3).
            if !eng_ir.blocks.iter().any(|b| b.id == c.from_block) {
                return Err(IrError::OrphanComponent(c.id));
            }
        }
        for n in nets {
            // invariant: every net member is an existing pin (P13: connectivity is explicit).
            for pid in &n.members {
                if !pins.iter().any(|p| p.id == *pid) {
                    return Err(IrError::UnknownNetMember(*pid));
                }
            }
        }
        Ok(Self {
            schema_version: SCHEMATIC_IR_SCHEMA_VERSION,
            engineering_ir_schema_version: eng_ir.schema_version,
            blocks: eng_ir.blocks.clone(),
            components: components.to_vec(),
            pins: pins.to_vec(),
            nets: nets.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_domain::{ComponentClass, NetClass, PinElectricalType, Priority, RequirementCategory};

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

    fn block(id: u128, reqs: Vec<EntityId>) -> FunctionalBlock {
        FunctionalBlock {
            id: EntityId(id),
            name: "regulation".into(),
            function: "step down 12V to 3V3".into(),
            requirements: reqs,
        }
    }
    fn component(id: u128, from_block: EntityId) -> Component {
        Component {
            id: EntityId(id),
            refdes: "U1".into(),
            class: ComponentClass::Regulator,
            value: None,
            from_block,
        }
    }
    fn pin(id: u128, comp: EntityId) -> Pin {
        Pin {
            id: EntityId(id),
            component: comp,
            designation: "1".into(),
            electrical_type: PinElectricalType::PowerOut,
        }
    }
    fn net(id: u128, members: Vec<EntityId>) -> Net {
        Net {
            id: EntityId(id),
            name: "VCC".into(),
            class: NetClass::Power,
            members,
        }
    }

    fn req_ir() -> RequirementIr {
        let r = req(2, RequirementStatus::Accepted, "crit", EntityId(1));
        RequirementIr::project(&intent(), std::slice::from_ref(&r), &[]).unwrap()
    }

    #[test]
    fn engineering_project_traces_blocks_to_requirements() {
        let ir = req_ir();
        let eng = EngineeringIr::project(&ir, &[block(10, vec![EntityId(2)])], &[]).unwrap();
        assert_eq!(eng.schema_version, ENGINEERING_IR_SCHEMA_VERSION);
        assert_eq!(eng.requirement_ir_schema_version, ir.schema_version);
        assert_eq!(eng.blocks.len(), 1);
        assert_eq!(eng.blocks[0].requirements, vec![EntityId(2)]);
    }

    #[test]
    fn engineering_project_rejects_block_without_requirement() {
        let ir = req_ir();
        // a block realizing nothing.
        assert!(matches!(
            EngineeringIr::project(&ir, &[block(10, vec![])], &[]),
            Err(IrError::BlockWithoutRequirement(_))
        ));
        // a block tracing to a requirement that does not exist upstream.
        assert!(matches!(
            EngineeringIr::project(&ir, &[block(10, vec![EntityId(999)])], &[]),
            Err(IrError::BlockWithoutRequirement(_))
        ));
    }

    #[test]
    fn schematic_project_links_components_and_nets() {
        let eng = EngineeringIr::project(&req_ir(), &[block(10, vec![EntityId(2)])], &[]).unwrap();
        let c = component(20, EntityId(10));
        let p = pin(30, EntityId(20));
        let n = net(40, vec![EntityId(30)]);
        let sch = SchematicIr::project(&eng, &[c], &[p], &[n]).unwrap();
        assert_eq!(sch.schema_version, SCHEMATIC_IR_SCHEMA_VERSION);
        assert_eq!(sch.engineering_ir_schema_version, eng.schema_version);
        assert_eq!(sch.components.len(), 1);
        assert_eq!(sch.nets[0].members, vec![EntityId(30)]);
    }

    #[test]
    fn schematic_project_rejects_orphan_component() {
        let eng = EngineeringIr::project(&req_ir(), &[block(10, vec![EntityId(2)])], &[]).unwrap();
        // component minted from a block that is not in the Engineering IR.
        let c = component(20, EntityId(99));
        assert!(matches!(
            SchematicIr::project(&eng, &[c], &[], &[]),
            Err(IrError::OrphanComponent(_))
        ));
    }

    #[test]
    fn schematic_project_rejects_unknown_net_member() {
        let eng = EngineeringIr::project(&req_ir(), &[block(10, vec![EntityId(2)])], &[]).unwrap();
        let c = component(20, EntityId(10));
        let p = pin(30, EntityId(20));
        // net references a pin id (31) that does not exist.
        let n = net(40, vec![EntityId(31)]);
        assert!(matches!(
            SchematicIr::project(&eng, &[c], &[p], &[n]),
            Err(IrError::UnknownNetMember(_))
        ));
    }
}
