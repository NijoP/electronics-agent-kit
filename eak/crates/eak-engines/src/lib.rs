//! Domain engines (deterministic services). Phase 1 shipped the Planning Engine; Phase 2
//! adds the [`ConstraintEngine`] (does a value satisfy a bound? do two bounds contradict?)
//! and a generic [`VerificationEngine`] — a [`Rule`] registry whose first rule,
//! [`ConstraintConsistencyRule`], catches mutually-unsatisfiable constraints. ERC/DRC/DFM
//! are future rules over the same framework. All engines are pure and deterministic.
//! See `docs/engineering/constraint-engine.md` and `docs/engineering/verification-engine.md`.

use eak_domain::{
    Board, BoardSide, BomLineItem, Component, ComponentClass, Constraint, ConstraintKind, EntityId,
    Layer, LayerStack, Net, NetClass, Part, PartLifecycle, Pin, PinElectricalType, Placement,
    Requirement, RequirementCategory, Track, Violation, ViolationSeverity,
};
use eak_units::{Dimension, PhysicalQuantity, Unit, UnitError};
use std::cmp::Ordering;
use std::collections::BTreeSet;

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

/// The Length-dimensioned targets stated on [`Fabrication`](RequirementCategory::Fabrication)
/// requirements, in commit order. Several fab-process floors share this one target space through a
/// documented POSITIONAL SLOT CONTRACT (no typed discriminator yet): slot 0 = the minimum trace
/// width ([`DrcTraceWidthRule`]), slot 1 = the board-edge keep-out band
/// ([`resolve_edge_keepout_si`], used by the DFM rules), slot 2 = the minimum copper-to-copper
/// spacing ([`DrcCopperClearanceRule`]). A caller takes its slot by position; an unstated slot
/// either stays silent (slots 0 and 2 emit nothing rather than guess a floor) or falls back to a
/// constant (slot 1's keep-out), so a lone trace-width target never weakens another floor and
/// vice-versa. A future increment may replace the slots with a typed target role.
fn fabrication_length_targets(reqs: &[Requirement]) -> impl Iterator<Item = &PhysicalQuantity> {
    reqs.iter()
        .filter(|r| r.category == RequirementCategory::Fabrication)
        .flat_map(|r| r.targets.iter())
        .filter(|t| t.dimension() == Dimension::Length)
}

/// The board-edge keep-out band in SI metres: the SECOND Fabrication Length target (slot 1) when
/// stated, else the [`DFM_EDGE_CLEARANCE_FALLBACK_MM`] constant. Shared by both DFM edge rules so
/// the placement keep-out and the copper keep-out always agree.
fn resolve_edge_keepout_si(reqs: &[Requirement]) -> f64 {
    fabrication_length_targets(reqs)
        .nth(1)
        .map(|t| t.si_magnitude())
        .unwrap_or(DFM_EDGE_CLEARANCE_FALLBACK_MM * 1e-3)
}

/// DRC rule: every routed [`Track`]'s copper `width` must be at least the design's minimum
/// manufacturable trace width — the fabrication process floor. The floor is the FIRST
/// length-dimensioned target on a [`Fabrication`](RequirementCategory::Fabrication)
/// requirement (a process limit, e.g. an IPC trace-width class), mirroring how floor planning
/// takes the board edge from the first Mechanical length target. A trace finer than the floor
/// cannot be etched by the chosen process, so it is an Error. With no such requirement there is
/// no stated process floor, so the rule emits nothing rather than guessing one — a design that
/// has not pinned a process is not spuriously failed. Comparisons are on the SI axis via
/// `si_magnitude()` so the check is unit-independent (P9); tracks are scanned in slice order for
/// determinism.
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
        // The process floor: slot 0 of the Fabrication Length targets (the first, in commit
        // order). Absent one, there is no floor to check against.
        let Some(floor) = fabrication_length_targets(ctx.requirements).next() else {
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

/// DRC rule: every committed [`Net`] must be realized by at least one routed [`Track`]. An
/// unrouted net carries no copper, so the nodes it joins are not actually connected on the board
/// — an electrical break. Routing Planning realizes at least one track per net (a daisy-chain of
/// segments over its member pads), so in a complete layout this rule is silent; it is the
/// downstream check that makes net-realization *completeness* a first-class, traceable
/// [`Violation`](eak_domain::Violation) rather than resting solely on the upstream "every
/// component is placed" invariant (a regression guard, since DRC runs only after Routing). This
/// rule checks only that a net carries *some* copper; that the copper actually *joins* every pad
/// is [`DrcNetOpenRule`]'s concern. A flagged net implicates itself
/// (`Violation -> Net -> ... -> Requirement -> Intent`).
/// Nets are scanned in slice order for determinism.
pub struct DrcUnroutedNetRule;

impl DrcUnroutedNetRule {
    pub const ID: &'static str = "drc-unrouted-net";

    pub fn new() -> Self {
        Self
    }
}
impl Default for DrcUnroutedNetRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DrcUnroutedNetRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        let mut findings = Vec::new();
        for net in ctx.nets.iter() {
            let routed = ctx.tracks.iter().any(|t| t.net == net.id);
            if !routed {
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![net.id],
                    message: format!(
                        "net \"{}\" ({}) is not realized by any routed track",
                        net.name,
                        net.id.short()
                    ),
                });
            }
        }
        findings
    }
}

/// The SI tolerance (in metres, ~1 nm) for matching a track endpoint to a placement centroid. The
/// router mints each endpoint AT a member's courtyard centroid, but through a millimetre
/// round-trip (`(si * 1000) mm`, read back via `si_magnitude`), so an epsilon match decouples the
/// connectivity check from exact float reproduction while staying deterministic — the same
/// tolerance shape the trace-width and antenna-length rules use.
const CENTROID_MATCH_EPS_SI: f64 = 1e-9;

/// A placement's courtyard centroid on the SI axis, as `(x, y)` in metres: origin + half extent,
/// the very point the router routes a segment endpoint to.
fn placement_centroid_si(p: &Placement) -> (f64, f64) {
    (
        p.x.si_magnitude() + p.width.si_magnitude() / 2.0,
        p.y.si_magnitude() + p.height.si_magnitude() / 2.0,
    )
}

/// Resolve a track endpoint (SI coordinates) to the index of the `nodes` placement whose courtyard
/// centroid it matches within [`CENTROID_MATCH_EPS_SI`], or `None` if it matches none. Scans
/// `nodes` in order, so the result is deterministic.
fn endpoint_node(px: f64, py: f64, nodes: &[&Placement]) -> Option<usize> {
    nodes.iter().position(|p| {
        let (cx, cy) = placement_centroid_si(p);
        (cx - px).abs() <= CENTROID_MATCH_EPS_SI && (cy - py).abs() <= CENTROID_MATCH_EPS_SI
    })
}

/// A minimal disjoint-set (union-find) over a fixed node set, indexed `0..n`. Used by
/// [`DrcNetOpenRule`] to coalesce a net's member placements joined by its tracks; the number of
/// distinct roots that remain is the count of disconnected copper components. Deterministic:
/// `find`/`union` depend only on the union order, which is the net's slice-ordered tracks.
struct DisjointSet {
    parent: Vec<usize>,
}

impl DisjointSet {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]]; // path halving
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[ra] = rb;
        }
    }

    /// The number of distinct sets (connected components) over the `0..n` nodes.
    fn component_count(&mut self) -> usize {
        let n = self.parent.len();
        let mut roots: BTreeSet<usize> = BTreeSet::new();
        for i in 0..n {
            let r = self.find(i);
            roots.insert(r);
        }
        roots.len()
    }
}

/// DRC rule: every committed [`Net`] whose pads span more than one placement must be electrically
/// *connected* by its copper — all member pads must fall in a single connected component of the
/// net's tracks. This is the dual of [`DrcUnroutedNetRule`]: that rule asks whether a net carries
/// *any* copper; this asks whether the copper actually *joins* the pads. A net whose tracks leave
/// a member pad in its own island is electrically OPEN — a hard break — so it is a blocking Error.
///
/// ALGORITHM (per net, scanned in slice order for determinism): the NODES are the distinct
/// placements hosting the net's member pins (a member pin -> its component -> that component's
/// placement), collected in a [`BTreeSet`] so the node order is reproducible. The EDGES are the
/// net's own tracks; each track endpoint is resolved to the node placement whose courtyard
/// centroid it matches (within [`CENTROID_MATCH_EPS_SI`] — the router mints endpoints at those
/// centroids) and the two matched nodes are unioned in a [`DisjointSet`]. The net is open when its
/// member placements span more than one set once all its tracks are folded in. A net with fewer
/// than two distinct placements is trivially connected (skipped), and a net with NO track is left
/// to [`DrcUnroutedNetRule`] (skipped here) so the two rules never double-flag the same break.
///
/// SCOPE: this detects OPENS only, not SHORTS. In this data model a [`Track`] carries only a net
/// id and endpoints (no pad refs) and a single placement hosts pins from several nets (a connector
/// sits on both VBUS and GND), so a global placement-node graph would report a false short on
/// every multi-net component. Copper-to-copper SHORT detection is a separate, geometric concern
/// (the copper-clearance rule), not this rule's.
pub struct DrcNetOpenRule;

impl DrcNetOpenRule {
    pub const ID: &'static str = "drc-net-open";

    pub fn new() -> Self {
        Self
    }
}
impl Default for DrcNetOpenRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DrcNetOpenRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        let mut findings = Vec::new();
        for net in ctx.nets.iter() {
            // NODES: the distinct placements hosting this net's member pins, in a deterministic
            // (sorted) order. A dangling member pin or unplaced component contributes no node —
            // those gaps are caught at their own seams/rules, not here.
            let mut node_ids: BTreeSet<EntityId> = BTreeSet::new();
            for pin_id in &net.members {
                let Some(pin) = ctx.pins.iter().find(|p| p.id == *pin_id) else {
                    continue;
                };
                if let Some(pl) = ctx
                    .placements
                    .iter()
                    .find(|pl| pl.component == pin.component)
                {
                    node_ids.insert(pl.id);
                }
            }
            // Fewer than two distinct placements: the net cannot be open (it has at most one pad).
            if node_ids.len() < 2 {
                continue;
            }
            let nodes: Vec<&Placement> = node_ids
                .iter()
                .filter_map(|id| ctx.placements.iter().find(|p| p.id == *id))
                .collect();

            // EDGES: this net's tracks. A net with no copper at all is left to
            // DrcUnroutedNetRule so the two rules never double-flag the same break.
            let net_tracks: Vec<&Track> = ctx.tracks.iter().filter(|t| t.net == net.id).collect();
            if net_tracks.is_empty() {
                continue;
            }

            // Union the node placements joined by each track segment.
            let mut dsu = DisjointSet::new(nodes.len());
            for t in &net_tracks {
                let a = endpoint_node(t.x1.si_magnitude(), t.y1.si_magnitude(), &nodes);
                let b = endpoint_node(t.x2.si_magnitude(), t.y2.si_magnitude(), &nodes);
                if let (Some(a), Some(b)) = (a, b) {
                    dsu.union(a, b);
                }
            }

            let components = dsu.component_count();
            if components > 1 {
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![net.id],
                    message: format!(
                        "net \"{}\" ({}) is electrically open: member pads fall in {} disconnected copper components",
                        net.name,
                        net.id.short(),
                        components
                    ),
                });
            }
        }
        findings
    }
}

/// Twice the signed area of triangle `(a, b, c)` — positive when `a -> b -> c` turns
/// counter-clockwise, zero when the three points are colinear. The orientation primitive behind
/// the proper-crossing test in [`segment_distance`].
fn orient(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

/// The Euclidean distance from point `p` to the finite segment `s` in the SI plane (metres). `p`
/// is projected onto the segment with the projection parameter clamped to `[0, 1]`, so when the
/// foot of the perpendicular falls beyond an end the nearest point is that endpoint; a degenerate
/// (zero-length) segment reduces to the point-to-point distance.
fn point_segment_distance(p: (f64, f64), s: ((f64, f64), (f64, f64))) -> f64 {
    let (a, b) = s;
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq <= 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len_sq).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

/// The minimum Euclidean distance between two finite segments `a` and `b` in the SI plane
/// (metres), used by [`DrcCopperClearanceRule`] to gauge the gap between two trace centre-lines.
/// Zero when the segments cross or touch: a *proper* (transversal) crossing is caught by the
/// orientation test, while a touch/overlap (including colinear overlap, e.g. the stub router's
/// identical different-net segments) surfaces as a zero endpoint-to-segment distance. Otherwise
/// the closest pair includes an endpoint, so the result is the least of the four
/// endpoint-to-segment distances. Pure and deterministic — closed form, no iteration.
fn segment_distance(a: ((f64, f64), (f64, f64)), b: ((f64, f64), (f64, f64))) -> f64 {
    let (a1, a2) = a;
    let (b1, b2) = b;
    // Proper crossing: a1/a2 strictly straddle line b AND b1/b2 strictly straddle line a.
    let (d1, d2) = (orient(b1, b2, a1), orient(b1, b2, a2));
    let (d3, d4) = (orient(a1, a2, b1), orient(a1, a2, b2));
    let straddles_b = (d1 > 0.0) != (d2 > 0.0) && d1 != 0.0 && d2 != 0.0;
    let straddles_a = (d3 > 0.0) != (d4 > 0.0) && d3 != 0.0 && d4 != 0.0;
    if straddles_a && straddles_b {
        return 0.0;
    }
    point_segment_distance(a1, b)
        .min(point_segment_distance(a2, b))
        .min(point_segment_distance(b1, a))
        .min(point_segment_distance(b2, a))
}

/// DRC rule: two copper traces on the SAME side belonging to DIFFERENT nets must keep at least the
/// design's minimum copper-to-copper spacing — the fabrication clearance floor. Two conductors
/// closer than the process can reliably etch and isolate risk an acid-trap or a solder bridge — a
/// hard short between distinct nets — so it is a blocking Error. It is the co-equal sibling of
/// [`DrcTraceWidthRule`] (a per-track floor) and the geometric counterpart to [`DrcNetOpenRule`]'s
/// topological open check: that rule asks whether a net's copper joins its own pads; this asks
/// whether one net's copper strays too near another's.
///
/// SPACING SOURCE: slot 2 of the [`fabrication_length_targets`] positional contract. With NO such
/// target stated there is no clearance floor to check against, so the rule emits nothing rather
/// than guessing one — mirroring [`DrcTraceWidthRule`]'s "no floor -> silent". Crucially there is
/// NO constant fallback: the stub router mints identical, overlapping centre-lines for the
/// different nets that connect the same component pair (every member centroid shares `y = MARGIN`),
/// a true geometric coincidence that any non-zero floor would (correctly) flag — so a fallback
/// would fail the released golden board. A design opts into the check by pinning the floor.
///
/// GEOMETRY: pairs scanned `i < j` over the tracks (slice order, deterministic). A pair is skipped
/// when the tracks sit on opposite [`BoardSide`] layers (opposite copper never shorts — as in
/// [`DrcCourtyardOverlapRule`]) or share a net (a net's own daisy segments and branches may touch
/// — the same-net exemption). Otherwise the edge-to-edge gap is the centre-line
/// [`segment_distance`] less each trace's half-width; the pair is flagged when it falls below the
/// floor by more than an epsilon (the same tolerance shape [`DrcTraceWidthRule`] and the EMC rule
/// use, so copper exactly at the floor is never a false positive). All comparisons are on the SI
/// axis via `si_magnitude()` (P9). A flagged pair implicates both tracks, listed as a sorted
/// subject vector so the dedup key is stable regardless of pair order
/// (`Violation -> Track -> Net -> ... -> Requirement -> Intent`).
///
/// SCOPE: IPC-2221 scales the minimum spacing with the peak voltage between the two conductors,
/// but the model carries no per-net voltage, so the rule applies the stated slot-2 floor flat.
/// Voltage-derived scaling is a noted future refinement once a net carries a voltage (the same
/// deferred-reasoning boundary as the EMC velocity factor), not a fabricated figure.
pub struct DrcCopperClearanceRule;

impl DrcCopperClearanceRule {
    pub const ID: &'static str = "drc-copper-clearance";

    pub fn new() -> Self {
        Self
    }
}
impl Default for DrcCopperClearanceRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DrcCopperClearanceRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        // The clearance floor: slot 2 of the Fabrication Length targets. Absent one, there is no
        // floor to check against — and deliberately NO constant fallback (see the type doc-comment).
        let Some(min_space) = fabrication_length_targets(ctx.requirements).nth(2) else {
            return Vec::new();
        };
        let min_space_si = min_space.si_magnitude();
        let mut findings = Vec::new();
        let tracks = ctx.tracks;
        for i in 0..tracks.len() {
            for j in (i + 1)..tracks.len() {
                let (a, b) = (&tracks[i], &tracks[j]);
                // Opposite copper sides never short; a net's own copper may touch (same-net exempt).
                if a.layer != b.layer || a.net == b.net {
                    continue;
                }
                let seg_a = (
                    (a.x1.si_magnitude(), a.y1.si_magnitude()),
                    (a.x2.si_magnitude(), a.y2.si_magnitude()),
                );
                let seg_b = (
                    (b.x1.si_magnitude(), b.y1.si_magnitude()),
                    (b.x2.si_magnitude(), b.y2.si_magnitude()),
                );
                // Edge-to-edge gap: centre-line distance less each trace's half-width.
                let gap = segment_distance(seg_a, seg_b)
                    - (a.width.si_magnitude() + b.width.si_magnitude()) / 2.0;
                // Below the floor by more than an epsilon, so copper exactly at the floor is never
                // a false violation. `scale` clamps to 1 m; spacings are sub-millimetre, so in
                // practice this is an absolute ~1 nm tolerance.
                let scale = min_space_si.abs().max(gap.abs()).max(1.0);
                if min_space_si - gap > 1e-9 * scale {
                    let mut subjects = vec![a.id, b.id];
                    subjects.sort(); // stable dedup key regardless of pair order
                    findings.push(ViolationFinding {
                        rule: Self::ID.to_string(),
                        severity: ViolationSeverity::Error,
                        subjects,
                        message: format!(
                            "tracks {} and {} are closer than the {} copper-to-copper clearance",
                            a.id.short(),
                            b.id.short(),
                            min_space
                        ),
                    });
                }
            }
        }
        findings
    }
}

// IPC-2221 external-layer current-capacity curve `I = k·ΔT^0.44·A^0.725` (A in mil², ΔT in °C,
// I in A), inverted to size the copper. Routing lays every Track on an OUTER `BoardSide`
// (Top/Bottom), so the external coefficient applies and the internal-layer 0.024 is unused today.
const AMPACITY_K_EXTERNAL: f64 = 0.048;
const AMPACITY_DELTA_T_EXP: f64 = 0.44; // b — the ΔT exponent
const AMPACITY_AREA_EXP: f64 = 0.725; // c — the cross-section exponent
/// The conservative default allowable copper temperature rise, in °C, for the self-heating floor.
/// A fixed first-order DC proxy (IPC-2221), documented as such — a denser design may allow less; a
/// supplied per-net rise is a later refinement, not a fabricated figure (the same deferred-input
/// discipline the copper-clearance voltage scaling and EMC velocity factor follow).
const AMPACITY_TEMP_RISE_C: f64 = 10.0;
/// mil² → mm²: one mil is 0.0254 mm, so one mil² is `0.0254² = 6.4516e-4` mm².
const MIL2_TO_MM2: f64 = 6.4516e-4;

/// The minimum external-copper trace width, in millimetres, that carries `current_a` amperes
/// within the conservative self-heating rise: the IPC-2221 curve `I = k·ΔT^0.44·A^0.725` inverted
/// for the cross-section `A`, then divided by the copper thickness to get a width. `thickness_mm`
/// is the layer copper thickness read from the board's [`LayerStack`] — the increment-12 keystone:
/// without a real `t` this floor is uncomputable in principle, not merely unchecked. A first-order
/// DC proxy at a fixed 10 °C rise on an external layer; the IR-drop floor is a separate future term
/// (it needs a per-net voltage budget and a source-to-load path length). Pure and deterministic.
fn ampacity_min_width_mm(current_a: f64, thickness_mm: f64) -> f64 {
    let denom = AMPACITY_K_EXTERNAL * AMPACITY_TEMP_RISE_C.powf(AMPACITY_DELTA_T_EXP);
    let area_mil2 = (current_a / denom).powf(1.0 / AMPACITY_AREA_EXP);
    let area_mm2 = area_mil2 * MIL2_TO_MM2;
    area_mm2 / thickness_mm
}

/// The copper thickness, in millimetres, of the stack layer a [`Track`] on `side` rides on.
/// Routing lays tracks on an outer [`BoardSide`], and a [`LayerStack`] is ordered top→bottom, so
/// `Top` maps to the first (top) layer and `Bottom` to the last (bottom) layer. `None` when the
/// stack carries no layers (a malformed board — caught by `Board::validate`).
fn layer_copper_thickness_mm(stack: &LayerStack, side: BoardSide) -> Option<f64> {
    let layer = match side {
        BoardSide::Top => stack.layers.first(),
        BoardSide::Bottom => stack.layers.last(),
    }?;
    Some(layer.copper_thickness.si_magnitude() * 1e3)
}

/// DRC rule: every routed [`Track`] whose [`Net`] states a `current` must have copper `width` at
/// least the net's ampacity floor — the width that keeps that current inside a conservative
/// self-heating temperature rise (IPC-2221, `engineering-science/electrical/ohms-law.md` §6). The
/// floor is [`ampacity_min_width_mm`] evaluated on the net's stated current and the track's layer
/// copper thickness from the board [`LayerStack`] (the increment-12 keystone paying off: a real `t`
/// makes `A = w·t` computable). A track finer than the floor cannot carry its net's current without
/// exceeding the rise — a rail that overheats — so it is a blocking Error.
///
/// CURRENT SOURCE: the per-net [`Net::current`]. A net that states NO current gets no DC ampacity
/// floor and the rule emits nothing for it rather than inventing a current — the per-net form of
/// [`DrcTraceWidthRule`]'s and [`DrcCopperClearanceRule`]'s "no input → silent" discipline. With no
/// board there is no stack to read `t` from, so the rule is likewise silent. The manufacturing
/// process floor stays independently enforced by [`DrcTraceWidthRule`] (slot 0), so the effective
/// combined floor is `max(ampacity, process)`; the IR-drop term is a documented future refinement.
///
/// This is a CHECKER, not an actuator: Routing still stamps a per-class default width, so a
/// current-carrying net finer than its floor is flagged and looped back (the generic gate), exactly
/// as the trace-width rule behaves — sizing the width to satisfy the floor is the separate
/// closed-loop-actuation move. Comparisons are on the SI axis via `si_magnitude()` with the same
/// `1e-9·scale` epsilon the other width/clearance rules use, so copper exactly at the floor is never
/// a false positive; tracks are scanned in slice order for determinism (P4). A flagged track
/// implicates itself (`Violation → Track → Net → … → Requirement → Intent`, P3).
pub struct DrcAmpacityWidthRule;

impl DrcAmpacityWidthRule {
    pub const ID: &'static str = "drc-ampacity-width";

    pub fn new() -> Self {
        Self
    }
}
impl Default for DrcAmpacityWidthRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DrcAmpacityWidthRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        // Copper thickness comes from the board's LayerStack; with no board there is no stack to
        // size against, so the rule stays silent (mirrors the geometry rules).
        let Some(board) = ctx.board else {
            return Vec::new();
        };
        let mut findings = Vec::new();
        for track in ctx.tracks.iter() {
            // Only nets that STATE a current get a DC ampacity floor; absent one, emit nothing
            // rather than inventing a current (the per-net "no input → silent" discipline).
            let Some(net) = ctx.nets.iter().find(|n| n.id == track.net) else {
                continue;
            };
            let Some(current) = net.current.as_ref() else {
                continue;
            };
            let current_a = current.si_magnitude();
            if !current_a.is_finite() || current_a <= 0.0 {
                continue; // Net::validate rejects these; defensive skip keeps the rule total.
            }
            let Some(thickness_mm) = layer_copper_thickness_mm(&board.stack, track.layer) else {
                continue;
            };
            let floor_mm = ampacity_min_width_mm(current_a, thickness_mm);
            let floor_si = floor_mm * 1e-3;
            let width_si = track.width.si_magnitude();
            let scale = width_si.abs().max(floor_si.abs()).max(1.0);
            if floor_si - width_si > 1e-9 * scale {
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![track.id],
                    message: format!(
                        "track {} width {} is finer than the {} ampacity floor for net {}'s {} load",
                        track.id.short(),
                        track.width,
                        PhysicalQuantity::new(floor_mm, Unit::Millimetre),
                        net.id.short(),
                        current,
                    ),
                });
            }
        }
        findings
    }
}

// ===== Controlled-impedance microstrip model (IPC-2141 / stackup.md clause 3) =====

/// The IPC-2141 microstrip constant `87` in `Z₀ ≈ (87/√(ε_r+1.41))·ln(5.98h/(0.8w+t))`.
const MICROSTRIP_C0: f64 = 87.0;
/// The `+1.41` effective-permittivity offset under the square root.
const MICROSTRIP_ER_OFFSET: f64 = 1.41;
/// The `5.98` height coefficient in the log argument.
const MICROSTRIP_H_COEFF: f64 = 5.98;
/// The `0.8` width coefficient in the log argument.
const MICROSTRIP_W_COEFF: f64 = 0.8;
/// The realized-vs-target characteristic-impedance tolerance band: a routed width whose realized
/// Z₀ deviates from the net's target by more than ±10 % is a controlled-impedance violation
/// (`engineering-science/electrical/transmission-lines.md`; the `±10 %` typed constraint band).
const IMPEDANCE_TOLERANCE: f64 = 0.10;

/// The characteristic impedance Z₀ (Ω) of a microstrip trace of width `w_mm` and copper thickness
/// `t_mm` over a dielectric of height `h_mm` and relative permittivity `er`, via the IPC-2141
/// closed form (`stackup.md` clause 3). All three lengths must be in the same unit (the log
/// argument is dimensionless). Z₀ is strictly decreasing in `w`, so there is exactly one width per
/// stack that hits a target — see [`microstrip_width`], its inverse. First-order engineering
/// approximation, not a field solver; adequate for a first-pass width, and the realized Z₀ the
/// verification rule reports back.
pub fn microstrip_z0(er: f64, h_mm: f64, w_mm: f64, t_mm: f64) -> f64 {
    (MICROSTRIP_C0 / (er + MICROSTRIP_ER_OFFSET).sqrt())
        * (MICROSTRIP_H_COEFF * h_mm / (MICROSTRIP_W_COEFF * w_mm + t_mm)).ln()
}

/// The microstrip trace width (mm) that realizes the target characteristic impedance `z0` (Ω) on a
/// stack of dielectric height `h_mm`, permittivity `er`, and copper thickness `t_mm` — the inverse
/// of [`microstrip_z0`]: `w = (5.98·h·e^(−E) − t)/0.8` with `E = z0·√(er+1.41)/87`. Returns `None`
/// when the result is non-physical (`w ≤ 0`), which means the target lies above the stack's
/// achievable impedance ceiling `Z₀_max = (87/√(er+1.41))·ln(5.98h/t)` — the dielectric is too thin
/// (or too few layers) to realize this Z₀ at any positive width. That is the recoverable
/// "stack-up infeasible for the impedance target" case (`stackup.md` clause 7), NOT a value to
/// clamp silently.
pub fn microstrip_width(z0: f64, er: f64, h_mm: f64, t_mm: f64) -> Option<f64> {
    let e = z0 * (er + MICROSTRIP_ER_OFFSET).sqrt() / MICROSTRIP_C0;
    let w = (MICROSTRIP_H_COEFF * h_mm * (-e).exp() - t_mm) / MICROSTRIP_W_COEFF;
    (w.is_finite() && w > 0.0).then_some(w)
}

/// The voltage reflection coefficient `Γ = (Z_real − Z_target)/(Z_real + Z_target)` — the
/// physically-meaningful measure of an impedance mismatch (`transmission-lines.md`): `Γ = 0` is a
/// matched, invisible line; `|Γ|` grows with the mismatch. Used to describe a controlled-impedance
/// violation.
pub fn reflection_gamma(z0_real: f64, z0_target: f64) -> f64 {
    (z0_real - z0_target) / (z0_real + z0_target)
}

/// The stack layer a [`Track`] on `side` references for its microstrip model: routing lays tracks
/// on an outer [`BoardSide`], and a [`LayerStack`] is ordered top→bottom, so `Top` references the
/// first (top) layer and `Bottom` the last. The layer carries the microstrip `h` (its
/// `dielectric_height`), `ε_r`, and `t` (its `copper_thickness`). `None` when the stack is empty.
fn reference_layer_for_side(stack: &LayerStack, side: BoardSide) -> Option<&Layer> {
    match side {
        BoardSide::Top => stack.layers.first(),
        BoardSide::Bottom => stack.layers.last(),
    }
}

/// DRC rule: every routed [`Track`] whose [`Net`] declares an `impedance_target` must realize a
/// characteristic impedance within ±10 % of that target. The realized Z₀ is [`microstrip_z0`]
/// evaluated on the track's width and its reference stack layer's `ε_r`/`h`/`t`; the deviation is
/// reported as the reflection coefficient [`reflection_gamma`]. A width whose Z₀ strays past the
/// tolerance guarantees reflections (`Γ ≠ 0`) on a line that was declared controlled, so it is a
/// blocking Error. Routing sizes a controlled net's width from [`microstrip_width`], so on a
/// feasible stack this rule is silent; it fires when the target is infeasible for the stack (routing
/// fell back to the class default) or a hand-set width drifts — the check that makes the impedance
/// target *enforced* rather than merely declared.
///
/// TARGET SOURCE: the per-net [`Net::impedance_target`]. A net that declares no target is
/// uncontrolled and the rule emits nothing for it (the per-net "no input → silent" discipline); with
/// no board there is no stack to model the microstrip against, so the rule is likewise silent.
/// Comparisons use the relative ±10 % band (a fractional tolerance, not the SI-axis nm epsilon the
/// geometry rules use, because impedance is not a length); tracks are scanned in slice order for
/// determinism (P4). A flagged track implicates itself
/// (`Violation → Track → Net → … → Requirement → Intent`, P3).
pub struct DrcImpedanceMatchRule;

impl DrcImpedanceMatchRule {
    pub const ID: &'static str = "drc-impedance-match";

    pub fn new() -> Self {
        Self
    }
}
impl Default for DrcImpedanceMatchRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DrcImpedanceMatchRule {
    fn id(&self) -> &str {
        Self::ID
    }

    fn evaluate(&self, ctx: &VerificationContext) -> Vec<ViolationFinding> {
        // The microstrip model needs the board's LayerStack; with no board the rule is silent.
        let Some(board) = ctx.board else {
            return Vec::new();
        };
        let mut findings = Vec::new();
        for track in ctx.tracks.iter() {
            // Only nets that DECLARE a controlled-impedance target are checked; absent one the net
            // is uncontrolled and the rule emits nothing rather than inventing a target.
            let Some(net) = ctx.nets.iter().find(|n| n.id == track.net) else {
                continue;
            };
            let Some(target) = net.impedance_target.as_ref() else {
                continue;
            };
            let target_ohm = target.si_magnitude();
            if !target_ohm.is_finite() || target_ohm <= 0.0 {
                continue; // Net::validate rejects these; defensive skip keeps the rule total.
            }
            let Some(layer) = reference_layer_for_side(&board.stack, track.layer) else {
                continue;
            };
            let h_mm = layer.dielectric_height.si_magnitude() * 1e3;
            let t_mm = layer.copper_thickness.si_magnitude() * 1e3;
            let w_mm = track.width.si_magnitude() * 1e3;
            let z0_real = microstrip_z0(layer.dielectric_er, h_mm, w_mm, t_mm);
            // A width so wide that `0.8w + t ≥ 5.98h` drives the log argument ≤ 1, pushing the
            // modelled Z₀ ≤ 0 — outside IPC-2141 microstrip validity. Routing never mints such a
            // width for a controlled net ([`microstrip_width`] guards `w > 0`), but a hand-set
            // width could; flag it as its own validity violation rather than emitting a
            // divide-by-zero Γ.
            if !z0_real.is_finite() || z0_real <= 0.0 {
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![track.id],
                    message: format!(
                        "track {} width {} is too wide to model as a microstrip for net {}'s {} target (outside IPC-2141 validity)",
                        track.id.short(),
                        track.width,
                        net.id.short(),
                        target,
                    ),
                });
                continue;
            }
            let deviation = (z0_real - target_ohm).abs() / target_ohm;
            if deviation > IMPEDANCE_TOLERANCE {
                let gamma = reflection_gamma(z0_real, target_ohm);
                findings.push(ViolationFinding {
                    rule: Self::ID.to_string(),
                    severity: ViolationSeverity::Error,
                    subjects: vec![track.id],
                    message: format!(
                        "track {} realizes {:.1} ohm (gamma {:+.3}) but net {} targets {} — past the {:.0}% controlled-impedance band",
                        track.id.short(),
                        z0_real,
                        gamma,
                        net.id.short(),
                        target,
                        IMPEDANCE_TOLERANCE * 100.0,
                    ),
                });
            }
        }
        findings
    }
}

// ================================== DFM Rules ==================================

/// The DEFAULT fabrication/assembly keep-out band from the board edge, in millimetres — the
/// fallback used when no fab-process keep-out is stated. Component bodies and copper inside this
/// band foul pick-and-place and are nicked during depanelization, so a design that merely *fits*
/// the outline can still be unmanufacturable. A design pins the real band on a
/// [`Fabrication`](RequirementCategory::Fabrication) requirement (slot 1, see
/// [`resolve_edge_keepout_si`]); absent one, this constant applies. Kept below the placement
/// margin so a normally-placed design clears it.
const DFM_EDGE_CLEARANCE_FALLBACK_MM: f64 = 0.5;

/// DFM rule: every [`Placement`]'s courtyard must keep at least the fabrication keep-out band
/// (slot 1 of the Fabrication Length targets, else the fallback — see [`resolve_edge_keepout_si`])
/// from the [`Board`] edge. Distinct from [`DrcOutOfBoundsRule`], which only requires the
/// courtyard to *fit*: this demands an assembly keep-out band, so a component that fits but hugs
/// the edge is a manufacturability Error. Comparisons are on the SI axis via `si_magnitude()` (P9);
/// with no board there is no edge to measure from, so the rule emits nothing. Placements are
/// scanned in slice order for determinism.
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
        let m = resolve_edge_keepout_si(ctx.requirements); // SI metres
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
                        "placement {} is within the {:.3} mm board-edge keep-out",
                        placement.id.short(),
                        m * 1e3
                    ),
                });
            }
        }
        findings
    }
}

/// DFM rule: every [`Track`]'s copper must keep at least the fabrication keep-out band (the same
/// resolved band as [`DfmEdgeClearanceRule`], see [`resolve_edge_keepout_si`]) from the
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
        let m = resolve_edge_keepout_si(ctx.requirements); // SI metres
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
                        "track {} copper is within the {:.3} mm board-edge keep-out",
                        track.id.short(),
                        m * 1e3
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
    use eak_domain::{BoardSide, ConstraintStatus, LayerStack, Priority, RequirementStatus};
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
            current: None,
            impedance_target: None,
        }
    }

    fn net_with_current(id: u128, class: NetClass, current_a: f64) -> Net {
        Net {
            id: EntityId(id),
            name: format!("NET{id}"),
            class,
            members: vec![],
            current: Some(PhysicalQuantity::new(current_a, Unit::Ampere)),
            impedance_target: None,
        }
    }

    fn net_with_impedance(id: u128, ohms: f64) -> Net {
        Net {
            id: EntityId(id),
            name: format!("NET{id}"),
            class: NetClass::Signal,
            members: vec![],
            current: None,
            impedance_target: Some(PhysicalQuantity::new(ohms, Unit::Ohm)),
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
            stack: LayerStack::standard_two_layer(),
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

    fn ampacity_ctx<'a>(
        board: Option<&'a Board>,
        nets: &'a [Net],
        tracks: &'a [Track],
    ) -> VerificationContext<'a> {
        VerificationContext {
            requirements: &[],
            constraints: &[],
            components: &[],
            pins: &[],
            nets,
            parts: &[],
            bom_line_items: &[],
            board,
            placements: &[],
            tracks,
        }
    }

    #[test]
    fn ampacity_width_matches_the_ipc_2221_external_curve() {
        // 3 A on 1 oz (0.035 mm) external copper at the conservative 10 °C rise needs ~1.37 mm
        // (IPC-2221 inverted). Asserted to ±0.01 mm so the closed form is pinned, not approximated.
        let w = ampacity_min_width_mm(3.0, 0.035);
        assert!((w - 1.367).abs() < 0.01, "expected ~1.367 mm, got {w}");
        // Twice the copper (2 oz) carries the same current in strictly less width.
        assert!(ampacity_min_width_mm(3.0, 0.070) < w);
        // A 50 mA signal current needs far less copper than any real trace width.
        assert!(ampacity_min_width_mm(0.05, 0.035) < 0.01);
    }

    #[test]
    fn underwidth_current_carrying_track_is_flagged() {
        let b = board(1, 100.0, 80.0);
        // track(10) realizes net EntityId(910); give that net a 3 A load...
        let nets = vec![net_with_current(910, NetClass::Power, 3.0)];
        // ...but route it as a 0.5 mm trace, well under the ~1.37 mm ampacity floor.
        let tracks = vec![track(10, 0.5)];
        let findings = DrcAmpacityWidthRule::new().evaluate(&ampacity_ctx(Some(&b), &nets, &tracks));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, DrcAmpacityWidthRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(10)]);
    }

    #[test]
    fn track_at_or_above_the_ampacity_floor_passes() {
        let b = board(1, 100.0, 80.0);
        let nets = vec![net_with_current(910, NetClass::Power, 3.0)];
        // 2 mm comfortably exceeds the ~1.37 mm floor for 3 A on 1 oz copper.
        let tracks = vec![track(10, 2.0)];
        assert!(DrcAmpacityWidthRule::new()
            .evaluate(&ampacity_ctx(Some(&b), &nets, &tracks))
            .is_empty());
    }

    #[test]
    fn net_without_a_stated_current_has_no_ampacity_floor() {
        let b = board(1, 100.0, 80.0);
        // No current on the net: the rule invents none, so even an absurdly thin trace is silent.
        let nets = vec![net(910, NetClass::Signal, vec![])];
        let tracks = vec![track(10, 0.05)];
        assert!(DrcAmpacityWidthRule::new()
            .evaluate(&ampacity_ctx(Some(&b), &nets, &tracks))
            .is_empty());
    }

    #[test]
    fn ampacity_is_silent_without_a_board() {
        // No board → no LayerStack → no copper thickness to size against, so the rule is silent.
        let nets = vec![net_with_current(910, NetClass::Power, 3.0)];
        let tracks = vec![track(10, 0.5)];
        assert!(DrcAmpacityWidthRule::new()
            .evaluate(&ampacity_ctx(None, &nets, &tracks))
            .is_empty());
    }

    #[test]
    fn microstrip_inversion_round_trips() {
        // 50 Ω on the standard 1.6 mm FR-4 stack (ε_r 4.5, 1 oz Cu) resolves to ~2.914 mm...
        let w = microstrip_width(50.0, 4.5, 1.6, 0.035).expect("50 Ω is feasible on this stack");
        assert!((w - 2.914).abs() < 0.01, "expected ~2.914 mm, got {w}");
        // ...and that width fed back through the forward model realizes ~50 Ω (the round trip closes).
        let z0 = microstrip_z0(4.5, 1.6, w, 0.035);
        // The inverse is exact, so the round-trip closes to float precision — a tight bound pins
        // the two closed forms as algebraic inverses, not merely "close".
        assert!((z0 - 50.0).abs() < 1e-8, "round-trip Z0 = {z0}");
    }

    #[test]
    fn microstrip_z0_decreases_with_width() {
        // Wider copper → lower impedance: the monotonicity that makes the width unique per target.
        let narrow = microstrip_z0(4.5, 1.6, 0.5, 0.035);
        let wide = microstrip_z0(4.5, 1.6, 3.0, 0.035);
        assert!(narrow > wide, "narrow {narrow} should exceed wide {wide}");
    }

    #[test]
    fn microstrip_width_is_none_above_the_stack_ceiling() {
        // 130 Ω on a thin 0.2 mm dielectric is above the achievable ceiling → no positive width.
        assert!(microstrip_width(130.0, 4.5, 0.2, 0.035).is_none());
    }

    #[test]
    fn reflection_gamma_is_zero_when_matched() {
        assert!(reflection_gamma(50.0, 50.0).abs() < 1e-12);
        assert!(reflection_gamma(75.0, 50.0) > 0.0); // realized above target → positive Γ
    }

    #[test]
    fn impedance_matched_track_passes() {
        let b = board(1, 100.0, 80.0);
        let nets = vec![net_with_impedance(910, 50.0)];
        // 2.914 mm realizes ~50 Ω on the standard stack — inside the ±10 % band.
        let tracks = vec![track(10, 2.914)];
        assert!(DrcImpedanceMatchRule::new()
            .evaluate(&ampacity_ctx(Some(&b), &nets, &tracks))
            .is_empty());
    }

    #[test]
    fn impedance_mismatched_track_is_flagged() {
        let b = board(1, 100.0, 80.0);
        let nets = vec![net_with_impedance(910, 50.0)];
        // The flat 0.25 mm signal default realizes ~133 Ω on this stack — far past ±10 %.
        let tracks = vec![track(10, 0.25)];
        let findings =
            DrcImpedanceMatchRule::new().evaluate(&ampacity_ctx(Some(&b), &nets, &tracks));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, DrcImpedanceMatchRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(10)]);
    }

    #[test]
    fn uncontrolled_net_has_no_impedance_check() {
        let b = board(1, 100.0, 80.0);
        // No impedance target: even the mismatched 0.25 mm default is silent (the net is uncontrolled).
        let nets = vec![net(910, NetClass::Signal, vec![])];
        let tracks = vec![track(10, 0.25)];
        assert!(DrcImpedanceMatchRule::new()
            .evaluate(&ampacity_ctx(Some(&b), &nets, &tracks))
            .is_empty());
    }

    #[test]
    fn impedance_is_silent_without_a_board() {
        // No board → no LayerStack → no microstrip model to realize Z₀ against, so the rule is silent.
        let nets = vec![net_with_impedance(910, 50.0)];
        let tracks = vec![track(10, 0.25)];
        assert!(DrcImpedanceMatchRule::new()
            .evaluate(&ampacity_ctx(None, &nets, &tracks))
            .is_empty());
    }

    #[test]
    fn absurdly_wide_controlled_track_is_flagged_as_out_of_validity() {
        let b = board(1, 100.0, 80.0);
        let nets = vec![net_with_impedance(910, 50.0)];
        // 15 mm on the 1.6 mm stack drives 0.8w+t past 5.98h, so the modelled Z₀ goes non-physical
        // (≤ 0). The rule flags it via the validity guard rather than dividing by zero in Γ.
        let tracks = vec![track(10, 15.0)];
        let findings =
            DrcImpedanceMatchRule::new().evaluate(&ampacity_ctx(Some(&b), &nets, &tracks));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, DrcImpedanceMatchRule::ID);
        assert_eq!(findings[0].subjects, vec![EntityId(10)]);
        assert!(findings[0].message.contains("validity"));
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

    /// A Fabrication requirement carrying a length target — the fabrication process floor the
    /// trace-width rule reads.
    fn process_floor_req(min_mm: f64) -> Requirement {
        Requirement {
            id: EntityId(700),
            statement: format!("Fabrication process supports a {min_mm} mm minimum trace width"),
            category: RequirementCategory::Fabrication,
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
        // No Fabrication length target: the process floor is unstated, so even a hair-thin trace
        // is not flagged rather than guessing a floor.
        let tracks = vec![track(10, 0.05)];
        assert!(DrcTraceWidthRule::new()
            .evaluate(&trace_ctx(&[], &tracks))
            .is_empty());
    }

    // ------------------------------ unrouted-net rule tests ------------------------------

    /// A track realizing a specific net (the unrouted-net rule keys on `track.net`).
    fn track_for_net(id: u128, net_id: u128) -> Track {
        Track {
            id: EntityId(id),
            net: EntityId(net_id),
            layer: BoardSide::Top,
            width: mm(0.25),
            x1: mm(1.0),
            y1: mm(1.0),
            x2: mm(9.0),
            y2: mm(1.0),
        }
    }

    fn unrouted_ctx<'a>(nets: &'a [Net], tracks: &'a [Track]) -> VerificationContext<'a> {
        VerificationContext {
            requirements: &[],
            constraints: &[],
            components: &[],
            pins: &[],
            nets,
            parts: &[],
            bom_line_items: &[],
            board: None,
            placements: &[],
            tracks,
        }
    }

    #[test]
    fn unrouted_net_is_flagged() {
        // Net 10 has a track; net 11 has none — only net 11 is an electrical break.
        let nets = vec![
            net(10, NetClass::Power, vec![1]),
            net(11, NetClass::Signal, vec![2]),
        ];
        let tracks = vec![track_for_net(100, 10)];
        let findings = DrcUnroutedNetRule::new().evaluate(&unrouted_ctx(&nets, &tracks));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, DrcUnroutedNetRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(11)]);
    }

    #[test]
    fn fully_routed_nets_pass() {
        let nets = vec![
            net(10, NetClass::Power, vec![1]),
            net(11, NetClass::Signal, vec![2]),
        ];
        let tracks = vec![track_for_net(100, 10), track_for_net(101, 11)];
        assert!(DrcUnroutedNetRule::new()
            .evaluate(&unrouted_ctx(&nets, &tracks))
            .is_empty());
    }

    #[test]
    fn unrouted_rule_is_silent_with_no_nets() {
        // Nothing to realize: a design with no nets has no unrouted nets.
        assert!(DrcUnroutedNetRule::new()
            .evaluate(&unrouted_ctx(&[], &[]))
            .is_empty());
    }

    // ------------------------------ net-open rule tests ------------------------------

    /// A track segment of a specific net spanning two arbitrary endpoints (mm). The open rule
    /// resolves each endpoint to a placement centroid, so its coordinates carry the topology.
    #[allow(clippy::too_many_arguments)]
    fn seg(id: u128, net_id: u128, x1: f64, y1: f64, x2: f64, y2: f64) -> Track {
        Track {
            id: EntityId(id),
            net: EntityId(net_id),
            layer: BoardSide::Top,
            width: mm(0.25),
            x1: mm(x1),
            y1: mm(y1),
            x2: mm(x2),
            y2: mm(y2),
        }
    }

    fn open_ctx<'a>(
        pins: &'a [Pin],
        nets: &'a [Net],
        placements: &'a [Placement],
        tracks: &'a [Track],
    ) -> VerificationContext<'a> {
        VerificationContext {
            requirements: &[],
            constraints: &[],
            components: &[],
            pins,
            nets,
            parts: &[],
            bom_line_items: &[],
            board: None,
            placements,
            tracks,
        }
    }

    /// The three-pad fixture shared by the open-rule tests: pins 1/2/3 sit on components
    /// 901/902/903 (the `pin` helper's `900 + id` convention), placed left-to-right with
    /// centroids at (5,5), (25,5), (45,5) mm. Net 50 joins all three pins.
    fn three_pad_net() -> (Vec<Pin>, Vec<Net>, Vec<Placement>) {
        let pins = vec![
            pin(1, PinElectricalType::Passive),
            pin(2, PinElectricalType::Passive),
            pin(3, PinElectricalType::Passive),
        ];
        let nets = vec![net(50, NetClass::Signal, vec![1, 2, 3])];
        let placements = vec![
            placement(10, 901, 0.0, 0.0, 10.0, 10.0, BoardSide::Top),
            placement(11, 902, 20.0, 0.0, 10.0, 10.0, BoardSide::Top),
            placement(12, 903, 40.0, 0.0, 10.0, 10.0, BoardSide::Top),
        ];
        (pins, nets, placements)
    }

    #[test]
    fn daisy_chained_net_is_connected() {
        // The happy-path topology: two consecutive segments (pA-pB, pB-pC) coalesce all three
        // pads into one copper component, so the net is connected — no finding.
        let (pins, nets, placements) = three_pad_net();
        let tracks = vec![
            seg(100, 50, 5.0, 5.0, 25.0, 5.0),
            seg(101, 50, 25.0, 5.0, 45.0, 5.0),
        ];
        assert!(DrcNetOpenRule::new()
            .evaluate(&open_ctx(&pins, &nets, &placements, &tracks))
            .is_empty());
    }

    #[test]
    fn net_with_isolated_interior_pad_is_open() {
        // A single first->last segment (pA-pC) leaves the interior pad pB in its own island:
        // the member pads fall in two components, so the net is electrically open.
        let (pins, nets, placements) = three_pad_net();
        let tracks = vec![seg(100, 50, 5.0, 5.0, 45.0, 5.0)];
        let findings =
            DrcNetOpenRule::new().evaluate(&open_ctx(&pins, &nets, &placements, &tracks));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, DrcNetOpenRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(50)]);
    }

    #[test]
    fn net_with_no_track_is_deferred_to_unrouted_rule() {
        // No copper at all: leave it to DrcUnroutedNetRule rather than double-flag the break.
        let (pins, nets, placements) = three_pad_net();
        assert!(DrcNetOpenRule::new()
            .evaluate(&open_ctx(&pins, &nets, &placements, &[]))
            .is_empty());
    }

    #[test]
    fn single_placement_net_is_trivially_connected() {
        // Two pins on ONE component (one placement) span no gap, so the net cannot be open even
        // with a single degenerate self-track — guards against a false positive on intra-part nets.
        // Both pins resolve to component 901 (pin 2 is forced onto it from its default 902).
        let pins = [
            pin(1, PinElectricalType::Passive),
            Pin {
                component: EntityId(901),
                ..pin(2, PinElectricalType::Passive)
            },
        ];
        let nets = vec![net(50, NetClass::Signal, vec![1, 2])];
        let placements = vec![placement(10, 901, 0.0, 0.0, 10.0, 10.0, BoardSide::Top)];
        let tracks = vec![seg(100, 50, 5.0, 5.0, 5.0, 5.0)];
        assert!(DrcNetOpenRule::new()
            .evaluate(&open_ctx(&pins, &nets, &placements, &tracks))
            .is_empty());
    }

    // --------------------------- copper-clearance rule tests ---------------------------
    // (Reuses the `fab_req(id, &[mm…])` Fabrication-target builder defined in the DFM tests; the
    // clearance floor is slot 2 of that positional target contract.)

    #[test]
    fn overlapping_different_net_copper_is_silent_without_a_clearance_floor() {
        // The happy-path guard: the stub router mints identical, overlapping centre-lines for two
        // different nets (gap = 0). With slots 0/1 stated but NO slot-2 clearance floor — exactly
        // the default run — the rule emits nothing rather than flag this true geometric overlap.
        let reqs = vec![fab_req(710, &[0.75, 0.5])];
        let tracks = vec![
            seg(100, 60, 5.0, 5.0, 25.0, 5.0),
            seg(101, 61, 5.0, 5.0, 25.0, 5.0),
        ];
        assert!(DrcCopperClearanceRule::new()
            .evaluate(&trace_ctx(&reqs, &tracks))
            .is_empty());
    }

    #[test]
    fn overlapping_different_net_copper_is_flagged_once_a_floor_is_stated() {
        // The same overlapping pair, now with a slot-2 floor stated: the zero gap is far below
        // 1.0 mm, so it is a blocking short. Proves the silence above is the absent floor, not a
        // geometry quirk — and that colinear overlap reads as a zero centre-line distance.
        let reqs = vec![fab_req(710, &[0.75, 0.5, 1.0])];
        let tracks = vec![
            seg(100, 60, 5.0, 5.0, 25.0, 5.0),
            seg(101, 61, 5.0, 5.0, 25.0, 5.0),
        ];
        let findings = DrcCopperClearanceRule::new().evaluate(&trace_ctx(&reqs, &tracks));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, DrcCopperClearanceRule::ID);
        assert_eq!(findings[0].severity, ViolationSeverity::Error);
        assert_eq!(findings[0].subjects, vec![EntityId(100), EntityId(101)]);
    }

    #[test]
    fn parallel_different_net_tracks_closer_than_the_floor_are_flagged() {
        // Two parallel traces 0.5 mm apart centre-to-centre; each is 0.25 mm wide, so the
        // edge-to-edge gap is 0.25 mm — below the 1.0 mm slot-2 floor. Listed b-before-a in slice
        // order to prove the subject vector is sorted.
        let reqs = vec![fab_req(710, &[0.75, 0.5, 1.0])];
        let tracks = vec![
            seg(101, 61, 0.0, 0.5, 10.0, 0.5),
            seg(100, 60, 0.0, 0.0, 10.0, 0.0),
        ];
        let findings = DrcCopperClearanceRule::new().evaluate(&trace_ctx(&reqs, &tracks));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, DrcCopperClearanceRule::ID);
        assert_eq!(findings[0].subjects, vec![EntityId(100), EntityId(101)]);
    }

    #[test]
    fn same_net_tracks_are_exempt_from_the_clearance_floor() {
        // The identical close geometry, but both traces realize ONE net: a net's own copper may
        // touch, so the same-net exemption keeps it silent.
        let reqs = vec![fab_req(710, &[0.75, 0.5, 1.0])];
        let tracks = vec![
            seg(100, 60, 0.0, 0.0, 10.0, 0.0),
            seg(101, 60, 0.0, 0.5, 10.0, 0.5),
        ];
        assert!(DrcCopperClearanceRule::new()
            .evaluate(&trace_ctx(&reqs, &tracks))
            .is_empty());
    }

    #[test]
    fn copper_on_opposite_sides_does_not_short() {
        // The close different-net pair, but one trace on Top and one on Bottom: opposite copper
        // never shorts, so the pair is skipped.
        let reqs = vec![fab_req(710, &[0.75, 0.5, 1.0])];
        let tracks = vec![
            seg(100, 60, 0.0, 0.0, 10.0, 0.0),
            Track {
                layer: BoardSide::Bottom,
                ..seg(101, 61, 0.0, 0.5, 10.0, 0.5)
            },
        ];
        assert!(DrcCopperClearanceRule::new()
            .evaluate(&trace_ctx(&reqs, &tracks))
            .is_empty());
    }

    #[test]
    fn copper_clearing_the_floor_is_not_flagged() {
        // Two different-net traces 5 mm apart centre-to-centre: the 4.75 mm edge-to-edge gap
        // comfortably clears the 1.0 mm floor, so there is no finding.
        let reqs = vec![fab_req(710, &[0.75, 0.5, 1.0])];
        let tracks = vec![
            seg(100, 60, 0.0, 0.0, 10.0, 0.0),
            seg(101, 61, 0.0, 5.0, 10.0, 5.0),
        ];
        assert!(DrcCopperClearanceRule::new()
            .evaluate(&trace_ctx(&reqs, &tracks))
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

    // ------------------- fabrication-sourced edge-clearance keep-out tests -------------------

    /// A Fabrication requirement carrying Length targets in slot order: slot 0 = minimum trace
    /// width, slot 1 = board-edge keep-out band.
    fn fab_req(id: u128, mm_targets: &[f64]) -> Requirement {
        Requirement {
            id: EntityId(id),
            statement: "Fabrication process limits".into(),
            category: RequirementCategory::Fabrication,
            priority: Priority::High,
            acceptance_criterion: "process limits met".into(),
            status: RequirementStatus::Accepted,
            source: EntityId(1),
            targets: mm_targets.iter().map(|&v| mm(v)).collect(),
        }
    }

    fn keepout_ctx<'a>(
        board: Option<&'a Board>,
        placements: &'a [Placement],
        requirements: &'a [Requirement],
    ) -> VerificationContext<'a> {
        VerificationContext {
            requirements,
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

    fn keepout_track_ctx<'a>(
        board: Option<&'a Board>,
        tracks: &'a [Track],
        requirements: &'a [Requirement],
    ) -> VerificationContext<'a> {
        VerificationContext {
            requirements,
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
    fn stated_keepout_widens_the_band() {
        let b = board(1, 100.0, 80.0);
        // Courtyard right edge at 99.3 mm — 0.7 mm from the board edge: clears the 0.5 mm fallback,
        // but a stated slot-1 keep-out of 1.0 mm flags it. (slot 0 = 0.25 mm trace width.)
        let placements = vec![placement(10, 900, 79.3, 10.0, 20.0, 20.0, BoardSide::Top)];
        assert!(DfmEdgeClearanceRule::new()
            .evaluate(&keepout_ctx(Some(&b), &placements, &[]))
            .is_empty());
        let reqs = vec![fab_req(700, &[0.25, 1.0])];
        let findings =
            DfmEdgeClearanceRule::new().evaluate(&keepout_ctx(Some(&b), &placements, &reqs));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].subjects, vec![EntityId(10)]);
    }

    #[test]
    fn stated_keepout_can_relax_a_fallback_failure() {
        let b = board(1, 100.0, 80.0);
        // 0.3 mm from the right edge: fails the 0.5 mm fallback, but a stated slot-1 keep-out of
        // 0.25 mm clears it (and it still fits the outline, so DRC out-of-bounds is clean).
        let placements = vec![placement(10, 900, 79.7, 10.0, 20.0, 20.0, BoardSide::Top)];
        assert_eq!(
            DfmEdgeClearanceRule::new()
                .evaluate(&keepout_ctx(Some(&b), &placements, &[]))
                .len(),
            1
        );
        let reqs = vec![fab_req(700, &[0.25, 0.25])];
        assert!(DfmEdgeClearanceRule::new()
            .evaluate(&keepout_ctx(Some(&b), &placements, &reqs))
            .is_empty());
    }

    #[test]
    fn lone_trace_floor_leaves_keepout_at_fallback_and_still_floors_traces() {
        let b = board(1, 100.0, 80.0);
        // Only a slot-0 (trace-width) Fabrication target is stated: the keep-out must NOT claim it
        // — it stays at the 0.5 mm fallback, so a 0.3-mm-from-edge courtyard is still flagged.
        let placements = vec![placement(10, 900, 79.7, 10.0, 20.0, 20.0, BoardSide::Top)];
        let reqs = vec![fab_req(700, &[0.25])];
        assert_eq!(
            DfmEdgeClearanceRule::new()
                .evaluate(&keepout_ctx(Some(&b), &placements, &reqs))
                .len(),
            1
        );
        // ...and the trace-width rule still reads that lone slot-0 value as its floor.
        let tracks = vec![track(11, 0.20)];
        let tw = DrcTraceWidthRule::new().evaluate(&trace_ctx(&reqs, &tracks));
        assert_eq!(tw.len(), 1);
        assert_eq!(tw[0].subjects, vec![EntityId(11)]);
    }

    #[test]
    fn trace_edge_rule_honours_the_resolved_keepout() {
        let b = board(1, 100.0, 80.0);
        // Copper edge ~0.3 mm from the board edge (centre 0.425 mm, half-width 0.125 mm): flagged
        // at the 0.5 mm fallback, cleared by a stated 0.25 mm keep-out — the copper band tracks the
        // same resolver as placements.
        let tracks = vec![edge_track(10, 0.425, 40.0, 90.0, 40.0, 0.25)];
        assert_eq!(
            DfmTraceEdgeClearanceRule::new()
                .evaluate(&keepout_track_ctx(Some(&b), &tracks, &[]))
                .len(),
            1
        );
        let reqs = vec![fab_req(700, &[0.25, 0.25])];
        assert!(DfmTraceEdgeClearanceRule::new()
            .evaluate(&keepout_track_ctx(Some(&b), &tracks, &reqs))
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
