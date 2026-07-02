//! Schematic Planning state machine (instance) — DETERMINISTIC in Phase 3.
//!
//! It realizes each [`FunctionalBlock`] as a concrete [`Component`] with its [`Pin`]s, joins
//! the power and ground pins into first-class [`Net`]s, and projects the [`SchematicIr`] (P6).
//! It makes NO reasoning calls — classification and reference-designator assignment are
//! deterministic (per-class counters, fixed pin templates), so a replay is bit-identical
//! (P4). It is idempotent: if components already exist (e.g. on an ERC-loop back) it only
//! (re)projects the IR and re-emits the milestone, committing nothing new. See
//! `docs/state-machines/schematic-planning.md`.

use eak_compiler::{EngineeringIr, RequirementIr, SchematicIr};
use eak_domain::{
    Component, ComponentClass, EntityId, FunctionalBlock, Net, NetClass, Pin, PinElectricalType,
    ProvenanceLink, RelationType, Requirement, RequirementCategory,
};
use eak_ports::Event;
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};

pub struct SchematicPlanningMachine;

impl SchematicPlanningMachine {
    pub fn new() -> Self {
        Self
    }
}
impl Default for SchematicPlanningMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for SchematicPlanningMachine {
    fn name(&self) -> &str {
        "SchematicPlanning"
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
                // Idempotent: realize the schematic only once. A re-entry (ERC loop-back)
                // skips synthesis and just (re)projects the IR below.
                if ctx.components().is_empty() {
                    let blocks = ctx.functional_blocks();
                    let requirements = ctx.requirements();

                    // Deterministic per-class reference-designator counters.
                    let mut connector_n = 1u32;
                    let mut regulator_n = 1u32;
                    let mut ic_n = 1u32;

                    for block in &blocks {
                        let class = classify(block, &requirements);
                        let refdes = match class {
                            ComponentClass::Connector => {
                                let r = format!("J{connector_n}");
                                connector_n += 1;
                                r
                            }
                            ComponentClass::Regulator => {
                                let r = format!("VR{regulator_n}");
                                regulator_n += 1;
                                r
                            }
                            // Every other (non-source) block is realized as an IC load (Phase 3).
                            _ => {
                                let r = format!("U{ic_n}");
                                ic_n += 1;
                                r
                            }
                        };

                        let cid = ctx.fresh_id();
                        let component = Component {
                            id: cid,
                            refdes,
                            class,
                            value: None,
                            from_block: block.id,
                        };

                        // Fixed pin template per class: a connector source exposes a driven
                        // rail; a regulator sinks its input and drives its output; a load
                        // consumes the rail. All reference ground.
                        let pin_specs: Vec<(&str, PinElectricalType)> = match class {
                            ComponentClass::Connector => vec![
                                ("VBUS", PinElectricalType::PowerOut),
                                ("GND", PinElectricalType::Ground),
                            ],
                            ComponentClass::Regulator => vec![
                                ("VIN", PinElectricalType::PowerIn),
                                ("VOUT", PinElectricalType::PowerOut),
                                ("GND", PinElectricalType::Ground),
                            ],
                            ComponentClass::Ic => vec![
                                ("VDD", PinElectricalType::PowerIn),
                                ("GND", PinElectricalType::Ground),
                            ],
                            // Passives carry two undirected terminals. Defensive: classify()
                            // does not emit these yet, but the match stays exhaustive so a
                            // future class can never silently inherit the IC-load template.
                            ComponentClass::Resistor | ComponentClass::Capacitor => vec![
                                ("1", PinElectricalType::Passive),
                                ("2", PinElectricalType::Passive),
                            ],
                        };
                        let pins: Vec<Pin> = pin_specs
                            .iter()
                            .map(|(designation, electrical_type)| Pin {
                                id: ctx.fresh_id(),
                                component: cid,
                                designation: (*designation).to_string(),
                                electrical_type: *electrical_type,
                            })
                            .collect();

                        // Provenance: the component derives from the block it realizes (P3).
                        let link = ProvenanceLink {
                            id: ctx.fresh_id(),
                            from: cid,
                            to: block.id,
                            relation: RelationType::DerivedFrom,
                        };
                        ctx.invoke(CapabilityRequest::RealizeComponent {
                            component,
                            pins,
                            links: vec![link],
                        })
                        .map_err(|e| MachineError::Internal(e.to_string()))?;
                    }

                    // Join the realized pins into first-class nets (P13: connectivity is
                    // explicit). Ground collects every ground pin (below); the POWER rails depend
                    // on whether a regulator is present. WITH a regulator the source feeds the
                    // regulator's INPUT rail ("VBUS") and the regulator's OUTPUT feeds the loads
                    // ("VOUT") — two separate single-driver rails, so a regulator's VIN and VOUT no
                    // longer short onto one net (the old collapsed-rail defect). WITHOUT a regulator
                    // a single "VBUS" rail collects every power pin (byte-identical to before).
                    // Rails are committed in the fixed order VBUS -> VOUT -> GND, only if non-empty,
                    // so the fresh_id sequence and replay stay deterministic (P4). Multi-regulator /
                    // multi-voltage-domain separation is a later scope.
                    let pins = ctx.pins();
                    let components = ctx.components();
                    let class_of = |pin: &Pin| -> Option<ComponentClass> {
                        components
                            .iter()
                            .find(|c| c.id == pin.component)
                            .map(|c| c.class)
                    };
                    let has_regulator = components
                        .iter()
                        .any(|c| c.class == ComponentClass::Regulator);

                    let mut rails: Vec<(&str, Vec<EntityId>)> = Vec::new();
                    if has_regulator {
                        // Input rail: source drivers (connector PowerOut) + regulator inputs (VIN).
                        let vbus = pins
                            .iter()
                            .filter(|p| {
                                let c = class_of(p);
                                (p.electrical_type == PinElectricalType::PowerOut
                                    && c == Some(ComponentClass::Connector))
                                    || (p.electrical_type == PinElectricalType::PowerIn
                                        && c == Some(ComponentClass::Regulator))
                            })
                            .map(|p| p.id)
                            .collect();
                        // Output rail: regulator drivers (VOUT) + load inputs (every non-regulator
                        // PowerIn — an IC's VDD).
                        let vout = pins
                            .iter()
                            .filter(|p| {
                                let c = class_of(p);
                                (p.electrical_type == PinElectricalType::PowerOut
                                    && c == Some(ComponentClass::Regulator))
                                    || (p.electrical_type == PinElectricalType::PowerIn
                                        && c != Some(ComponentClass::Regulator))
                            })
                            .map(|p| p.id)
                            .collect();
                        rails.push(("VBUS", vbus));
                        rails.push(("VOUT", vout));
                    } else {
                        let vbus = pins
                            .iter()
                            .filter(|p| {
                                matches!(
                                    p.electrical_type,
                                    PinElectricalType::PowerOut | PinElectricalType::PowerIn
                                )
                            })
                            .map(|p| p.id)
                            .collect();
                        rails.push(("VBUS", vbus));
                    }

                    for (name, members) in rails {
                        if !members.is_empty() {
                            let net = Net {
                                id: ctx.fresh_id(),
                                name: name.into(),
                                class: NetClass::Power,
                                members,
                                // Deterministic Schematic Planning does not yet derive per-net
                                // load currents (a Datasheet-Intelligence / analysis input), so the
                                // ampacity floor stays unstated here and its DRC check is silent.
                                current: None,
                            };
                            ctx.invoke(CapabilityRequest::CreateNet { net, links: vec![] })
                                .map_err(|e| MachineError::Internal(e.to_string()))?;
                        }
                    }

                    let ground_members: Vec<EntityId> = pins
                        .iter()
                        .filter(|p| p.electrical_type == PinElectricalType::Ground)
                        .map(|p| p.id)
                        .collect();
                    if !ground_members.is_empty() {
                        let net = Net {
                            id: ctx.fresh_id(),
                            name: "GND".into(),
                            class: NetClass::Ground,
                            members: ground_members,
                            current: None,
                        };
                        ctx.invoke(CapabilityRequest::CreateNet { net, links: vec![] })
                            .map_err(|e| MachineError::Internal(e.to_string()))?;
                    }
                }

                // Project the Schematic IR (P6) and record the boundary milestone. The
                // projection re-asserts connectivity integrity (IrError -> Internal).
                let schematic = project_schematic(ctx)?;
                ctx.emit(vec![Event::SchematicIrProduced {
                    schema_version: schematic.schema_version,
                    net_count: schematic.nets.len(),
                }])
                .map_err(|e| MachineError::Internal(e.to_string()))?;
                Ok(StepResult::Done)
            }

            other => Err(MachineError::Internal(format!("unknown state {other}"))),
        }
    }
}

/// Classify a block into a [`ComponentClass`] from its primary requirement (P3). A voltage
/// regulator/LDO is recognized first ([`ComponentClass::Regulator`]) — its wording names a
/// regulation concept regardless of category. Otherwise a block is a power *source*
/// ([`ComponentClass::Connector`]) iff that requirement is functional AND its wording names a
/// power-entry concept; failing both it is a load ([`ComponentClass::Ic`]).
fn classify(block: &FunctionalBlock, requirements: &[Requirement]) -> ComponentClass {
    let primary = block
        .requirements
        .first()
        .and_then(|rid| requirements.iter().find(|r| r.id == *rid));
    if let Some(req) = primary {
        let s = req.statement.to_lowercase();
        // A regulator/LDO takes precedence: it both sinks an upstream rail and drives a
        // downstream one, so it must not be mistaken for a plain power-entry source.
        const REGULATOR_CUES: [&str; 3] = ["regulator", "ldo", "voltage regulation"];
        if REGULATOR_CUES.iter().any(|cue| s.contains(cue)) {
            return ComponentClass::Regulator;
        }
        const SOURCE_CUES: [&str; 4] = ["usb", "power", "connector", "supply"];
        let is_source = req.category == RequirementCategory::Functional
            && SOURCE_CUES.iter().any(|cue| s.contains(cue));
        if is_source {
            return ComponentClass::Connector;
        }
    }
    ComponentClass::Ic
}

/// Project canonical state into the [`SchematicIr`] through the full lowering chain
/// (Requirement IR -> Engineering IR -> Schematic IR), mapping any `IrError` to an internal
/// machine error. Shared by the synthesis path and the idempotent re-projection path.
fn project_schematic(ctx: &mut dyn AgentContext) -> Result<SchematicIr, MachineError> {
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
    let req_ir = RequirementIr::project(&intent, &reqs, &links)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    let eng_ir = EngineeringIr::project(&req_ir, &blocks, &constraints)
        .map_err(|e| MachineError::Internal(e.to_string()))?;
    SchematicIr::project(&eng_ir, &components, &pins, &nets)
        .map_err(|e| MachineError::Internal(e.to_string()))
}
