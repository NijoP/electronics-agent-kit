//! Routing Planning state machine (instance) — DETERMINISTIC in Phase 3.
//!
//! It realizes each committed [`Net`](eak_domain::Net) physically as one
//! [`Track`](eak_domain::Track) of copper on the board, then **enriches** the [`PcbIr`] (P6)
//! with those tracks. It makes NO reasoning calls: a net's track spans its member components'
//! placements (a pure function of the committed layout) and carries a fixed default trace
//! width, so a replay is bit-identical (P4). It is idempotent: it routes only the nets not yet
//! realized by a track, so a re-entry (a DRC loop-back) mints exactly the missing tracks — ids
//! and geometry stay reproducible — and a fully-routed re-entry commits nothing new. See
//! `docs/state-machines/routing-planning.md`.

use eak_compiler::{EngineeringIr, PcbIr, RequirementIr, SchematicIr};
use eak_domain::{BoardSide, EntityId, NetClass, Placement, ProvenanceLink, RelationType, Track};
use eak_ports::Event;
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};
use eak_units::{PhysicalQuantity, Unit};

/// Per-net-class default copper widths, in millimetres. Power and ground rails carry more current,
/// so they default wider than signal traces. Phase-3 scope: a fixed per-class policy (not yet
/// current-derived or per-net override) — but the resolved width is recorded into the `Track` (and
/// thus the event stream), so a replay re-folds the identical width and never recomputes it (P4).
/// Routing stays oblivious to the DRC fabrication floor: a class default finer than the floor is
/// still flagged downstream by the trace-width rule and looped back, exactly as before.
const POWER_TRACE_WIDTH_MM: f64 = 0.50;
const GROUND_TRACE_WIDTH_MM: f64 = 0.50;
const SIGNAL_TRACE_WIDTH_MM: f64 = 0.25;

/// The default copper width for a net of the given class, in millimetres. An exhaustive match (no
/// wildcard) so adding a [`NetClass`] variant is a compile error here — a deliberate guard that a
/// new class must make a width choice rather than silently inherit one.
fn class_width_mm(class: NetClass) -> f64 {
    match class {
        NetClass::Power => POWER_TRACE_WIDTH_MM,
        NetClass::Ground => GROUND_TRACE_WIDTH_MM,
        NetClass::Signal => SIGNAL_TRACE_WIDTH_MM,
    }
}

pub struct RoutingPlanningMachine;

impl RoutingPlanningMachine {
    pub fn new() -> Self {
        Self
    }
}
impl Default for RoutingPlanningMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for RoutingPlanningMachine {
    fn name(&self) -> &str {
        "RoutingPlanning"
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
            "Idle" => Ok(StepResult::Continue("Routing".into())),

            "Routing" => {
                // A track is copper on a substrate; Floor Planning and Placement run first, so a
                // missing board here is a workflow-ordering bug.
                if ctx.board().is_none() {
                    return Err(MachineError::Internal(
                        "cannot route nets before the board outline exists".into(),
                    ));
                }

                let nets = ctx.nets();
                let pins = ctx.pins();
                let placements = ctx.placements();
                let routed: Vec<EntityId> = ctx.tracks().iter().map(|t| t.net).collect();

                // Route each not-yet-realized net in commit order — a deterministic pass so the
                // minted tracks (and their ids) are reproducible (P4). A net's track spans the
                // placements of the components its member pins belong to.
                for net in &nets {
                    if routed.contains(&net.id) {
                        continue;
                    }

                    // Resolve the net's member pins -> components -> placements, in placement
                    // order, de-duplicated (several pins may share one component/placement). Both
                    // skips below are dead code under the upstream seam invariants: the net seam
                    // (handle_create_net) rejects unknown member pins, and PcbIr::project's
                    // UnplacedComponent invariant guarantees every component is placed before
                    // routing runs — so every member pin resolves to a placed component. They are
                    // kept as defensive guards, not as a path expected to fire.
                    let mut members: Vec<&Placement> = Vec::new();
                    for pin_id in &net.members {
                        let Some(pin) = pins.iter().find(|p| p.id == *pin_id) else {
                            continue;
                        };
                        if let Some(placement) =
                            placements.iter().find(|pl| pl.component == pin.component)
                        {
                            if !members.iter().any(|m| m.id == placement.id) {
                                members.push(placement);
                            }
                        }
                    }
                    // By the same invariants this is unreachable in a well-ordered workflow. If a
                    // net ever did resolve to no placement it is left unrouted here — and that is
                    // no longer a silent gap: `DrcUnroutedNetRule` raises an `drc-unrouted-net`
                    // Error for any net not realized by a track, so net-realization completeness is
                    // enforced downstream by DRC rather than resting solely on the upstream
                    // UnplacedComponent guarantee.
                    if members.is_empty() {
                        continue;
                    }

                    // Order endpoints by centre x (then id) so the segment is deterministic
                    // regardless of net-member order; the track runs from the first member's
                    // centroid to the last's. `total_cmp` is a total order (no NaN masking — a
                    // non-finite coordinate would be rejected at the track seam, not silently
                    // sorted as equal).
                    members.sort_by(|a, b| {
                        center_mm(a.x, a.width)
                            .total_cmp(&center_mm(b.x, b.width))
                            .then(a.id.cmp(&b.id))
                    });
                    let first = members.first().expect("members is non-empty");
                    let last = members.last().expect("members is non-empty");

                    let tid = ctx.fresh_id();
                    let track = Track {
                        id: tid,
                        net: net.id,
                        layer: BoardSide::Top,
                        width: PhysicalQuantity::new(class_width_mm(net.class), Unit::Millimetre),
                        x1: PhysicalQuantity::new(
                            center_mm(first.x, first.width),
                            Unit::Millimetre,
                        ),
                        y1: PhysicalQuantity::new(
                            center_mm(first.y, first.height),
                            Unit::Millimetre,
                        ),
                        x2: PhysicalQuantity::new(center_mm(last.x, last.width), Unit::Millimetre),
                        y2: PhysicalQuantity::new(center_mm(last.y, last.height), Unit::Millimetre),
                    };
                    // Provenance: the track traces to the net it realizes (P3), so a trace-width
                    // DRC violation is link-traceable back through the net to intent:
                    // Violation -> Track -> Net -> ... -> Requirement -> Intent.
                    let link = ProvenanceLink {
                        id: ctx.fresh_id(),
                        from: tid,
                        to: net.id,
                        relation: RelationType::TracesTo,
                    };
                    ctx.invoke(CapabilityRequest::RouteNet {
                        track,
                        links: vec![link],
                    })
                    .map_err(|e| MachineError::Internal(e.to_string()))?;
                }

                // Re-project the PCB IR enriched with the tracks (P6) and record the boundary
                // milestone. The projection re-asserts that every track realizes a real net.
                let pcb = project_pcb(ctx)?;
                ctx.emit(vec![Event::PcbIrEnriched {
                    schema_version: pcb.schema_version,
                    track_count: pcb.tracks.len(),
                }])
                .map_err(|e| MachineError::Internal(e.to_string()))?;
                Ok(StepResult::Done)
            }

            other => Err(MachineError::Internal(format!("unknown state {other}"))),
        }
    }
}

/// The centre of a placement along one axis, in millimetres: origin + half the courtyard
/// extent. Computed on the SI axis then expressed in millimetres so it is independent of the
/// unit the placement happens to carry (P9).
fn center_mm(origin: PhysicalQuantity, extent: PhysicalQuantity) -> f64 {
    (origin.si_magnitude() + extent.si_magnitude() / 2.0) * 1000.0
}

/// Project canonical state into the [`PcbIr`] enriched with tracks, through the full lowering
/// chain (Requirement IR -> Engineering IR -> Schematic IR -> PCB IR), mapping any `IrError` to
/// an internal machine error.
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
    let tracks = ctx.tracks();
    let req_ir = RequirementIr::project(&intent, &reqs, &links)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    let eng_ir = EngineeringIr::project(&req_ir, &blocks, &constraints)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    let sch_ir = SchematicIr::project(&eng_ir, &components, &pins, &nets)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    PcbIr::project(&sch_ir, board.as_ref(), &placements, &tracks)
        .map_err(|e| MachineError::Internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_and_ground_default_wider_than_signal() {
        // The core policy: current-carrying rails default wider than signal traces.
        assert!(class_width_mm(NetClass::Power) > class_width_mm(NetClass::Signal));
        assert!(class_width_mm(NetClass::Ground) > class_width_mm(NetClass::Signal));
        // Power and ground share the wider default; signal keeps the original 0.25 mm.
        assert_eq!(
            class_width_mm(NetClass::Power),
            class_width_mm(NetClass::Ground)
        );
        assert_eq!(class_width_mm(NetClass::Signal), 0.25);
    }

    #[test]
    fn all_class_widths_are_positive_and_finite() {
        // Guards Track::validate's positive-finite width invariant for every class.
        for c in [NetClass::Power, NetClass::Ground, NetClass::Signal] {
            let w = class_width_mm(c);
            assert!(w.is_finite() && w > 0.0);
        }
    }
}
