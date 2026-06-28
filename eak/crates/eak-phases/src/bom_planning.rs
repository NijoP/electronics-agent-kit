//! BOM Planning state machine (instance) — DETERMINISTIC in Phase 3.
//!
//! It turns the realized schematic into a procurable bill of materials: every
//! [`Component`](eak_domain::Component) is grouped by [`ComponentClass`], each class is
//! resolved to a concrete [`Part`] through the deterministic [`PartCatalog`], and a
//! [`BomLineItem`] binds that part to the components it covers (P13: nothing ships
//! unsourced). It makes NO reasoning calls — catalog selection is a pure function of the
//! component class, so a replay is bit-identical (P4). It is idempotent: if line items
//! already exist (e.g. on a BOM-loop back) it only (re)projects the [`BomIr`] and re-emits the
//! milestone, committing nothing new. Parts are deduplicated by manufacturer part number so a
//! class shared across components yields a single ordered part. See
//! `docs/state-machines/bom-planning.md`.

use eak_compiler::{BomIr, EngineeringIr, RequirementIr, SchematicIr};
use eak_domain::{BomLineItem, ComponentClass, EntityId, Part, ProvenanceLink, RelationType};
use eak_engines::PartCatalog;
use eak_ports::Event;
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};

pub struct BomPlanningMachine;

impl BomPlanningMachine {
    pub fn new() -> Self {
        Self
    }
}
impl Default for BomPlanningMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for BomPlanningMachine {
    fn name(&self) -> &str {
        "BomPlanning"
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
            "Idle" => Ok(StepResult::Continue("Planning".into())),

            "Planning" => {
                // Idempotent: synthesize the BOM only once. A re-entry (BOM loop-back) skips
                // synthesis and just (re)projects the IR below.
                if ctx.bom_line_items().is_empty() {
                    let catalog = PartCatalog::new();
                    let components = ctx.components();

                    // Group components by class in first-appearance order — a deterministic
                    // pass so the minted line items (and their ids) are reproducible (P4).
                    let mut groups: Vec<(ComponentClass, Vec<EntityId>)> = Vec::new();
                    for component in &components {
                        if let Some(group) = groups
                            .iter_mut()
                            .find(|(class, _)| *class == component.class)
                        {
                            group.1.push(component.id);
                        } else {
                            groups.push((component.class, vec![component.id]));
                        }
                    }

                    // Cache of manufacturer-part-number -> committed part id, seeded with any
                    // already-committed parts, so a part reused across classes (same mpn) is
                    // ordered once (P13).
                    let mut part_ids: Vec<(String, EntityId)> =
                        ctx.parts().iter().map(|p| (p.mpn.clone(), p.id)).collect();

                    for (class, comp_ids) in &groups {
                        let cp = catalog.part_for(*class);

                        // Dedup the part by mpn: reuse an existing one, else mint and commit it.
                        let part_id = if let Some((_, id)) =
                            part_ids.iter().find(|(mpn, _)| mpn.as_str() == cp.mpn)
                        {
                            *id
                        } else {
                            let pid = ctx.fresh_id();
                            let part = Part {
                                id: pid,
                                mpn: cp.mpn.into(),
                                manufacturer: cp.manufacturer.into(),
                                lifecycle: cp.lifecycle,
                                datasheet: cp.datasheet.into(),
                            };
                            ctx.invoke(CapabilityRequest::CreatePart {
                                part,
                                links: vec![],
                            })
                            .map_err(|e| MachineError::Internal(e.to_string()))?;
                            part_ids.push((cp.mpn.to_string(), pid));
                            pid
                        };

                        // The line binds the part to every component of this class. Provenance:
                        // the line traces to the part it orders AND to each component it covers,
                        // so a BOM violation is fully link-traceable back to intent (P3):
                        // Violation -> line -> {part ; component -> block -> requirement -> intent}.
                        let item_id = ctx.fresh_id();
                        let item = BomLineItem {
                            id: item_id,
                            part: part_id,
                            components: comp_ids.clone(),
                            quantity: comp_ids.len() as u32,
                        };
                        let mut links = vec![ProvenanceLink {
                            id: ctx.fresh_id(),
                            from: item_id,
                            to: part_id,
                            relation: RelationType::TracesTo,
                        }];
                        for cid in comp_ids {
                            links.push(ProvenanceLink {
                                id: ctx.fresh_id(),
                                from: item_id,
                                to: *cid,
                                relation: RelationType::TracesTo,
                            });
                        }
                        ctx.invoke(CapabilityRequest::CreateBomLineItem { item, links })
                            .map_err(|e| MachineError::Internal(e.to_string()))?;
                    }
                }

                // Project the BOM IR (P6) and record the boundary milestone. The projection
                // re-asserts procurement integrity: every line orders a known part, covers only
                // real components, and every component is covered (IrError -> Internal).
                let bom = project_bom(ctx)?;
                ctx.emit(vec![Event::BomIrProduced {
                    schema_version: bom.schema_version,
                    line_item_count: bom.line_items.len(),
                }])
                .map_err(|e| MachineError::Internal(e.to_string()))?;
                Ok(StepResult::Done)
            }

            other => Err(MachineError::Internal(format!("unknown state {other}"))),
        }
    }
}

/// Project canonical state into the [`BomIr`] through the full lowering chain (Requirement IR
/// -> Engineering IR -> Schematic IR -> BOM IR), mapping any `IrError` to an internal machine
/// error. Shared by the synthesis path and the idempotent re-projection path.
fn project_bom(ctx: &mut dyn AgentContext) -> Result<BomIr, MachineError> {
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
    let parts = ctx.parts();
    let line_items = ctx.bom_line_items();
    let req_ir = RequirementIr::project(&intent, &reqs, &links)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    let eng_ir = EngineeringIr::project(&req_ir, &blocks, &constraints)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    let sch_ir = SchematicIr::project(&eng_ir, &components, &pins, &nets)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    BomIr::project(&sch_ir, &parts, &line_items).map_err(|e| MachineError::Internal(e.to_string()))
}
