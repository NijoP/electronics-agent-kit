//! Domain engines (deterministic services). Phase 1 shipped the Planning Engine; Phase 2
//! adds the [`ConstraintEngine`] (does a value satisfy a bound? do two bounds contradict?)
//! and a generic [`VerificationEngine`] — a [`Rule`] registry whose first rule,
//! [`ConstraintConsistencyRule`], catches mutually-unsatisfiable constraints. ERC/DRC/DFM
//! are future rules over the same framework. All engines are pure and deterministic.
//! See `docs/engineering/constraint-engine.md` and `docs/engineering/verification-engine.md`.

use eak_domain::{
    Board, BomLineItem, Component, ComponentClass, Constraint, ConstraintKind, EntityId, Net,
    NetClass, Part, PartLifecycle, Pin, PinElectricalType, Placement, Requirement,
    RequirementCategory, Track, Violation, ViolationSeverity,
};
use eak_units::{Dimension, PhysicalQuantity, UnitError};
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
/// nets) so ERC rules can reason over connectivity, the BOM layer (parts, line items) so
/// BOM rules can reason over coverage and lifecycle, and the PCB layer (`board`, `placements`)
/// so DRC rules can reason over physical fit and courtyard collisions (P9). `board` is
/// `Option` because DRC runs only once an outline exists; when absent, geometry rules emit
/// no findings rather than guessing a substrate. The routing layer (`tracks`) lets the
/// trace-width DRC rule reason over the copper realizing each net.
pub struct VerificationContext<'a> {
    pub requirements: &'a [Requirement],
    pub constraints: &'a [Constraint],
    pub components: &'a [Component],
    pub pins: &'a [Pin],
    pub nets: &'a [Net],
    pub parts: &'a [Part],
    pub bom_line_items: &'a [BomLineItem],
    pub board: Option<&'a Board>,
    pub placements: &'a [Placement],
    pub tracks: &'a [Track],
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

    /// The ids of the registered rules. A verification phase uses this to scope its pass/fail
    /// gate to *its own* findings: a phase fails iff one of ITS rules has an open, blocking
    /// violation — not iff any violation anywhere is open. (The global "all violations clear"
    /// check belongs to the Manufacturing gate, which spans every rule-check phase.)
    pub fn rule_ids(&self) -> Vec<&str> {
        self.rules.iter().map(|r| r.id()).collect()
    }

    /// Count the OPEN, blocking violations that belong to THIS engine's rules — the per-phase
    /// gate. Scoping to the engine's own rule ids (rather than the global violation set) keeps a
    /// phase's pass/fail about its own checks: a violation raised by a *different* rule-check
    /// phase — e.g. a DFM violation still open while DRC re-runs on a DFM loop-back — must not
    /// fail this phase. The cross-phase "all violations clear" check is the Manufacturing gate.
    pub fn count_open_blocking(&self, violations: &[Violation]) -> usize {
        let ids = self.rule_ids();
        violations
            .iter()
            .filter(|v| v.is_blocking() && ids.contains(&v.rule.as_str()))
            .count()
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

// ================================ Part Catalog =================================

/// A catalog entry: the manufacturer part data a [`ComponentClass`] maps to. Carries
/// `'static` strings — this is a fixed, compiled-in catalog, not a live distributor feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogPart {
    pub mpn: &'static str,
    pub manufacturer: &'static str,
    pub lifecycle: PartLifecycle,
    pub datasheet: &'static str,
}

/// Pure, deterministic part-selection service: maps a [`ComponentClass`] to a concrete
/// catalog part (P9 — same class always yields the same part). A stand-in for a distributor
/// lookup; the regulator entry is deliberately [`Eol`](PartLifecycle::Eol) so the BOM
/// lifecycle gate has something to flag in the end-to-end demo.
#[derive(Debug, Clone, Copy, Default)]
pub struct PartCatalog;

impl PartCatalog {
    pub fn new() -> Self {
        Self
    }

    /// The catalog part realizing a given component class. Total over `ComponentClass`.
    pub fn part_for(&self, class: ComponentClass) -> CatalogPart {
        match class {
            ComponentClass::Connector => CatalogPart {
                mpn: "USB4110-GF-A",
                manufacturer: "GCT",
                lifecycle: PartLifecycle::Active,
                datasheet: "https://gct.co/usb4110",
            },
            ComponentClass::Ic => CatalogPart {
                mpn: "STM32L010F4P6",
                manufacturer: "STMicroelectronics",
                lifecycle: PartLifecycle::Active,
                datasheet: "https://st.com/stm32l0",
            },
            ComponentClass::Regulator => CatalogPart {
                mpn: "LM1117-3.3",
                manufacturer: "Texas Instruments",
                lifecycle: PartLifecycle::Eol,
                datasheet: "https://ti.com/lm1117",
            },
            ComponentClass::Resistor => CatalogPart {
                mpn: "RC0402FR-0710KL",
                manufacturer: "Yageo",
                lifecycle: PartLifecycle::Active,
                datasheet: "https://yageo.com/rc0402",
            },
            ComponentClass::Capacitor => CatalogPart {
                mpn: "CL05A104KA5NNNC",
                manufacturer: "Samsung",
                lifecycle: PartLifecycle::Active,
                datasheet: "https://samsung.com/cl05",
            },
        }
    }
}

// ================================== BOM Rules ==================================

/// BOM rule: every [`Component`] in the schematic must be covered by at least one
/// [`BomLineItem`] — an uncovered component cannot be ordered (P13). Components are scanned
/// in slice order for determinism.
pub struct BomCoverageRule;

impl BomCoverageRule {
    pub const ID: &'static str = "bom-coverage";

    pub fn new() -> Self {
        Self
    }
}
impl Default for BomCoverageRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for BomCoverageRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        let mut findings = Vec::new();
        for component in ctx.components.iter() {
            let covered = ctx
                .bom_line_items
                .iter()
                .any(|item| item.components.contains(&component.id));
            if !covered {
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![component.id],
                    message: format!(
                        "component \"{}\" ({}) is not covered by any BOM line item",
                        component.refdes,
                        component.id.short()
                    ),
                });
            }
        }
        findings
    }
}

/// BOM rule: a [`BomLineItem`] whose [`Part`] is [`Eol`](PartLifecycle::Eol) is an Error (it
/// can no longer be sourced); an [`Nrnd`](PartLifecycle::Nrnd) part is a Warning the designer
/// should heed (P13). Line items are scanned in slice order for determinism. A line whose
/// part is absent from `ctx.parts` is silently skipped — part-reference integrity is enforced
/// at the commit seam (P3), not by this rule.
pub struct BomLifecycleRule;

impl BomLifecycleRule {
    pub const ID: &'static str = "bom-lifecycle";

    pub fn new() -> Self {
        Self
    }
}
impl Default for BomLifecycleRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for BomLifecycleRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        let mut findings = Vec::new();
        for item in ctx.bom_line_items.iter() {
            let Some(part) = ctx.parts.iter().find(|p| p.id == item.part) else {
                continue;
            };
            // One match over the lifecycle: Active lines are fine and skipped; every other
            // variant maps to its (severity, label) pair. Single source of truth, so there is
            // no unreachable! arm to drift out of sync on a future refactor.
            let (severity, state) = match part.lifecycle {
                PartLifecycle::Eol => (ViolationSeverity::Error, "end-of-life"),
                PartLifecycle::Nrnd => (
                    ViolationSeverity::Warning,
                    "not recommended for new designs",
                ),
                PartLifecycle::Active => continue,
            };
            findings.push(ViolationFinding {
                rule: Self::ID.to_string(),
                severity,
                subjects: vec![item.id],
                message: format!(
                    "BOM line {} orders part \"{}\" which is {}",
                    item.id.short(),
                    part.mpn,
                    state
                ),
            });
        }
        findings
    }
}

// ================================== DRC Rules ==================================

/// DRC rule: every [`Placement`]'s courtyard must lie wholly within the [`Board`] outline.
/// The courtyard spans `[x, x + width] x [y, y + height]`; it is out of bounds when any edge
/// crosses the origin or the board's far edge. Comparisons are on the SI axis via
/// `si_magnitude()` so the check is unit-independent (P9). With no board there is no outline
/// to fit within, so the rule emits nothing. Placements are scanned in slice order for
/// determinism.
pub struct DrcOutOfBoundsRule;

impl DrcOutOfBoundsRule {
    pub const ID: &'static str = "drc-out-of-bounds";

    pub fn new() -> Self {
        Self
    }
}
impl Default for DrcOutOfBoundsRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DrcOutOfBoundsRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        let Some(board) = ctx.board else {
            return Vec::new();
        };
        let (board_w, board_h) = (board.width.si_magnitude(), board.height.si_magnitude());
        let mut findings = Vec::new();
        for placement in ctx.placements.iter() {
            let x = placement.x.si_magnitude();
            let y = placement.y.si_magnitude();
            let w = placement.width.si_magnitude();
            let h = placement.height.si_magnitude();
            if x < 0.0 || y < 0.0 || x + w > board_w || y + h > board_h {
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![placement.id],
                    message: format!(
                        "placement {} extends outside the board outline",
                        placement.id.short()
                    ),
                });
            }
        }
        findings
    }
}

/// DRC rule: no two courtyards on the same [`BoardSide`] may overlap — components on opposite
/// copper sides never collide, so only same-side pairs are tested. Overlap is a standard AABB
/// intersection on the SI axis (P9), using strict `<` (open-set): courtyards that merely *touch*
/// edge-to-edge (zero clearance) are NOT flagged here — minimum-clearance enforcement is a
/// separate future rule. Pairs are scanned `i < j` in slice order and each finding's subjects
/// are sorted, so output is deterministic regardless of placement order.
pub struct DrcCourtyardOverlapRule;

impl DrcCourtyardOverlapRule {
    pub const ID: &'static str = "drc-courtyard-overlap";

    pub fn new() -> Self {
        Self
    }
}
impl Default for DrcCourtyardOverlapRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DrcCourtyardOverlapRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        let mut findings = Vec::new();
        let placements = ctx.placements;
        for i in 0..placements.len() {
            for j in (i + 1)..placements.len() {
                let (a, b) = (&placements[i], &placements[j]);
                if a.side != b.side {
                    continue;
                }
                let (ax, ay) = (a.x.si_magnitude(), a.y.si_magnitude());
                let (aw, ah) = (a.width.si_magnitude(), a.height.si_magnitude());
                let (bx, by) = (b.x.si_magnitude(), b.y.si_magnitude());
                let (bw, bh) = (b.width.si_magnitude(), b.height.si_magnitude());
                let overlaps = ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah;
                if overlaps {
                    let mut subjects = vec![a.id, b.id];
                    subjects.sort(); // stable dedup key regardless of pair order
                    findings.push(ViolationFinding {
                        rule: Self::ID.to_string(),
                        severity: ViolationSeverity::Error,
                        subjects,
                        message: format!(
                            "placements {} and {} have overlapping courtyards on the same side",
                            a.id.short(),
                            b.id.short()
                        ),
                    });
                }
            }
        }
        findings
    }
}

/// DRC rule: every routed [`Track`]'s copper `width` must be at least the design's minimum
/// manufacturable trace width — the fabrication process floor. The floor is read from the
/// first length-dimensioned target on a [`Regulatory`](RequirementCategory::Regulatory)
/// requirement (a process/standards constraint, e.g. an IPC trace-width class), mirroring how
/// floor planning takes the board edge from the first Mechanical length target. A trace finer
/// than the floor cannot be etched by the chosen process, so it is an Error. With no such
/// requirement there is no stated process floor, so the rule emits nothing rather than guessing
/// one — a design that has not pinned a process is not spuriously failed. Comparisons are on
/// the SI axis via `si_magnitude()` so the check is unit-independent (P9); tracks are scanned in
/// slice order for determinism.
pub struct DrcTraceWidthRule;

impl DrcTraceWidthRule {
    pub const ID: &'static str = "drc-trace-width";

    pub fn new() -> Self {
        Self
    }
}
impl Default for DrcTraceWidthRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DrcTraceWidthRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        // The process floor: the first length-dimensioned target on a Regulatory requirement,
        // in commit order. Absent one, there is no floor to check against.
        let Some(floor) = ctx
            .requirements
            .iter()
            .filter(|r| r.category == RequirementCategory::Regulatory)
            .flat_map(|r| r.targets.iter())
            .find(|t| t.dimension() == Dimension::Length)
        else {
            return Vec::new();
        };
        let floor_si = floor.si_magnitude();
        let mut findings = Vec::new();
        for track in ctx.tracks.iter() {
            let width_si = track.width.si_magnitude();
            // Below the floor by more than an epsilon, so a width that merely equals the floor
            // (floating-point) is never a false violation. `scale` clamps to 1 m, and trace
            // widths are sub-millimetre, so in practice this is an absolute ~1 nm tolerance.
            let scale = width_si.abs().max(floor_si.abs()).max(1.0);
            if floor_si - width_si > 1e-9 * scale {
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![track.id],
                    message: format!(
                        "track {} width {} is finer than the {} process floor",
                        track.id.short(),
                        track.width,
                        floor
                    ),
                });
            }
        }
        findings
    }
}

// ================================== DFM Rules ==================================

/// The fabrication/assembly keep-out band from the board edge, in millimetres. Component
/// bodies and copper inside this band foul pick-and-place and are nicked during
/// depanelization, so a design that merely *fits* the outline can still be unmanufacturable. A
/// fixed process constant in Phase-3 scope (a real flow derives it from the fab/assembly
/// class); kept below the placement margin so a normally-placed design clears it.
const DFM_EDGE_CLEARANCE_MM: f64 = 0.5;

/// DFM rule: every [`Placement`]'s courtyard must keep at least [`DFM_EDGE_CLEARANCE_MM`] from
/// the [`Board`] edge. Distinct from [`DrcOutOfBoundsRule`], which only requires the courtyard
/// to *fit*: this demands an assembly keep-out band, so a component that fits but hugs the edge
/// is a manufacturability Error. Comparisons are on the SI axis via `si_magnitude()` (P9); with
/// no board there is no edge to measure from, so the rule emits nothing. Placements are scanned
/// in slice order for determinism.
pub struct DfmEdgeClearanceRule;

impl DfmEdgeClearanceRule {
    pub const ID: &'static str = "dfm-edge-clearance";

    pub fn new() -> Self {
        Self
    }
}
impl Default for DfmEdgeClearanceRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DfmEdgeClearanceRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        let Some(board) = ctx.board else {
            return Vec::new();
        };
        let (board_w, board_h) = (board.width.si_magnitude(), board.height.si_magnitude());
        let m = DFM_EDGE_CLEARANCE_MM * 1e-3; // millimetres -> metres (SI)
        let mut findings = Vec::new();
        for placement in ctx.placements.iter() {
            let x = placement.x.si_magnitude();
            let y = placement.y.si_magnitude();
            let w = placement.width.si_magnitude();
            let h = placement.height.si_magnitude();
            if x < m || y < m || x + w > board_w - m || y + h > board_h - m {
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![placement.id],
                    message: format!(
                        "placement {} is within the {} mm board-edge keep-out",
                        placement.id.short(),
                        DFM_EDGE_CLEARANCE_MM
                    ),
                });
            }
        }
        findings
    }
}

/// DFM rule: every [`Track`]'s copper must keep at least [`DFM_EDGE_CLEARANCE_MM`] from the
/// [`Board`] edge — edge copper is nicked during depanelization. The keep-out is four
/// axis-aligned edge bands. Because `x` and `y` vary linearly along a straight segment, the
/// copper band's extreme reach in each axis (the centre-line extreme ± half the width) occurs at
/// an endpoint; checking both endpoints' `±half` against the four bands is therefore exact for a
/// straight trace (no need to sample interior points). With no board the rule emits nothing.
/// Tracks are scanned in slice order for determinism.
pub struct DfmTraceEdgeClearanceRule;

impl DfmTraceEdgeClearanceRule {
    pub const ID: &'static str = "dfm-trace-edge-clearance";

    pub fn new() -> Self {
        Self
    }
}
impl Default for DfmTraceEdgeClearanceRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DfmTraceEdgeClearanceRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        let Some(board) = ctx.board else {
            return Vec::new();
        };
        let (board_w, board_h) = (board.width.si_magnitude(), board.height.si_magnitude());
        let m = DFM_EDGE_CLEARANCE_MM * 1e-3; // millimetres -> metres (SI)
        let mut findings = Vec::new();
        for track in ctx.tracks.iter() {
            let half = track.width.si_magnitude() / 2.0;
            let xs = [track.x1.si_magnitude(), track.x2.si_magnitude()];
            let ys = [track.y1.si_magnitude(), track.y2.si_magnitude()];
            let too_close = xs.iter().any(|&x| x - half < m || x + half > board_w - m)
                || ys.iter().any(|&y| y - half < m || y + half > board_h - m);
            if too_close {
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![track.id],
                    message: format!(
                        "track {} copper is within the {} mm board-edge keep-out",
                        track.id.short(),
                        DFM_EDGE_CLEARANCE_MM
                    ),
                });
            }
        }
        findings
    }
}

// ================================== EMC Rules ==================================

/// Free-space speed of light, in metres per second — the propagation speed used to size the
/// electrically-long threshold. This is a deliberately *lenient* first-order model: a real
/// signal on an FR-4 microstrip travels at roughly `c / sqrt(eps_eff)` (~half `c`), so its
/// on-board wavelength — and therefore its critical length — is SHORTER than the free-space
/// figure. Using the free-space value can only *under*-report emission risk, never invent it; a
/// velocity-factor refinement (an effective-permittivity term) would tighten the limit and is a
/// noted Phase-3 scope boundary.
const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

/// The "electrically long" fraction of a wavelength. A conductor longer than about one tenth of
/// the wavelength at the signal's frequency stops behaving as a lumped wire and radiates
/// efficiently / must be treated as a transmission line — the classic lambda/10 EMC rule of
/// thumb. The critical length is therefore `c / (ELECTRICAL_LENGTH_DIVISOR * f)`.
const ELECTRICAL_LENGTH_DIVISOR: f64 = 10.0;

/// EMC rule: a routed [`Track`] longer than the design's *electrically-long* threshold — one
/// tenth of the wavelength at the highest stated operating/emission frequency — is an efficient
/// radiator and a radiated-emissions risk, so it is an Error. The frequency is the largest
/// [`Frequency`](Dimension::Frequency)-dimensioned target across all requirements (the worst
/// case: the highest frequency gives the shortest wavelength and so the tightest limit). It is a
/// *different dimension* than the trace-width rule's length floor, so the two rules never
/// contend for the same target. With no frequency stated the design has not pinned an emission
/// spectrum, so the rule emits nothing rather than guessing one — mirroring how the trace-width
/// rule stays silent without a process floor. A track's length is the straight-line distance
/// between its endpoints on the SI axis (P9); tracks are scanned in slice order for determinism.
///
/// EMC is *analysis*, not pass/fail rule-checking in the full lifecycle (it interprets simulated
/// fields against limits — see `docs/state-machines/emc-analysis.md`). This rule is the
/// deterministic Phase-3 subset: a closed-form geometric proxy for the dominant emission
/// mechanism (an electrically-long trace acting as an antenna), evaluated on the same
/// [`Rule`] framework as ERC/DRC/DFM. Its `Failed` terminal loops back to Routing Planning —
/// the trace geometry is what a re-route changes.
pub struct EmcAntennaLengthRule;

impl EmcAntennaLengthRule {
    pub const ID: &'static str = "emc-antenna-length";

    pub fn new() -> Self {
        Self
    }
}
impl Default for EmcAntennaLengthRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for EmcAntennaLengthRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        // Every stated frequency target and the requirement that carries it. ABSENT any frequency
        // the design has not pinned an emission spectrum, so the rule is silent (mirroring the
        // trace-width rule without a process floor).
        let freq_targets: Vec<(&Requirement, f64)> = ctx
            .requirements
            .iter()
            .flat_map(|r| r.targets.iter().map(move |t| (r, t)))
            .filter(|(_, t)| t.dimension() == Dimension::Frequency)
            .map(|(r, t)| (r, t.si_magnitude()))
            .collect();
        if freq_targets.is_empty() {
            return Vec::new();
        }

        // The worst-case (highest) usable frequency gives the shortest wavelength and so the
        // tightest limit. `total_cmp` is a total order over the already finite, positive set.
        let max_valid = freq_targets
            .iter()
            .map(|(_, f)| *f)
            .filter(|f| f.is_finite() && *f > 0.0)
            .max_by(|a, b| a.total_cmp(b));
        let Some(freq_si) = max_valid else {
            // A frequency is stated but NONE is finite and positive: a malformed spectrum. Do not
            // behave as if no frequency were given (which would silently pass an electrically-long
            // design) — surface it as a blocking finding against the requirement(s) at fault, so
            // the bad datum is loud and traceable (P9 — no silent errors).
            let mut subjects: Vec<EntityId> = freq_targets.iter().map(|(r, _)| r.id).collect();
            subjects.sort();
            subjects.dedup();
            return vec![ViolationFinding {
                rule: Self::ID.to_string(),
                severity: ViolationSeverity::Error,
                subjects,
                message: "a stated operating/emission frequency is non-positive or non-finite; \
                          the EMC antenna-length analysis cannot be performed"
                    .to_string(),
            }];
        };
        let critical_len = SPEED_OF_LIGHT_M_S / (ELECTRICAL_LENGTH_DIVISOR * freq_si);
        let mut findings = Vec::new();
        for track in ctx.tracks.iter() {
            // Straight-line copper length on the SI axis. `hypot` avoids overflow and is exact
            // for the axis-aligned and diagonal single segments the router currently mints.
            let dx = track.x2.si_magnitude() - track.x1.si_magnitude();
            let dy = track.y2.si_magnitude() - track.y1.si_magnitude();
            let len = dx.hypot(dy);
            // Over the limit by more than a relative epsilon, so a track exactly at the critical
            // length (floating-point) is never a false violation — the same tolerance shape the
            // trace-width rule uses.
            let scale = len.abs().max(critical_len.abs()).max(1.0);
            if len - critical_len > 1e-9 * scale {
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![track.id],
                    message: format!(
                        "track {} length {:.1} mm exceeds the {:.1} mm electrically-long limit \
                         (1/{:.0} wavelength at {:.0} MHz)",
                        track.id.short(),
                        len * 1e3,
                        critical_len * 1e3,
                        ELECTRICAL_LENGTH_DIVISOR,
                        freq_si / 1e6
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
    use eak_domain::{BoardSide, ConstraintStatus, Priority, RequirementStatus};
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
            parts: &[],
            bom_line_items: &[],
            board: None,
            placements: &[],
            tracks: &[],
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
            parts: &[],
            bom_line_items: &[],
            board: None,
            placements: &[],
            tracks: &[],
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
            parts: &[],
            bom_line_items: &[],
            board: None,
            placements: &[],
            tracks: &[],
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
            parts: &[],
            bom_line_items: &[],
            board: None,
            placements: &[],
            tracks: &[],
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

    // ------------------------------ catalog + BOM tests ------------------------------

    fn component(id: u128, class: ComponentClass) -> Component {
        Component {
            id: EntityId(id),
            refdes: format!("U{id}"),
            class,
            value: None,
            from_block: EntityId(800 + id),
        }
    }

    fn part(id: u128, lifecycle: PartLifecycle) -> Part {
        Part {
            id: EntityId(id),
            mpn: format!("MPN-{id}"),
            manufacturer: "ACME".to_string(),
            lifecycle,
            datasheet: format!("https://acme/{id}"),
        }
    }

    fn line_item(id: u128, part_id: u128, components: Vec<u128>) -> BomLineItem {
        BomLineItem {
            id: EntityId(id),
            part: EntityId(part_id),
            components: components.into_iter().map(EntityId).collect(),
            quantity: 1,
        }
    }

    fn bom_ctx<'a>(
        components: &'a [Component],
        parts: &'a [Part],
        bom_line_items: &'a [BomLineItem],
    ) -> VerificationContext<'a> {
        VerificationContext {
            requirements: &[],
            constraints: &[],
            components,
            pins: &[],
            nets: &[],
            parts,
            bom_line_items,
            board: None,
            placements: &[],
            tracks: &[],
        }
    }

    #[test]
    fn catalog_regulator_is_deliberately_eol() {
        let cat = PartCatalog::new();
        let reg = cat.part_for(ComponentClass::Regulator);
        assert_eq!(reg.mpn, "LM1117-3.3");
        assert_eq!(reg.manufacturer, "Texas Instruments");
        assert_eq!(reg.lifecycle, PartLifecycle::Eol);
        assert_eq!(reg.datasheet, "https://ti.com/lm1117");
        // Active classes stay active.
        assert_eq!(
            cat.part_for(ComponentClass::Connector).lifecycle,
            PartLifecycle::Active
        );
        assert_eq!(cat.part_for(ComponentClass::Ic).mpn, "STM32L010F4P6");
    }

    #[test]
    fn coverage_rule_flags_uncovered_component() {
        // C1 is covered; C2 is not.
        let components = vec![
            component(1, ComponentClass::Resistor),
            component(2, ComponentClass::Capacitor),
        ];
        let parts = vec![part(50, PartLifecycle::Active)];
        let items = vec![line_item(60, 50, vec![1])];
        let findings = BomCoverageRule::new().evaluate(&bom_ctx(&components, &parts, &items));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, BomCoverageRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(2)]);
    }

    #[test]
    fn lifecycle_rule_flags_eol_line_item() {
        let components = vec![component(1, ComponentClass::Regulator)];
        let parts = vec![part(50, PartLifecycle::Eol)];
        let items = vec![line_item(60, 50, vec![1])];
        let findings = BomLifecycleRule::new().evaluate(&bom_ctx(&components, &parts, &items));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, BomLifecycleRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(60)]);
        assert!(findings[0].message.contains("MPN-50"));
    }

    #[test]
    fn lifecycle_rule_passes_all_active_bom() {
        let components = vec![component(1, ComponentClass::Resistor)];
        let parts = vec![part(50, PartLifecycle::Active)];
        let items = vec![line_item(60, 50, vec![1])];
        assert!(BomLifecycleRule::new()
            .evaluate(&bom_ctx(&components, &parts, &items))
            .is_empty());
    }

    #[test]
    fn lifecycle_rule_warns_on_nrnd() {
        let parts = vec![part(50, PartLifecycle::Nrnd)];
        let items = vec![line_item(60, 50, vec![1])];
        let findings = BomLifecycleRule::new().evaluate(&bom_ctx(&[], &parts, &items));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, ViolationSeverity::Warning);
    }

    // -------------------------------- DRC rule tests --------------------------------

    fn mm(v: f64) -> PhysicalQuantity {
        PhysicalQuantity::new(v, Unit::Millimetre)
    }

    fn board(id: u128, w: f64, h: f64) -> Board {
        Board {
            id: EntityId(id),
            width: mm(w),
            height: mm(h),
            layers: 2,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn placement(
        id: u128,
        component: u128,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        side: BoardSide,
    ) -> Placement {
        Placement {
            id: EntityId(id),
            component: EntityId(component),
            x: mm(x),
            y: mm(y),
            width: mm(w),
            height: mm(h),
            side,
        }
    }

    fn drc_ctx<'a>(
        board: Option<&'a Board>,
        placements: &'a [Placement],
    ) -> VerificationContext<'a> {
        VerificationContext {
            requirements: &[],
            constraints: &[],
            components: &[],
            pins: &[],
            nets: &[],
            parts: &[],
            bom_line_items: &[],
            board,
            placements,
            tracks: &[],
        }
    }

    #[test]
    fn placement_inside_board_passes_drc() {
        let b = board(1, 100.0, 80.0);
        // Courtyard [10,30] x [10,30] sits comfortably inside the 100x80 outline.
        let placements = vec![placement(10, 900, 10.0, 10.0, 20.0, 20.0, BoardSide::Top)];
        assert!(DrcOutOfBoundsRule::new()
            .evaluate(&drc_ctx(Some(&b), &placements))
            .is_empty());
    }

    #[test]
    fn placement_past_board_edge_is_out_of_bounds() {
        let b = board(1, 100.0, 80.0);
        // x + width = 90 + 20 = 110 > 100: the courtyard runs off the right edge.
        let placements = vec![placement(10, 900, 90.0, 10.0, 20.0, 20.0, BoardSide::Top)];
        let findings = DrcOutOfBoundsRule::new().evaluate(&drc_ctx(Some(&b), &placements));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, DrcOutOfBoundsRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(10)]);
    }

    #[test]
    fn out_of_bounds_yields_no_findings_without_a_board() {
        // No outline to fit within: the geometry rule stays silent rather than guessing.
        let placements = vec![placement(10, 900, 90.0, 10.0, 20.0, 20.0, BoardSide::Top)];
        assert!(DrcOutOfBoundsRule::new()
            .evaluate(&drc_ctx(None, &placements))
            .is_empty());
    }

    #[test]
    fn overlapping_courtyards_on_same_side_are_flagged() {
        let b = board(1, 100.0, 80.0);
        // Two [.,.+20] courtyards offset by 10mm overlap; both on Top.
        let placements = vec![
            placement(20, 900, 10.0, 10.0, 20.0, 20.0, BoardSide::Top),
            placement(10, 901, 20.0, 20.0, 20.0, 20.0, BoardSide::Top),
        ];
        let findings = DrcCourtyardOverlapRule::new().evaluate(&drc_ctx(Some(&b), &placements));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, DrcCourtyardOverlapRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        // Subjects sorted regardless of slice order (20 listed first, 10 second).
        assert_eq!(findings[0].subjects, vec![EntityId(10), EntityId(20)]);
    }

    #[test]
    fn overlapping_courtyards_on_opposite_sides_do_not_collide() {
        let b = board(1, 100.0, 80.0);
        // Same footprint, but one Top and one Bottom: opposite copper never collides.
        let placements = vec![
            placement(10, 900, 10.0, 10.0, 20.0, 20.0, BoardSide::Top),
            placement(11, 901, 20.0, 20.0, 20.0, 20.0, BoardSide::Bottom),
        ];
        assert!(DrcCourtyardOverlapRule::new()
            .evaluate(&drc_ctx(Some(&b), &placements))
            .is_empty());
    }

    #[test]
    fn courtyard_overlap_needs_no_board() {
        // Overlap is purely pairwise geometry; it does not depend on the outline.
        let placements = vec![
            placement(10, 900, 0.0, 0.0, 20.0, 20.0, BoardSide::Top),
            placement(11, 901, 5.0, 5.0, 20.0, 20.0, BoardSide::Top),
        ];
        let findings = DrcCourtyardOverlapRule::new().evaluate(&drc_ctx(None, &placements));
        assert_eq!(findings.len(), 1);
    }

    // ------------------------------ trace-width rule tests ------------------------------

    /// A Regulatory requirement carrying a length target — the fabrication process floor the
    /// trace-width rule reads.
    fn process_floor_req(min_mm: f64) -> Requirement {
        Requirement {
            id: EntityId(700),
            statement: format!("Fabrication process supports a {min_mm} mm minimum trace width"),
            category: RequirementCategory::Regulatory,
            priority: Priority::High,
            acceptance_criterion: "all traces meet the process minimum".into(),
            status: RequirementStatus::Accepted,
            source: EntityId(1),
            targets: vec![mm(min_mm)],
        }
    }

    fn track(id: u128, width_mm: f64) -> Track {
        Track {
            id: EntityId(id),
            net: EntityId(900 + id),
            layer: BoardSide::Top,
            width: mm(width_mm),
            x1: mm(1.0),
            y1: mm(1.0),
            x2: mm(9.0),
            y2: mm(1.0),
        }
    }

    fn trace_ctx<'a>(
        requirements: &'a [Requirement],
        tracks: &'a [Track],
    ) -> VerificationContext<'a> {
        VerificationContext {
            requirements,
            constraints: &[],
            components: &[],
            pins: &[],
            nets: &[],
            parts: &[],
            bom_line_items: &[],
            board: None,
            placements: &[],
            tracks,
        }
    }

    #[test]
    fn trace_finer_than_floor_is_flagged() {
        // A 0.25 mm trace cannot be etched by a 0.5 mm process.
        let reqs = vec![process_floor_req(0.5)];
        let tracks = vec![track(10, 0.25)];
        let findings = DrcTraceWidthRule::new().evaluate(&trace_ctx(&reqs, &tracks));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, DrcTraceWidthRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(10)]);
    }

    #[test]
    fn trace_meeting_floor_passes() {
        // A trace exactly at the floor (and one above it) is manufacturable.
        let reqs = vec![process_floor_req(0.25)];
        let tracks = vec![track(10, 0.25), track(11, 0.40)];
        assert!(DrcTraceWidthRule::new()
            .evaluate(&trace_ctx(&reqs, &tracks))
            .is_empty());
    }

    #[test]
    fn trace_width_is_silent_without_a_process_floor() {
        // No Regulatory length target: the process floor is unstated, so even a hair-thin trace
        // is not flagged rather than guessing a floor.
        let tracks = vec![track(10, 0.05)];
        assert!(DrcTraceWidthRule::new()
            .evaluate(&trace_ctx(&[], &tracks))
            .is_empty());
    }

    // -------------------------------- DFM rule tests --------------------------------

    fn edge_track(id: u128, x1: f64, y1: f64, x2: f64, y2: f64, width: f64) -> Track {
        Track {
            id: EntityId(id),
            net: EntityId(900 + id),
            layer: BoardSide::Top,
            width: mm(width),
            x1: mm(x1),
            y1: mm(y1),
            x2: mm(x2),
            y2: mm(y2),
        }
    }

    fn dfm_track_ctx<'a>(board: Option<&'a Board>, tracks: &'a [Track]) -> VerificationContext<'a> {
        VerificationContext {
            requirements: &[],
            constraints: &[],
            components: &[],
            pins: &[],
            nets: &[],
            parts: &[],
            bom_line_items: &[],
            board,
            placements: &[],
            tracks,
        }
    }

    #[test]
    fn placement_well_inside_passes_dfm_edge_clearance() {
        let b = board(1, 100.0, 80.0);
        // Courtyard [10,30] x [10,30] is far from every edge (keep-out is 0.5 mm).
        let placements = vec![placement(10, 900, 10.0, 10.0, 20.0, 20.0, BoardSide::Top)];
        assert!(DfmEdgeClearanceRule::new()
            .evaluate(&drc_ctx(Some(&b), &placements))
            .is_empty());
    }

    #[test]
    fn placement_hugging_edge_is_flagged_but_still_fits() {
        let b = board(1, 100.0, 80.0);
        // x + width = 79.9 + 20 = 99.9: inside the 100 mm outline (so DRC out-of-bounds passes),
        // but only 0.1 mm from the right edge — inside the 0.5 mm assembly keep-out.
        let placements = vec![placement(10, 900, 79.9, 10.0, 20.0, 20.0, BoardSide::Top)];
        assert!(DrcOutOfBoundsRule::new()
            .evaluate(&drc_ctx(Some(&b), &placements))
            .is_empty());
        let findings = DfmEdgeClearanceRule::new().evaluate(&drc_ctx(Some(&b), &placements));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, DfmEdgeClearanceRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(10)]);
    }

    #[test]
    fn edge_clearance_yields_no_findings_without_a_board() {
        // No outline: there is no edge to measure a keep-out from, so the rule stays silent.
        let placements = vec![placement(10, 900, 0.0, 0.0, 20.0, 20.0, BoardSide::Top)];
        assert!(DfmEdgeClearanceRule::new()
            .evaluate(&drc_ctx(None, &placements))
            .is_empty());
    }

    #[test]
    fn trace_well_inside_passes_dfm_edge_clearance() {
        let b = board(1, 100.0, 80.0);
        // A horizontal trace from (10,40) to (90,40), 0.25 mm wide: comfortably inside.
        let tracks = vec![edge_track(10, 10.0, 40.0, 90.0, 40.0, 0.25)];
        assert!(DfmTraceEdgeClearanceRule::new()
            .evaluate(&dfm_track_ctx(Some(&b), &tracks))
            .is_empty());
    }

    #[test]
    fn trace_copper_hugging_edge_is_flagged() {
        let b = board(1, 100.0, 80.0);
        // An endpoint at x = 0.2 mm with a 0.25 mm trace puts the copper edge at 0.075 mm —
        // inside the 0.5 mm board-edge keep-out.
        let tracks = vec![edge_track(10, 0.2, 40.0, 90.0, 40.0, 0.25)];
        let findings = DfmTraceEdgeClearanceRule::new().evaluate(&dfm_track_ctx(Some(&b), &tracks));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, DfmTraceEdgeClearanceRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(10)]);
    }

    #[test]
    fn trace_edge_clearance_yields_no_findings_without_a_board() {
        let tracks = vec![edge_track(10, 0.1, 0.1, 90.0, 0.1, 0.25)];
        assert!(DfmTraceEdgeClearanceRule::new()
            .evaluate(&dfm_track_ctx(None, &tracks))
            .is_empty());
    }

    // -------------------------------- EMC rule tests --------------------------------

    /// A requirement carrying a frequency target — the highest operating/emission frequency the
    /// antenna-length rule sizes its critical length against. The rule scans EVERY requirement's
    /// targets, so the category is irrelevant; `Electrical` stands in for a high-speed signal.
    fn freq_req(mhz: f64) -> Requirement {
        Requirement {
            id: EntityId(710),
            statement: format!("High-speed interface operates at {mhz} MHz"),
            category: RequirementCategory::Electrical,
            priority: Priority::High,
            acceptance_criterion: "emissions assessed at the stated frequency".into(),
            status: RequirementStatus::Accepted,
            source: EntityId(1),
            targets: vec![PhysicalQuantity::new(mhz, Unit::Megahertz)],
        }
    }

    fn emc_ctx<'a>(
        requirements: &'a [Requirement],
        tracks: &'a [Track],
    ) -> VerificationContext<'a> {
        VerificationContext {
            requirements,
            constraints: &[],
            components: &[],
            pins: &[],
            nets: &[],
            parts: &[],
            bom_line_items: &[],
            board: None,
            placements: &[],
            tracks,
        }
    }

    #[test]
    fn track_longer_than_a_tenth_wavelength_is_flagged() {
        // `track` spans (1,1) -> (9,1) mm = 8 mm of copper. At 10 GHz the free-space wavelength
        // is 30 mm, so the lambda/10 critical length is 3 mm: an 8 mm trace is electrically long.
        let reqs = vec![freq_req(10_000.0)];
        let tracks = vec![track(10, 0.25)];
        let findings = EmcAntennaLengthRule::new().evaluate(&emc_ctx(&reqs, &tracks));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, EmcAntennaLengthRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(10)]);
    }

    #[test]
    fn electrically_short_track_passes() {
        // At 100 MHz the wavelength is 3 m, so the critical length is 300 mm: an 8 mm trace is
        // electrically short and well within the limit.
        let reqs = vec![freq_req(100.0)];
        let tracks = vec![track(10, 0.25)];
        assert!(EmcAntennaLengthRule::new()
            .evaluate(&emc_ctx(&reqs, &tracks))
            .is_empty());
    }

    #[test]
    fn antenna_rule_is_silent_without_a_stated_frequency() {
        // No frequency target: the emission spectrum is unstated, so even a long trace is not
        // flagged rather than guessing a frequency — paralleling the trace-width rule's silence
        // without a process floor.
        let tracks = vec![track(10, 0.25)];
        assert!(EmcAntennaLengthRule::new()
            .evaluate(&emc_ctx(&[], &tracks))
            .is_empty());
    }

    #[test]
    fn antenna_rule_sizes_its_limit_from_the_highest_frequency() {
        // Two frequencies stated; the worst case (10 GHz -> 3 mm) governs, under which the 8 mm
        // trace fails — even though the 100 MHz limit (300 mm) alone would pass it.
        let reqs = vec![freq_req(100.0), freq_req(10_000.0)];
        let tracks = vec![track(10, 0.25)];
        let findings = EmcAntennaLengthRule::new().evaluate(&emc_ctx(&reqs, &tracks));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].subjects, vec![EntityId(10)]);
    }

    #[test]
    fn track_at_exactly_the_critical_length_is_not_flagged() {
        // Pin the documented guarantee: a trace exactly at lambda/10 is never a false positive.
        // Build a horizontal track whose length equals the critical length at 10 GHz to f64
        // precision, computed from the SAME constants the rule uses (so the test tracks the impl).
        let freq_si = 10_000.0_f64 * 1e6; // 10 GHz in Hz
        let critical_mm = (SPEED_OF_LIGHT_M_S / (ELECTRICAL_LENGTH_DIVISOR * freq_si)) * 1e3;
        let reqs = vec![freq_req(10_000.0)];
        let exact = Track {
            id: EntityId(10),
            net: EntityId(910),
            layer: BoardSide::Top,
            width: mm(0.25),
            x1: mm(0.0),
            y1: mm(0.0),
            x2: mm(critical_mm),
            y2: mm(0.0),
        };
        assert!(EmcAntennaLengthRule::new()
            .evaluate(&emc_ctx(&reqs, &[exact]))
            .is_empty());
    }

    #[test]
    fn malformed_frequency_is_surfaced_not_silently_skipped() {
        // A frequency target is stated but its magnitude is non-positive (0 MHz). The design must
        // NOT pass EMC silently as if no frequency were given — the bad spectrum is flagged as a
        // blocking error against the offending requirement, even though the tracks are never
        // measured against a (here, undefined) limit.
        let reqs = vec![freq_req(0.0)];
        let tracks = vec![track(10, 0.25)];
        let findings = EmcAntennaLengthRule::new().evaluate(&emc_ctx(&reqs, &tracks));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, EmcAntennaLengthRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(710)]); // the offending requirement
    }
}
