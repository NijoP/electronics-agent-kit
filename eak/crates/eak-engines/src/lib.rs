//! Domain engines (deterministic services). Phase 1 shipped the Planning Engine; Phase 2
//! adds the [`ConstraintEngine`] (does a value satisfy a bound? do two bounds contradict?)
//! and a generic [`VerificationEngine`] — a [`Rule`] registry whose first rule,
//! [`ConstraintConsistencyRule`], catches mutually-unsatisfiable constraints. ERC/DRC/DFM
//! are future rules over the same framework. All engines are pure and deterministic.
//! See `docs/engineering/constraint-engine.md` and `docs/engineering/verification-engine.md`.

use eak_domain::{
    Component, Constraint, ConstraintKind, EntityId, Net, NetClass, Pin, PinElectricalType,
    Requirement, ViolationSeverity,
};
use eak_units::{PhysicalQuantity, UnitError};
use std::cmp::Ordering;

/// One step in an agent's elicitation reasoning-plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    pub name: String,
}

/// Sequences an agent's elicitation steps (a *reasoning plan*, distinct from the workflow
/// plan). Phase 1 uses a fixed linear plan; backtracking/branching is deferred.
#[derive(Debug, Clone, Default)]
pub struct PlanningEngine;

impl PlanningEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn elicitation_plan(&self) -> Vec<PlanStep> {
        ["read_intent", "propose_requirements", "validate_and_commit"]
            .into_iter()
            .map(|s| PlanStep {
                name: s.to_string(),
            })
            .collect()
    }
}

// ============================== Constraint Engine ==============================

/// Pure constraint arithmetic over [`PhysicalQuantity`] bounds (P9). Stateless.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConstraintEngine;

impl ConstraintEngine {
    pub fn new() -> Self {
        Self
    }

    /// True when `value` honours `constraint`. Errors (never silently) if the value and the
    /// bound are of different physical dimensions (P9).
    pub fn satisfies(
        &self,
        value: &PhysicalQuantity,
        constraint: &Constraint,
    ) -> Result<bool, UnitError> {
        let ord = value.try_compare(&constraint.bound)?;
        Ok(match constraint.kind {
            ConstraintKind::Max => ord != Ordering::Greater,
            ConstraintKind::Min => ord != Ordering::Less,
            ConstraintKind::Equal => ord == Ordering::Equal,
        })
    }

    /// True when two constraints on the same dimension are mutually unsatisfiable — their
    /// feasible intervals on the SI axis are disjoint (e.g. `power <= 5 W` and
    /// `power >= 8 W`). Constraints on different dimensions never contradict.
    pub fn contradiction(&self, a: &Constraint, b: &Constraint) -> bool {
        if a.bound.dimension() != b.bound.dimension() {
            return false;
        }
        let (a_lo, a_hi) = feasible_interval(a);
        let (b_lo, b_hi) = feasible_interval(b);
        let lo = a_lo.max(b_lo);
        let hi = a_hi.min(b_hi);
        // Disjoint when the combined lower edge sits above the combined upper edge by more
        // than a relative epsilon (so floating-point equality is not a false contradiction).
        let scale = lo.abs().max(hi.abs()).max(1.0);
        lo - hi > 1e-9 * scale
    }
}

/// The closed feasible interval of a constraint on its dimension's SI axis.
fn feasible_interval(c: &Constraint) -> (f64, f64) {
    let x = c.bound.si_magnitude();
    match c.kind {
        ConstraintKind::Max => (f64::NEG_INFINITY, x),
        ConstraintKind::Min => (x, f64::INFINITY),
        ConstraintKind::Equal => (x, x),
    }
}

// ============================= Verification Engine =============================

/// The read-only inputs a [`Rule`] evaluates against — a snapshot of the design's
/// machine-checkable layer. Additive in Phase 3: the schematic layer (components, pins,
/// nets) so ERC rules can reason over connectivity (P9).
pub struct VerificationContext<'a> {
    pub requirements: &'a [Requirement],
    pub constraints: &'a [Constraint],
    pub components: &'a [Component],
    pub pins: &'a [Pin],
    pub nets: &'a [Net],
}

/// A problem a rule detected. Not yet a domain `Violation` — the runtime mints that at the
/// commit seam (P3); this is the engine's pure judgement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViolationFinding {
    pub rule: String,
    pub severity: ViolationSeverity,
    /// The entities implicated, in a stable (sorted) order so dedup keys are reproducible.
    pub subjects: Vec<EntityId>,
    pub message: String,
}

/// One verification check. The framework runs many of these; ERC/DRC/DFM are future
/// specializations of this same trait (reuse: one framework, many checks).
pub trait Rule {
    fn id(&self) -> &str;
    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding>;
}

/// Runs a registered set of [`Rule`]s and collects their findings. Deterministic: findings
/// come out in rule-registration order, each rule in its own deterministic order.
#[derive(Default)]
pub struct VerificationEngine {
    rules: Vec<Box<dyn Rule>>,
}

impl VerificationEngine {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Builder-style registration.
    pub fn with_rule(mut self, rule: Box<dyn Rule>) -> Self {
        self.rules.push(rule);
        self
    }

    pub fn register(&mut self, rule: Box<dyn Rule>) {
        self.rules.push(rule);
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Evaluate every rule and return all findings.
    pub fn run(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        self.rules.iter().flat_map(|r| r.evaluate(ctx)).collect()
    }
}

/// First verification rule: every pair of active constraints must be mutually satisfiable.
pub struct ConstraintConsistencyRule;

impl ConstraintConsistencyRule {
    pub const ID: &'static str = "constraint-consistency";

    pub fn new() -> Self {
        Self
    }
}
impl Default for ConstraintConsistencyRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for ConstraintConsistencyRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        let engine = ConstraintEngine::new();
        let active: Vec<&Constraint> = ctx.constraints.iter().filter(|c| c.is_active()).collect();
        let mut findings = Vec::new();
        for i in 0..active.len() {
            for j in (i + 1)..active.len() {
                let (a, b) = (active[i], active[j]);
                if engine.contradiction(a, b) {
                    let mut subjects = vec![a.id, b.id];
                    subjects.sort(); // stable dedup key regardless of pair order
                    findings.push(ViolationFinding {
                        rule: Self::ID.to_string(),
                        severity: ViolationSeverity::Error,
                        subjects,
                        message: format!(
                            "constraints {} and {} are mutually unsatisfiable: \"{}\" vs \"{}\"",
                            a.id.short(),
                            b.id.short(),
                            a.statement,
                            b.statement
                        ),
                    });
                }
            }
        }
        findings
    }
}

/// Resolve a net's member pin ids against the context's pin slice, preserving member order
/// so findings stay deterministic. Unknown ids (dangling references) are silently skipped —
/// member-pin integrity is enforced at the commit seam (P3), not by a rule.
fn resolve_members<'a>(net: &Net, pins: &'a [Pin]) -> Vec<&'a Pin> {
    net.members
        .iter()
        .filter_map(|id| pins.iter().find(|p| p.id == *id))
        .collect()
}

// ================================== ERC Rules ==================================

/// ERC rule: a power net that has at least one consumer ([`PinElectricalType::PowerIn`]) but
/// no source ([`PinElectricalType::PowerOut`]) is undriven — nothing supplies it (P9). Nets
/// are scanned in slice order for determinism.
pub struct ErcPowerNetUndrivenRule;

impl ErcPowerNetUndrivenRule {
    pub const ID: &'static str = "erc-power-net-undriven";

    pub fn new() -> Self {
        Self
    }
}
impl Default for ErcPowerNetUndrivenRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for ErcPowerNetUndrivenRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        let mut findings = Vec::new();
        for net in ctx.nets.iter().filter(|n| n.class == NetClass::Power) {
            let members = resolve_members(net, ctx.pins);
            let has_consumer = members
                .iter()
                .any(|p| p.electrical_type == PinElectricalType::PowerIn);
            let has_source = members
                .iter()
                .any(|p| p.electrical_type == PinElectricalType::PowerOut);
            if has_consumer && !has_source {
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![net.id],
                    message: format!(
                        "power net \"{}\" ({}) has consumers but no driver (no PowerOut pin)",
                        net.name,
                        net.id.short()
                    ),
                });
            }
        }
        findings
    }
}

/// ERC rule: a net with two or more drivers ([`PinElectricalType::PowerOut`] or
/// [`PinElectricalType::Output`]) has contending sources (P9). Applies to every net class;
/// nets are scanned in slice order for determinism.
pub struct ErcMultipleDriversRule;

impl ErcMultipleDriversRule {
    pub const ID: &'static str = "erc-multiple-drivers";

    pub fn new() -> Self {
        Self
    }
}
impl Default for ErcMultipleDriversRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for ErcMultipleDriversRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        let mut findings = Vec::new();
        for net in ctx.nets.iter() {
            let driver_count = resolve_members(net, ctx.pins)
                .iter()
                .filter(|p| {
                    matches!(
                        p.electrical_type,
                        PinElectricalType::PowerOut | PinElectricalType::Output
                    )
                })
                .count();
            if driver_count >= 2 {
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![net.id],
                    message: format!(
                        "net \"{}\" ({}) has {} drivers; only one is allowed",
                        net.name,
                        net.id.short(),
                        driver_count
                    ),
                });
            }
        }
        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_domain::ConstraintStatus;
    use eak_units::Unit;

    #[test]
    fn plan_is_linear_and_nonempty() {
        let plan = PlanningEngine::new().elicitation_plan();
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0].name, "read_intent");
    }

    fn con(id: u128, kind: ConstraintKind, mag: f64, unit: Unit) -> Constraint {
        Constraint {
            id: EntityId(id),
            statement: format!("{kind:?} {mag} {}", unit.symbol()),
            subject_requirement: EntityId(100 + id),
            kind,
            bound: PhysicalQuantity::new(mag, unit),
            source: EntityId(100 + id),
            status: ConstraintStatus::Active,
        }
    }

    #[test]
    fn satisfies_respects_kind_and_units() {
        let e = ConstraintEngine::new();
        let max5w = con(1, ConstraintKind::Max, 5.0, Unit::Watt);
        // 4200 mW <= 5 W holds; 6 W <= 5 W does not.
        assert!(e
            .satisfies(&PhysicalQuantity::new(4200.0, Unit::Milliwatt), &max5w)
            .unwrap());
        assert!(!e
            .satisfies(&PhysicalQuantity::new(6.0, Unit::Watt), &max5w)
            .unwrap());
    }

    #[test]
    fn satisfies_cross_dimension_errors() {
        let e = ConstraintEngine::new();
        let max5w = con(1, ConstraintKind::Max, 5.0, Unit::Watt);
        assert!(e
            .satisfies(&PhysicalQuantity::new(5.0, Unit::Volt), &max5w)
            .is_err());
    }

    #[test]
    fn contradiction_detects_disjoint_bounds() {
        let e = ConstraintEngine::new();
        let max5 = con(1, ConstraintKind::Max, 5.0, Unit::Watt);
        let min8 = con(2, ConstraintKind::Min, 8.0, Unit::Watt);
        let min3 = con(3, ConstraintKind::Min, 3.0, Unit::Watt);
        // <=5 W and >=8 W cannot both hold; <=5 W and >=3 W can.
        assert!(e.contradiction(&max5, &min8));
        assert!(!e.contradiction(&max5, &min3));
        // expressed in different units but same value: <=5 W vs >=8000 mW still contradicts.
        let min8000mw = con(4, ConstraintKind::Min, 8000.0, Unit::Milliwatt);
        assert!(e.contradiction(&max5, &min8000mw));
    }

    #[test]
    fn contradiction_ignores_different_dimensions() {
        let e = ConstraintEngine::new();
        let max5w = con(1, ConstraintKind::Max, 5.0, Unit::Watt);
        let min8mm = con(2, ConstraintKind::Min, 8.0, Unit::Millimetre);
        assert!(!e.contradiction(&max5w, &min8mm));
    }

    #[test]
    fn consistency_rule_flags_contradictory_pair() {
        let rule = ConstraintConsistencyRule::new();
        let cons = vec![
            con(1, ConstraintKind::Max, 5.0, Unit::Watt),
            con(2, ConstraintKind::Min, 8.0, Unit::Watt),
            con(3, ConstraintKind::Max, 50.0, Unit::Millimetre),
        ];
        let ctx = VerificationContext {
            requirements: &[],
            constraints: &cons,
            components: &[],
            pins: &[],
            nets: &[],
        };
        let findings = rule.evaluate(&ctx);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, ConstraintConsistencyRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(1), EntityId(2)]);
    }

    #[test]
    fn verification_engine_runs_registered_rules() {
        let engine =
            VerificationEngine::new().with_rule(Box::new(ConstraintConsistencyRule::new()));
        assert_eq!(engine.rule_count(), 1);
        let cons = vec![
            con(1, ConstraintKind::Max, 5.0, Unit::Watt),
            con(2, ConstraintKind::Min, 8.0, Unit::Watt),
        ];
        let findings = engine.run(&VerificationContext {
            requirements: &[],
            constraints: &cons,
            components: &[],
            pins: &[],
            nets: &[],
        });
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn superseded_constraints_are_ignored_by_consistency_rule() {
        let rule = ConstraintConsistencyRule::new();
        let mut superseded = con(2, ConstraintKind::Min, 8.0, Unit::Watt);
        superseded.status = ConstraintStatus::Superseded;
        let cons = vec![con(1, ConstraintKind::Max, 5.0, Unit::Watt), superseded];
        let findings = rule.evaluate(&VerificationContext {
            requirements: &[],
            constraints: &cons,
            components: &[],
            pins: &[],
            nets: &[],
        });
        assert!(findings.is_empty());
    }

    // -------------------------------- ERC rule tests --------------------------------

    fn pin(id: u128, ty: PinElectricalType) -> Pin {
        Pin {
            id: EntityId(id),
            component: EntityId(900 + id),
            designation: format!("P{id}"),
            electrical_type: ty,
        }
    }

    fn net(id: u128, class: NetClass, members: Vec<u128>) -> Net {
        Net {
            id: EntityId(id),
            name: format!("NET{id}"),
            class,
            members: members.into_iter().map(EntityId).collect(),
        }
    }

    fn erc_ctx<'a>(pins: &'a [Pin], nets: &'a [Net]) -> VerificationContext<'a> {
        VerificationContext {
            requirements: &[],
            constraints: &[],
            components: &[],
            pins,
            nets,
        }
    }

    #[test]
    fn undriven_power_net_is_flagged() {
        // A power net with two consumers and no source is undriven.
        let pins = vec![
            pin(1, PinElectricalType::PowerIn),
            pin(2, PinElectricalType::PowerIn),
        ];
        let nets = vec![net(10, NetClass::Power, vec![1, 2])];
        let findings = ErcPowerNetUndrivenRule::new().evaluate(&erc_ctx(&pins, &nets));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, ErcPowerNetUndrivenRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(10)]);
    }

    #[test]
    fn driven_power_net_passes_both_erc_rules() {
        // One PowerOut driver and N PowerIn consumers: not undriven, not contended.
        let pins = vec![
            pin(1, PinElectricalType::PowerOut),
            pin(2, PinElectricalType::PowerIn),
            pin(3, PinElectricalType::PowerIn),
        ];
        let nets = vec![net(10, NetClass::Power, vec![1, 2, 3])];
        let ctx = erc_ctx(&pins, &nets);
        assert!(ErcPowerNetUndrivenRule::new().evaluate(&ctx).is_empty());
        assert!(ErcMultipleDriversRule::new().evaluate(&ctx).is_empty());
    }

    #[test]
    fn two_drivers_are_flagged() {
        // Two sources (PowerOut + Output) on one net contend.
        let pins = vec![
            pin(1, PinElectricalType::PowerOut),
            pin(2, PinElectricalType::Output),
            pin(3, PinElectricalType::PowerIn),
        ];
        let nets = vec![net(10, NetClass::Signal, vec![1, 2, 3])];
        let findings = ErcMultipleDriversRule::new().evaluate(&erc_ctx(&pins, &nets));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, ErcMultipleDriversRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(10)]);
    }

    #[test]
    fn power_net_without_consumers_is_not_flagged_undriven() {
        // No PowerIn member: nothing demands a driver, so the undriven rule stays silent.
        let pins = vec![pin(1, PinElectricalType::PowerOut)];
        let nets = vec![net(10, NetClass::Power, vec![1])];
        assert!(ErcPowerNetUndrivenRule::new()
            .evaluate(&erc_ctx(&pins, &nets))
            .is_empty());
    }
}
