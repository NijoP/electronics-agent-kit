//! Component Placement state machine (instance) — DETERMINISTIC in Phase 3.
//!
//! It lays every realized [`Component`](eak_domain::Component) onto the committed
//! [`Board`](eak_domain::Board) outline, minting one [`Placement`](eak_domain::Placement) per
//! component, then projects the [`PcbIr`] (P6). It makes NO reasoning calls: positions follow a
//! fixed left-to-right row (a constant margin + per-index pitch) and courtyard sizes are a pure
//! function of [`ComponentClass`], so a replay is bit-identical (P4). It is idempotent: if
//! placements already exist (e.g. on a DRC-loop back) it skips layout and only (re)projects the
//! IR and re-emits the milestone, committing nothing new. See
//! `docs/state-machines/component-placement.md`.

use eak_compiler::{EngineeringIr, PcbIr, RequirementIr, SchematicIr};
use eak_domain::{BoardSide, ComponentClass, EntityId, Placement, ProvenanceLink, RelationType};
use eak_ports::Event;
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};
use eak_units::{PhysicalQuantity, Unit};

/// Clearance from the board origin to the first courtyard, in millimetres.
const MARGIN_MM: f64 = 2.0;
/// Centre-to-origin pitch between successive courtyards along the row, in millimetres.
const PITCH_MM: f64 = 12.0;

pub struct ComponentPlacementMachine;

impl ComponentPlacementMachine {
    pub fn new() -> Self {
        Self
    }
}
impl Default for ComponentPlacementMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// The square courtyard edge (mm) a component of `class` occupies. Connectors are the largest
/// footprint; actives (regulator/IC) sit in the middle; passives are the smallest.
fn courtyard_mm(class: ComponentClass) -> f64 {
    match class {
        ComponentClass::Connector => 9.0,
        ComponentClass::Regulator | ComponentClass::Ic => 6.0,
        ComponentClass::Resistor | ComponentClass::Capacitor => 3.0,
    }
}

impl Machine for ComponentPlacementMachine {
    fn name(&self) -> &str {
        "ComponentPlacement"
    }

    fn initial(&self) -> String {
        "Idle".into()
    }

    fn step(
        &mut self,
        state: &str,
        ctx: &mut dyn AgentContext,
    ) -> Result<StepResult, MachineError> {
        match state {
            "Idle" => Ok(StepResult::Continue("Placing".into())),

            "Placing" => {
                // Idempotent + recoverable: place only the components not yet placed, each at its
                // stable commit-order index, so a re-entry (DRC loop-back) or a partial prior
                // layout mints exactly the missing placements — positions and ids stay
                // reproducible (P4), and a fully-placed re-entry commits nothing.
                let components = ctx.components();
                let placed: Vec<EntityId> = ctx.placements().iter().map(|p| p.component).collect();
                if components.iter().any(|c| !placed.contains(&c.id)) {
                    // A placement is meaningless without an outline to fit against; Floor
                    // Planning runs first, so a missing board here is a workflow-ordering bug.
                    if ctx.board().is_none() {
                        return Err(MachineError::Internal(
                            "cannot place components before the board outline exists".into(),
                        ));
                    }

                    // Lay components left-to-right in a single row in commit order — a
                    // deterministic pass so the minted placements (and their ids) are
                    // reproducible (P4). Each courtyard is a class-sized square at a fixed pitch.
                    for (i, component) in components.iter().enumerate() {
                        if placed.contains(&component.id) {
                            continue;
                        }
                        let x = MARGIN_MM + (i as f64) * PITCH_MM;
                        let edge = courtyard_mm(component.class);

                        let pid = ctx.fresh_id();
                        let placement = Placement {
                            id: pid,
                            component: component.id,
                            x: PhysicalQuantity::new(x, Unit::Millimetre),
                            y: PhysicalQuantity::new(MARGIN_MM, Unit::Millimetre),
                            width: PhysicalQuantity::new(edge, Unit::Millimetre),
                            height: PhysicalQuantity::new(edge, Unit::Millimetre),
                            side: BoardSide::Top,
                        };
                        // Provenance: the placement traces to the component it positions (P3),
                        // so a DRC violation is link-traceable back through the schematic to
                        // intent: Violation -> Placement -> Component -> Block -> Requirement.
                        let link = ProvenanceLink {
                            id: ctx.fresh_id(),
                            from: pid,
                            to: component.id,
                            relation: RelationType::TracesTo,
                        };
                        ctx.invoke(CapabilityRequest::PlaceComponent {
                            placement,
                            links: vec![link],
                        })
                        .map_err(|e| MachineError::Internal(e.to_string()))?;
                    }
                }

                // Project the PCB IR (P6) and record the boundary milestone. The projection
                // re-asserts layout integrity: an outline exists, every placement binds a real
                // schematic component, and every component is placed (IrError -> Internal).
                let pcb = project_pcb(ctx)?;
                ctx.emit(vec![Event::PcbIrProduced {
                    schema_version: pcb.schema_version,
                    placement_count: pcb.placements.len(),
                }])
                .map_err(|e| MachineError::Internal(e.to_string()))?;
                Ok(StepResult::Done)
            }

            other => Err(MachineError::Internal(format!("unknown state {other}"))),
        }
    }
}

/// Project canonical state into the [`PcbIr`] through the full lowering chain (Requirement IR
/// -> Engineering IR -> Schematic IR -> PCB IR), mapping any `IrError` to an internal machine
/// error. Shared by the placement path and the idempotent re-projection path.
fn project_pcb(ctx: &mut dyn AgentContext) -> Result<PcbIr, MachineError> {
    let intent = ctx
        .design_intent()
        .ok_or_else(|| MachineError::Internal("no intent for lowering".into()))?;
    let reqs = ctx.requirements();
    let links = ctx.provenance_links();
    let blocks = ctx.functional_blocks();
    let constraints = ctx.constraints();
    let components = ctx.components();
    let pins = ctx.pins();
    let nets = ctx.nets();
    let board = ctx.board();
    let placements = ctx.placements();
    let req_ir = RequirementIr::project(&intent, &reqs, &links)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    let eng_ir = EngineeringIr::project(&req_ir, &blocks, &constraints)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    let sch_ir = SchematicIr::project(&eng_ir, &components, &pins, &nets)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    PcbIr::project(&sch_ir, board.as_ref(), &placements)
        .map_err(|e| MachineError::Internal(e.to_string()))
}
