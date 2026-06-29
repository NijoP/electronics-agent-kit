//! Intermediate Representations — typed projections of canonical state at phase
//! boundaries (P6, ADR-0005). The IR is never a rival source of truth; it is derived.
//!
//! Phase 1 owns the first IR ([`RequirementIr`]); Phase 3 adds the [`EngineeringIr`] and
//! [`SchematicIr`] projections (transformation P1) at the engineering and schematic seams.

use eak_domain::{
    Board, BomLineItem, Component, Constraint, DesignIntent, EntityId, FunctionalBlock, Net, Part,
    Pin, Placement, ProvenanceLink, Requirement, RequirementStatus, Track,
};
use serde::{Deserialize, Serialize};

pub const REQUIREMENT_IR_SCHEMA_VERSION: u32 = 1;
pub const ENGINEERING_IR_SCHEMA_VERSION: u32 = 1;
pub const SCHEMATIC_IR_SCHEMA_VERSION: u32 = 1;
pub const BOM_IR_SCHEMA_VERSION: u32 = 1;
pub const PCB_IR_SCHEMA_VERSION: u32 = 1;
pub const MANUFACTURING_IR_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrError {
    OrphanRequirement(EntityId),
    UntestableAccepted(EntityId),
    BlockWithoutRequirement(EntityId),
    OrphanComponent(EntityId),
    UnknownNetMember(EntityId),
    UnknownPart(EntityId),
    LineItemUnknownComponent(EntityId),
    UncoveredComponent(EntityId),
    NoBoard,
    PlacementUnknownComponent(EntityId),
    UnplacedComponent(EntityId),
    TrackUnknownNet(EntityId),
    UnsourcedPlacement(EntityId),
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
            IrError::UnknownPart(id) => {
                write!(f, "bom line item orders unknown part {}", id.short())
            }
            IrError::LineItemUnknownComponent(id) => {
                write!(f, "bom line item covers unknown component {}", id.short())
            }
            IrError::UncoveredComponent(id) => {
                write!(
                    f,
                    "schematic component {} is not covered by any bom line item",
                    id.short()
                )
            }
            IrError::NoBoard => {
                write!(f, "pcb layout requires a board outline but none exists")
            }
            IrError::PlacementUnknownComponent(id) => {
                write!(f, "placement binds unknown component {}", id.short())
            }
            IrError::UnplacedComponent(id) => {
                write!(
                    f,
                    "schematic component {} is not placed on the board",
                    id.short()
                )
            }
            IrError::TrackUnknownNet(id) => {
                write!(f, "track realizes unknown net {}", id.short())
            }
            IrError::UnsourcedPlacement(id) => {
                write!(
                    f,
                    "placed component {} is not sourced by any bom line item",
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

/// The fourth IR: the bill of materials at the boundary out of BOM Planning — the concrete
/// [`Part`]s, the [`BomLineItem`]s binding them to schematic [`Component`]s, and the components
/// they cover. A projection of canonical state (P6); never authored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BomIr {
    pub schema_version: u32,
    pub schematic_ir_schema_version: u32,
    pub parts: Vec<Part>,
    pub line_items: Vec<BomLineItem>,
    pub components: Vec<Component>,
}

impl BomIr {
    /// Project canonical state into the BOM IR (transformation P1), enforcing procurement
    /// integrity: every line item orders a part that exists (P3), covers only components that
    /// exist upstream in the schematic (P3), and every schematic component is covered by at
    /// least one line item — no component ships unsourced (P13).
    pub fn project(
        schematic: &SchematicIr,
        parts: &[Part],
        line_items: &[BomLineItem],
    ) -> Result<Self, IrError> {
        for item in line_items {
            // invariant: the ordered part is rooted in the part list (P3).
            if !parts.iter().any(|p| p.id == item.part) {
                return Err(IrError::UnknownPart(item.part));
            }
            // invariant: every covered component is a real schematic component (P3).
            for cid in &item.components {
                if !schematic.components.iter().any(|c| c.id == *cid) {
                    return Err(IrError::LineItemUnknownComponent(*cid));
                }
            }
        }
        // invariant: every schematic component is covered by >=1 line item (P13).
        for c in &schematic.components {
            if !line_items
                .iter()
                .any(|item| item.components.contains(&c.id))
            {
                return Err(IrError::UncoveredComponent(c.id));
            }
        }
        Ok(Self {
            schema_version: BOM_IR_SCHEMA_VERSION,
            schematic_ir_schema_version: schematic.schema_version,
            parts: parts.to_vec(),
            line_items: line_items.to_vec(),
            components: schematic.components.clone(),
        })
    }
}

/// The fifth IR: the PCB layout at the boundary out of Component Placement, **enriched** by
/// Routing Planning — the physical [`Board`] outline, the [`Placement`]s binding schematic
/// [`Component`]s to positions, the [`Track`]s realizing the nets, and the components they
/// place. A projection of canonical state (P6); never a rival source of truth. Physical values
/// stay typed [`PhysicalQuantity`]s, so DRC downstream is unambiguous. `tracks` is empty at the
/// Component Placement boundary and populated once Routing Planning has run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcbIr {
    pub schema_version: u32,
    pub schematic_ir_schema_version: u32,
    pub board: Board,
    pub placements: Vec<Placement>,
    pub tracks: Vec<Track>,
    pub components: Vec<Component>,
}

impl PcbIr {
    /// Project canonical state into the PCB IR (transformation P1), enforcing layout
    /// integrity: a board outline exists (else [`IrError::NoBoard`]); every placement binds a
    /// component that exists upstream in the schematic (P3); every schematic component is
    /// placed — nothing reaches manufacturing unplaced (P13); and every track realizes a net
    /// that exists upstream in the schematic — no track dangles off a phantom net (P3).
    ///
    /// Net-realization *completeness* (every net realized by a track) is NOT enforced here:
    /// the projection runs at the Component Placement boundary too, before any routing exists,
    /// so requiring a track per net would falsely reject the unrouted-but-valid placed board.
    /// Completeness is the routing phase's own concern; in Phase-3 scope it rests on the
    /// `UnplacedComponent` invariant above (every component placed => every net member routable),
    /// not on a downstream DRC rule (none exists for unrouted nets yet).
    pub fn project(
        schematic: &SchematicIr,
        board: Option<&Board>,
        placements: &[Placement],
        tracks: &[Track],
    ) -> Result<Self, IrError> {
        // invariant: a board outline must precede any layout (P3).
        let board = board.ok_or(IrError::NoBoard)?;
        for placement in placements {
            // invariant: every placement binds a real schematic component (P3).
            if !schematic
                .components
                .iter()
                .any(|c| c.id == placement.component)
            {
                return Err(IrError::PlacementUnknownComponent(placement.component));
            }
        }
        // invariant: every schematic component is placed on the board (P13).
        for c in &schematic.components {
            if !placements.iter().any(|p| p.component == c.id) {
                return Err(IrError::UnplacedComponent(c.id));
            }
        }
        // invariant: every track realizes a real schematic net (P3) — no dangling copper.
        for track in tracks {
            if !schematic.nets.iter().any(|n| n.id == track.net) {
                return Err(IrError::TrackUnknownNet(track.net));
            }
        }
        Ok(Self {
            schema_version: PCB_IR_SCHEMA_VERSION,
            schematic_ir_schema_version: schematic.schema_version,
            board: board.clone(),
            placements: placements.to_vec(),
            tracks: tracks.to_vec(),
            components: schematic.components.clone(),
        })
    }
}

/// One assembly placement directive: a placed [`Component`]'s reference designator bound to the
/// manufacturer part number it is built from. The geometry lives on the matching [`Placement`] in
/// [`ManufacturingIr::placements`] (keyed by `component`); together they are the pick-and-place
/// instruction the assembly house consumes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartAssignment {
    pub component: EntityId,
    pub refdes: String,
    pub mpn: String,
}

/// The sixth and terminal IR: the manufacturing dataset at the boundary out of Manufacturing
/// Generation — the fabrication outline + copper, the assembly pick-and-place ([`Placement`]
/// geometry plus the [`PartAssignment`] refdes->MPN binding), and the procurement BOM. Lowered
/// from the routed [`PcbIr`] and the [`BomIr`] (transformation P1); a projection of canonical
/// state (P6), never authored. Unlike the upstream IRs this one *joins* two seams — the physical
/// layout and the bill of materials — so an assembly directive carries both a position and a
/// part. See `docs/compiler/ir/manufacturing-ir.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManufacturingIr {
    pub schema_version: u32,
    pub pcb_ir_schema_version: u32,
    pub bom_ir_schema_version: u32,
    pub board: Board,
    pub placements: Vec<Placement>,
    pub assignments: Vec<PartAssignment>,
    pub copper: Vec<Track>,
    pub line_items: Vec<BomLineItem>,
}

impl ManufacturingIr {
    /// Project the routed [`PcbIr`] and the [`BomIr`] into the Manufacturing IR (transformation
    /// P1), enforcing output completeness: every placed component must resolve to a bom line item
    /// and an existing part, so every pick-and-place directive carries a real MPN — nothing is
    /// assembled unsourced (P13). Both seams are re-validated at the join (P3): a placement's
    /// component must be a real PCB component, and the line it resolves to must order a known part.
    /// Placements are walked in slice order so the assembly list is deterministic (P4).
    pub fn project(pcb: &PcbIr, bom: &BomIr) -> Result<Self, IrError> {
        let mut assignments = Vec::with_capacity(pcb.placements.len());
        for placement in &pcb.placements {
            // invariant: the placement binds a real PCB component (P3).
            let component = pcb
                .components
                .iter()
                .find(|c| c.id == placement.component)
                .ok_or(IrError::PlacementUnknownComponent(placement.component))?;
            // invariant: the placed component is sourced by some bom line (P13 — no unsourced
            // assembly). BomIr already guarantees coverage of every schematic component; this
            // re-asserts it at the manufacturing join rather than trusting the upstream seam.
            let line = bom
                .line_items
                .iter()
                .find(|li| li.components.contains(&component.id))
                .ok_or(IrError::UnsourcedPlacement(component.id))?;
            // invariant: that line orders a part that exists (P3).
            let part = bom
                .parts
                .iter()
                .find(|p| p.id == line.part)
                .ok_or(IrError::UnknownPart(line.part))?;
            assignments.push(PartAssignment {
                component: component.id,
                refdes: component.refdes.clone(),
                mpn: part.mpn.clone(),
            });
        }
        Ok(Self {
            schema_version: MANUFACTURING_IR_SCHEMA_VERSION,
            pcb_ir_schema_version: pcb.schema_version,
            bom_ir_schema_version: bom.schema_version,
            board: pcb.board.clone(),
            placements: pcb.placements.clone(),
            assignments,
            copper: pcb.tracks.clone(),
            line_items: bom.line_items.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_domain::{
        BoardSide, ComponentClass, LayerStack, NetClass, PartLifecycle, PinElectricalType,
        Priority, RequirementCategory,
    };
    use eak_units::{PhysicalQuantity, Unit};

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

    fn part(id: u128) -> Part {
        Part {
            id: EntityId(id),
            mpn: "LM1117-3.3".into(),
            manufacturer: "Texas Instruments".into(),
            lifecycle: PartLifecycle::Active,
            datasheet: "https://ti.com/lm1117".into(),
        }
    }
    fn line_item(id: u128, part: EntityId, components: Vec<EntityId>) -> BomLineItem {
        BomLineItem {
            id: EntityId(id),
            part,
            components,
            quantity: 1,
        }
    }
    fn schematic_ir() -> SchematicIr {
        let eng = EngineeringIr::project(&req_ir(), &[block(10, vec![EntityId(2)])], &[]).unwrap();
        let c = component(20, EntityId(10));
        SchematicIr::project(&eng, &[c], &[], &[]).unwrap()
    }

    #[test]
    fn bom_project_links_parts_to_components() {
        let sch = schematic_ir();
        let p = part(50);
        let item = line_item(60, EntityId(50), vec![EntityId(20)]);
        let bom = BomIr::project(&sch, &[p], &[item]).unwrap();
        assert_eq!(bom.schema_version, BOM_IR_SCHEMA_VERSION);
        assert_eq!(bom.schematic_ir_schema_version, sch.schema_version);
        assert_eq!(bom.parts.len(), 1);
        assert_eq!(bom.line_items.len(), 1);
        assert_eq!(bom.components.len(), 1);
        assert_eq!(bom.line_items[0].components, vec![EntityId(20)]);
    }

    #[test]
    fn bom_project_rejects_unknown_part() {
        let sch = schematic_ir();
        // line item orders a part (99) that is not in the part list.
        let item = line_item(60, EntityId(99), vec![EntityId(20)]);
        assert!(matches!(
            BomIr::project(&sch, &[part(50)], &[item]),
            Err(IrError::UnknownPart(_))
        ));
    }

    #[test]
    fn bom_project_rejects_line_item_unknown_component() {
        let sch = schematic_ir();
        // line item covers a component (21) that is not in the schematic.
        let item = line_item(60, EntityId(50), vec![EntityId(21)]);
        assert!(matches!(
            BomIr::project(&sch, &[part(50)], &[item]),
            Err(IrError::LineItemUnknownComponent(_))
        ));
    }

    #[test]
    fn bom_project_rejects_uncovered_component() {
        let sch = schematic_ir();
        // no line item covers schematic component 20.
        assert!(matches!(
            BomIr::project(&sch, &[], &[]),
            Err(IrError::UncoveredComponent(_))
        ));
    }

    fn qty(mm: f64) -> PhysicalQuantity {
        PhysicalQuantity::new(mm, Unit::Millimetre)
    }
    fn board(id: u128) -> Board {
        Board {
            id: EntityId(id),
            width: qty(100.0),
            height: qty(80.0),
            stack: LayerStack::standard_two_layer(),
        }
    }
    fn placement(id: u128, component: EntityId) -> Placement {
        Placement {
            id: EntityId(id),
            component,
            x: qty(10.0),
            y: qty(10.0),
            width: qty(5.0),
            height: qty(5.0),
            side: BoardSide::Top,
        }
    }

    #[test]
    fn pcb_project_links_board_and_placements() {
        let sch = schematic_ir();
        let b = board(80);
        let pl = placement(70, EntityId(20));
        let pcb = PcbIr::project(&sch, Some(&b), &[pl], &[]).unwrap();
        assert_eq!(pcb.schema_version, PCB_IR_SCHEMA_VERSION);
        assert_eq!(pcb.schematic_ir_schema_version, sch.schema_version);
        assert_eq!(pcb.board.id, EntityId(80));
        assert_eq!(pcb.placements.len(), 1);
        assert_eq!(pcb.placements[0].component, EntityId(20));
        assert!(pcb.tracks.is_empty());
        assert_eq!(pcb.components.len(), 1);
    }

    #[test]
    fn pcb_project_rejects_no_board() {
        let sch = schematic_ir();
        // a layout without a board outline cannot be projected.
        assert!(matches!(
            PcbIr::project(&sch, None, &[placement(70, EntityId(20))], &[]),
            Err(IrError::NoBoard)
        ));
    }

    #[test]
    fn pcb_project_rejects_placement_unknown_component() {
        let sch = schematic_ir();
        // placement binds a component (99) that is not in the schematic.
        assert!(matches!(
            PcbIr::project(&sch, Some(&board(80)), &[placement(70, EntityId(99))], &[]),
            Err(IrError::PlacementUnknownComponent(_))
        ));
    }

    #[test]
    fn pcb_project_rejects_unplaced_component() {
        let sch = schematic_ir();
        // schematic component 20 is never placed on the board.
        assert!(matches!(
            PcbIr::project(&sch, Some(&board(80)), &[], &[]),
            Err(IrError::UnplacedComponent(_))
        ));
    }

    fn track(id: u128, net: EntityId) -> Track {
        Track {
            id: EntityId(id),
            net,
            layer: BoardSide::Top,
            width: qty(0.25),
            x1: qty(1.0),
            y1: qty(1.0),
            x2: qty(9.0),
            y2: qty(1.0),
        }
    }

    /// A schematic carrying one component, its pin, and a net joining that pin — enough to
    /// route a track against.
    fn routed_schematic() -> SchematicIr {
        let eng = EngineeringIr::project(&req_ir(), &[block(10, vec![EntityId(2)])], &[]).unwrap();
        let c = component(20, EntityId(10));
        let p = pin(30, EntityId(20));
        let n = net(40, vec![EntityId(30)]);
        SchematicIr::project(&eng, &[c], &[p], &[n]).unwrap()
    }

    #[test]
    fn pcb_project_enriches_with_tracks() {
        let sch = routed_schematic();
        let pcb = PcbIr::project(
            &sch,
            Some(&board(80)),
            &[placement(70, EntityId(20))],
            &[track(90, EntityId(40))],
        )
        .unwrap();
        assert_eq!(pcb.tracks.len(), 1);
        assert_eq!(pcb.tracks[0].net, EntityId(40));
    }

    #[test]
    fn pcb_project_rejects_track_unknown_net() {
        let sch = routed_schematic();
        // a track realizing a net (99) that is not in the schematic.
        assert!(matches!(
            PcbIr::project(
                &sch,
                Some(&board(80)),
                &[placement(70, EntityId(20))],
                &[track(90, EntityId(99))]
            ),
            Err(IrError::TrackUnknownNet(_))
        ));
    }

    #[test]
    fn manufacturing_project_joins_layout_and_bom() {
        let sch = routed_schematic();
        let pcb = PcbIr::project(
            &sch,
            Some(&board(80)),
            &[placement(70, EntityId(20))],
            &[track(90, EntityId(40))],
        )
        .unwrap();
        let bom = BomIr::project(
            &sch,
            &[part(50)],
            &[line_item(60, EntityId(50), vec![EntityId(20)])],
        )
        .unwrap();
        let mfg = ManufacturingIr::project(&pcb, &bom).unwrap();
        assert_eq!(mfg.schema_version, MANUFACTURING_IR_SCHEMA_VERSION);
        assert_eq!(mfg.pcb_ir_schema_version, pcb.schema_version);
        assert_eq!(mfg.bom_ir_schema_version, bom.schema_version);
        assert_eq!(mfg.placements.len(), 1);
        assert_eq!(mfg.copper.len(), 1);
        // One assembly directive: placed component 20 -> refdes "U1" -> the regulator MPN.
        assert_eq!(mfg.assignments.len(), 1);
        assert_eq!(mfg.assignments[0].component, EntityId(20));
        assert_eq!(mfg.assignments[0].refdes, "U1");
        assert_eq!(mfg.assignments[0].mpn, "LM1117-3.3");
    }

    #[test]
    fn manufacturing_project_rejects_unsourced_placement() {
        let sch = routed_schematic();
        let pcb =
            PcbIr::project(&sch, Some(&board(80)), &[placement(70, EntityId(20))], &[]).unwrap();
        // A BOM that sources a DIFFERENT component (21), leaving the placed component 20
        // unsourced. Built directly (BomIr::project would reject the uncovered component first)
        // so we exercise ManufacturingIr's own completeness invariant at the join.
        let bom = BomIr {
            schema_version: BOM_IR_SCHEMA_VERSION,
            schematic_ir_schema_version: sch.schema_version,
            parts: vec![part(50)],
            line_items: vec![line_item(60, EntityId(50), vec![EntityId(21)])],
            components: sch.components.clone(),
        };
        assert!(matches!(
            ManufacturingIr::project(&pcb, &bom),
            Err(IrError::UnsourcedPlacement(_))
        ));
    }
}
