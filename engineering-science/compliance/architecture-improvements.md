# Architecture Improvements

This document collects the **forward-looking, structural** improvements the architecture review surfaced — moves that are bigger than a single edit or a single rule, and that would let the runtime *enforce more of the science it already documents*. The audit returned 32 confirmed findings (0 critical, 7 major, 20 minor, 5 info). Most of the major findings are not isolated defects; they are symptoms of a small number of missing entities in the inner ring — a typed layer stack-up, a reference-plane object, a copper-connectivity graph — whose absence makes whole families of checks *unrepresentable rather than merely unimplemented*. The proposals below cluster those findings into thirteen structural moves, ordered so that the keystone (a typed stack-up) lands first and unlocks the impedance, ampacity, thermal, and return-path work that depends on it. Each proposal states its rationale, the science it would unlock, and where it slots into the existing [clean-architecture rings](../../docs/decisions/0001-adopt-clean-architecture-dependency-rule.md) and the 15-phase pipeline. These are **recommendations, not commitments** — Phases 1–3 are built and honest about their scope; the value here is in naming the next foundations to pour, not in relabeling the present ones as defects.

## How to read this

- **Rings** are the [clean-architecture layers](../../docs/decisions/0001-adopt-clean-architecture-dependency-rule.md): *Entities* (`eak-domain`, `eak-units`), *Use cases / runtime* (`eak-engines`, `eak-compiler`, `eak-phases`, `eak-runtime`), *Interface adapters* (`eak-ports`, `eak-store`, `eak-reasoning`), *Frameworks* (`eak-cli`). Dependencies point only inward, so an entity added in the inner ring is visible to every phase above it.
- **Phases** are the state machines in [`eak-phases/src`](../../eak/crates/eak-phases). The verification rules they register all live in [`eak-engines/src/lib.rs`](../../eak/crates/eak-engines/src/lib.rs).
- Where a proposal would *retire a proxy* (a deliberately lenient placeholder), that is called out so the move is not read as fixing a bug — the proxy is honest today; the improvement is to make it exact.

## Proposal summary

| # | Proposal | Primary ring | Lands in phase(s) | Depends on | Findings addressed |
|---|----------|--------------|-------------------|------------|--------------------|
| A1 | Typed layer stack-up + unit-system extension (the keystone) | Entities | PCB Floor Planning → PCB IR | — | 2, 4, 12, 14, 23, 26 |
| B1 | Ampacity / IR-drop width-sizing pass | Use cases | Routing Planning, DRC | A1 | 11, 15, 32 |
| B2 | Impedance-aware net classes, differential pairs, stack-up-derived widths | Entities + Use cases | Schematic/Routing, Constraint Engine | A1 | 3, 18, 20, 30 |
| B3 | Thermal verification pass (junction temperature) | Entities + Use cases | Engineering Analysis | A1 | 2 |
| C1 | Copper-connectivity graph: open/short detection + copper clearance | Use cases | DRC | C3 (partial) | 1, 6 |
| C2 | Reference-plane / copper-pour model + return-path-continuity checker | Entities + Use cases | Routing, DRC, PCB IR | A1 | 5, 13, 17, 19 |
| C3 | Real search/optimization for routing & placement | Use cases | Routing Planning, Component Placement | C2 (for layers) | 8 |
| D1 | Corrective loop-back controllers (closed-loop actuation) | Use cases | All looping phases + orchestrator | B1/C3 | 10 |
| D2 | Richer constraint network: arc-consistency + typed target roles | Entities + Use cases | Constraint Engine, Fabrication targets | — | 24, 28 |
| D3 | Statistical / worst-case tolerance propagation | Use cases | Constraint Engine | — | 9 |
| E1 | Via / plated-hole entity + annular-ring / drill / aspect / mask DRC | Entities + Use cases | DFM Verification | — | 21 |
| E2 | Manufacturing-IR completeness: drill, mask, panel, fiducials, DFT, yield | Entities + Use cases | Manufacturing Generation | A1, E1 | 22, 23 |
| F1 | Per-net frequency / edge-rate EMC + loop-area model | Entities + Use cases | EMC Analysis | C2 (loop area) | 16, 29, 31 |
| G1 | Runtime-mapping documentation-honesty pass | Science / docs | runtime-mapping/ | — | 7, 25, 26, 27 |

---

## A1. Typed layer stack-up and unit-system extension *(the keystone)*

**Today.** The board's vertical cross-section is reduced to a single integer. [`Board`](../../eak/crates/eak-domain/src/lib.rs) is `{ id, width, height, layers: u32 }`; [PCB Floor Planning](../../eak/crates/eak-phases/src/pcb_floor_planning.rs) hardcodes `layers: 2`; there is no per-layer dielectric height `h`, no `ε_r`, no copper thickness/weight `t`, and no designation of which layers are planes versus signal. The [unit system](../../eak/crates/eak-units/src/lib.rs) `Dimension` enum has Temperature, Power, and (electrical) Resistance, but **no thermal-resistance (K/W) and no area (m²)** dimension.

**Why it is the keystone.** Three of the seven major findings (4, 12, 14) and a thermal one (2) reduce to the same root: with no `t` and no `ε_r`, the trace resistance `R = ρL/(w·t)`, the characteristic impedance `Z₀ = f(w, h, t, ε_r)`, and the IPC-2152 cross-section `A = w·t` are *uncomputable in principle*, not merely unchecked. Every downstream electrical, SI, and thermal proposal (B1, B2, B3, F1) is blocked behind this single data gap.

**Science it unlocks.** [`stackup.md`](../pcb/stackup.md) clause 3 (`Z₀ ≈ (87/√(ε_r+1.41))·ln(5.98h/(0.8w+t))`); [`ohms-law.md`](../electrical/ohms-law.md) §2 (`R = ρL/(w·t)`); [`thermal-physics.md`](../physics/thermal-physics.md) §4–§8 (θ in K/W, copper area in mm²); [`materials-science.md`](../physics/materials-science.md) (1 oz ≈ 35 µm fixes sheet resistance and ampacity).

**Proposed move.** Introduce a typed `LayerStack` entity on `Board`/`PcbIr`: an ordered list of layers, each carrying a [Physical Quantity](../../eak/crates/eak-units/src/lib.rs) height + `ε_r` + loss tangent + copper thickness/weight + a `Plane | Signal` role, plus a per-signal-layer reference binding. Add `ThermalResistance (K/W)` and `Area (m²)` to the `Dimension`/`Unit` enums. Populate the stack in PCB Floor Planning's `AllocatingRegions` and gate it in `ValidatingFloorPlan` so a stack that cannot meet a net class's impedance target re-proposes ([`stackup.md`](../pcb/stackup.md) clause 7).

**Where it slots.** *Entities* ring (`eak-domain`, `eak-units`); consumed first in **PCB Floor Planning**, persisted through the **PCB IR** ([`pcb-ir.md`](../../docs/compiler/ir/pcb-ir.md) already specifies this stack-up as invariant 5).

| Finding | Severity | Locus | What A1 supplies |
|---|---|---|---|
| 4 | major | bare `layers: u32`, no `h`/`ε_r`/`t`/roles | the stack-up entity itself |
| 12 | minor | Dk/Df + per-layer thickness absent | `ε_r`, loss tangent, `t` fields |
| 14 | minor | no copper weight / dielectric on `Board` | copper weight → `t`, `ε_r` |
| 2 | major | no K/W or m² unit, no `t` on Track/Board | unit dimensions + cross-section inputs |
| 23 | minor | Manufacturing IR carries no stack-up | the typed object E2 then lowers |
| 26 | minor | docs claim a typed Layer Stack that is absent | makes the present-tense claim true |

---

## B1. Ampacity / IR-drop width-sizing pass

**Today.** Trace width is a fixed per-class constant — 0.50 mm power/ground, 0.25 mm signal — chosen by [`class_width_mm`](../../eak/crates/eak-phases/src/routing_planning.rs) with no reference to current, length, copper weight, or ΔT. `Net` and `Track` carry no current attribute, so a 5 A rail and a 5 mA signal in the same class receive identical copper, and `I·R` is *unrepresentable*. The only width rule, `DrcTraceWidthRule` in [`eak-engines`](../../eak/crates/eak-engines/src/lib.rs), checks each track against the fabrication process floor only — a manufacturability minimum, explicitly not an electrical one. (Finding 32 confirms this floor *is* genuinely fab-sourced and honest; the gap is the missing ampacity layer above it.)

**Science it unlocks.** [`ohms-law.md`](../electrical/ohms-law.md) §4–§7 (`ΔV = I·R_path ≤ budget`; `width = max(ampacity, IR-drop)` floored by process); [`kirchhoff-laws.md`](../electrical/kirchhoff-laws.md) (rail current is the KCL sum of loads); [`thermal-physics.md`](../physics/thermal-physics.md) §8 (IPC-2152 self-heating sets minimum cross-section).

**Proposed move.** Add a worst-case current attribute to `Net` (or a per-net-class current budget), then compute `width = max(ampacity_width(I, ΔT, t), ir_drop_width(ρ, L, I, t, ΔV))` clamped to the process floor, and register a verification rule that fails a rail whose computed `I·R_path` exceeds its ΔV budget. Requires the copper thickness `t` from **A1**.

**Where it slots.** *Entities* (current on `Net`) + *Use cases* (**Routing Planning** sizing, new DRC rule in `eak-engines`).

| Finding | Severity | What B1 supplies |
|---|---|---|
| 11 | minor | current input + an IPC-2152 ΔT width minimum |
| 15 | minor | an enforced IR-drop budget linking current to copper geometry |
| 32 | info | retires the documented per-class-constant proxy while keeping the fab floor |

---

## B2. Impedance-aware net classes, differential pairs, and stack-up-derived widths

**Today.** [`NetClass`](../../eak/crates/eak-domain/src/lib.rs) is exactly `{ Power, Ground, Signal }`. There is no controlled-impedance class, no `Z₀`/`Z_diff` target field on `Net`, no differential-pair binding on `Track`, and no length-match/skew concept. A net that must be 50 Ω single-ended or 100 Ω differential *cannot be declared*, and the signal width is the impedance-blind constant 0.25 mm, so `Γ ≠ 0` is a certainty for any real impedance spec. In a real deterministic run, [Schematic Planning](../../eak/crates/eak-phases/src/schematic_planning.rs) never even mints a `Signal` net (finding 30), so the signal-width path has no carrier today.

**Science it unlocks.** [`transmission-lines.md`](../electrical/transmission-lines.md) (per-net-class width *is* the realization of `Z₀ = √(L/C)`; delay-based length matching); [`power-integrity.md`](../electrical/power-integrity.md) (impedance target as a first-class typed constraint); [`differential-pairs.md`](../pcb/differential-pairs.md) (a pair is one electrical object with `Z_diff`; symmetry/skew bound mode conversion).

**Proposed move.** Add a controlled-impedance net class (or an Ω-dimensioned impedance-target `Constraint`), a differential-pair binding (partner net/track id) plus a `Differential` class carrying `Z_diff` and `ΔL_max`, and — once **A1** lands `ε_r`/`h`/`t` — compute the impedance-driven width from the IPC-2141 microstrip/stripline closed forms instead of a constant. Have Routing Planning route pairs together at constant spacing and the Verification Engine evaluate the skew length bound. Classify and mint inter-IC nets as `Signal` so the path is exercised end-to-end.

**Where it slots.** *Entities* (`NetClass`, `Net`, `Track`) + *Use cases* (**Routing Planning** width derivation, **Constraint Engine**).

| Finding | Severity | What B2 supplies |
|---|---|---|
| 3 | major | a controlled-impedance class + `Z₀`/`Z_diff` target field |
| 18 | minor | differential-pair binding + skew/`Z_diff` constraints with a subject |
| 20 | minor | impedance-derived width replacing the flat constant |
| 30 | info | a deterministic pipeline that mints `Signal` nets |

---

## B3. Thermal verification pass (junction temperature)

**Today.** `RequirementCategory::Thermal` exists only as a label; nothing computes a junction temperature anywhere in the workspace. With no θ_JA unit (K/W) and no power/area model, `T_j = T_amb + θ_JA·P` cannot even be *formed*, and the manufacturing gate ships with zero thermal verification.

**Science it unlocks.** [`thermal-physics.md`](../physics/thermal-physics.md) §4–§8 (junction-temperature law, copper-area minima, derating).

**Proposed move.** With the `ThermalResistance (K/W)` and `Area (m²)` dimensions from **A1**, implement a thermal evaluator (`T_j = T_amb + θ_JA·P`) as a `Rule` so the gate can block an over-temperature design. The θ_JA source (Datasheet Intelligence) is a documented future-phase deferral, so this pass can begin against explicitly-supplied θ values.

**Where it slots.** *Entities* (units) + *Use cases* (a rule in `eak-engines`, naturally evaluated in [Engineering Analysis](../../eak/crates/eak-phases/src/engineering_analysis.rs)). Addresses the thermal half of finding 2.

---

## C1. Copper-connectivity graph: open/short detection + copper clearance

**Today.** Net realization is verified by *track existence*, not connectivity. `DrcUnroutedNetRule` ([`eak-engines`](../../eak/crates/eak-engines/src/lib.rs)) only asks whether *any* track references the net; [Routing Planning](../../eak/crates/eak-phases/src/routing_planning.rs) mints one straight centroid-to-centroid segment per net, so for `k ≥ 3` pins the interior pins are off the copper — an electrically **open** net that still passes DRC and the manufacturing gate. There is no union-find anywhere, and **no copper-to-copper clearance / short rule** at all: two different nets' tracks may overlap (a dead short) and still clear DRC + DFM. `DrcCourtyardOverlapRule` is a component-body check, not a copper check.

**Science it unlocks.** [`graph-theory.md`](../mathematics/graph-theory.md) §2 (connected-components / union-find — *open* iff a net's pads fall in >1 component, *short* iff two nets share a component); [`manufacturing-constraints.md`](../manufacturing/manufacturing-constraints.md) clause 1 (minimum space protects against shorts the way minimum width protects against opens); [`ipc-standards.md`](../manufacturing/ipc-standards.md) §1–§2 (IPC-2221 spacing as a resolved, voltage-scaled quantity).

**Proposed move.** Add a disjoint-set/union-find connectivity rule over the copper graph: union pads joined by a track, then require every net's pads to share one component (else *open*) and no two distinct nets to share a component (else *short*). Add a copper-clearance rule flagging any two same-side tracks whose edge-to-edge gap is below the fab minimum space (voltage-scaled per IPC-2221 where peak net voltage is known), with a same-net exemption. Either realize multi-pin nets as a Steiner/spanning structure (≥ k−1 segments — see **C3**) or scope `DrcUnroutedNetRule` honestly to "a track exists."

**Where it slots.** *Use cases* (**DRC**, `eak-engines`). The connectivity rule is the dual of `drc-unrouted-net`; the clearance rule is the co-equal sibling of `drc-trace-width`.

| Finding | Severity | What C1 supplies |
|---|---|---|
| 1 | major | union-find open/short detection (the connected-components backbone the docs claim) |
| 6 | major | a trace-to-trace minimum-space / short rule |

---

## C2. Reference-plane / copper-pour model + return-path-continuity checker

**Today.** The runtime has **no object** for a ground/reference plane or copper pour, no signal-to-reference adjacency, and no plane void/split representation. Every track is minted on `BoardSide::Top`; `BoardSide` is only `{ Top, Bottom }`, so inner stripline layers are unaddressable even on a board declaring 4+ layers, and there is no via entity, so layer transitions and stitching/return vias cannot be expressed. Consequently the textbook fatal error — a controlled net crossing a plane split or running with no adjacent reference — passes 100% of the checks: the "connectivity-complete, DRC-clean, wrong-on-the-bench" failure the return-path doc exists to prevent. No crosstalk rule exists either.

**Science it unlocks.** [`return-path.md`](../pcb/return-path.md) (least-impedance return hugs the trace; a plane split is an electrical break invisible to connectivity; width and reference-continuity are "two halves of one guarantee"); [`ground-plane.md`](../pcb/ground-plane.md) (reference continuity as a layer-assignment invariant; the pour as a first-class IR object); [`maxwell-equations.md`](../physics/maxwell-equations.md) (every signal is a closed loop).

**Proposed move.** Add a `Plane`/`Pour` domain entity bound to a `Net`; generalize layer identity beyond Top/Bottom to an index into the **A1** stack; add a `Via` entity (with same-net return-via / different-net stitching-cap requirement at reference changes); give `VerificationContext` access to plane geometry; and implement a reference-continuity `Rule` (no plane split/void under a controlled net's trace projection; controlled nets require a continuous adjacent reference), scoped by net class and looping back to Routing Planning on failure. Add a parallel-run-length / edge-to-edge crosstalk rule once the plane and spacing data exist.

**Where it slots.** *Entities* (`Plane`/`Pour`, `Via`, layer index) + *Use cases* (**Routing Planning** layer assignment, **DRC** continuity rule, **PCB IR** persistence).

| Finding | Severity | What C2 supplies |
|---|---|---|
| 5 | major | the plane/pour entity + reference-continuity rule |
| 13 | minor | the return-path model the width/EMC checks silently assume |
| 17 | minor | reference-plane + crosstalk reasoning; multi-side routing |
| 19 | minor | inner-layer routing, via entity, return/stitching vias |

---

## C3. Real search/optimization for routing and placement

**Today.** Routing and placement are deterministic geometric stubs (and openly say so in their module docs). [Routing Planning](../../eak/crates/eak-phases/src/routing_planning.rs) emits a straight centroid-to-centroid segment with no routing grid, obstacle field, shortest-path, Steiner topology, or rip-up. [Component Placement](../../eak/crates/eak-phases/src/component_placement.rs) lays parts in a fixed left-to-right row at a constant 12 mm pitch with no objective function or overlap-avoiding search. Because the stub router has no clearance awareness, routed copper can cross pads, courtyards, and other tracks with nothing detecting it (until **C1** lands).

**Science it unlocks.** [`search-algorithms.md`](../mathematics/search-algorithms.md) (Lee/Dijkstra/A* completeness; rip-up-and-retry); [`graph-theory.md`](../mathematics/graph-theory.md) §4–§5 (Steiner topology then per-connection shortest path); [`optimization-theory.md`](../mathematics/optimization-theory.md) (HPWL/congestion/thermal minimization via convex-relax-then-legalize / simulated annealing).

**Proposed move.** Implement a routing-grid graph with A*/Lee plus rip-up under negotiated congestion, and an analytic/SA placer minimizing HPWL/congestion subject to non-overlap. This is the natural carrier for the multi-segment, multi-layer, via-bearing routes that **C1** (Steiner realization) and **C2** (inner layers, return vias) presuppose. Until then, the "Mapping to the runtime" sections of the three math docs should carry an explicit "Phase-3 deterministic placeholder" caveat (see **G1**).

**Where it slots.** *Use cases* (**Routing Planning**, **Component Placement**). Addresses finding 8.

---

## D1. Corrective loop-back controllers (closed-loop actuation)

**Today.** The watchdog is sound — per-edge `max_retries`, a global cap, and per-machine `max_steps` in the [orchestrator](../../eak/crates/eak-runtime/src/orchestrator.rs) all bound the loop, and the gate blocks on open errors. But the loop-back *targets are idempotent*: on a re-entry, Routing Planning and Component Placement skip every already-committed net/placement and mint nothing, so a DRC width breach, EMC length breach, or DFM clearance breach is never actuated. The violation potential `V` stays constant across passes, the loop exhausts its retries, and a *genuinely correctable* defect fails instead of converging — the CLI comment admits this is "a no-op recovery."

**Science it unlocks.** [`control-theory.md`](../mathematics/control-theory.md) §3–§4 (each accepted step must strictly decrease the Lyapunov potential `V`; negative feedback removes more violations than it adds).

**Proposed move.** Give the loop-back targets real corrective authority — routing widens a sub-floor track on re-entry; placement nudges an edge-violating part inward — so an accepted pass strictly decreases the weighted-violation potential. This builds on the actuators from **B1** (current-derived widening) and **C3** (re-pathing). Until then, document the loop-backs as termination-only watchdogs rather than the negative-feedback correctors the doc portrays.

**Where it slots.** *Use cases* (the looping phases + orchestrator). Addresses finding 10.

---

## D2. Richer constraint network: arc-consistency and typed target roles

**Today.** The [`ConstraintEngine`](../../eak/crates/eak-engines/src/lib.rs) is a stateless pairwise calculator — `satisfies()` (value vs. one bound) and `contradiction()` (interval-disjointness of two constraints) — with `ConstraintConsistencyRule` an O(n²) pairwise scan. There is no domain store, no arc/generalized-arc-consistency propagation to a fixpoint, no applicability index, and no incremental re-check. Separately, Fabrication limits are read by an **untyped positional slot contract**: slot 0 = trace-width floor, slot 1 = edge keep-out. A design that pins only an edge keep-out as its single Fabrication target has it silently misread as the trace floor, while the edge band reverts to a hard-coded 0.5 mm — partly undoing the "fab-sourced, not hard-coded" guarantee.

**Science it unlocks.** [`constraint-satisfaction.md`](../mathematics/constraint-satisfaction.md) (the Constraint Engine as `⟨X, D, C⟩` with arc/GAC propagation to a confluent fixpoint, an applicability index, and incremental re-check whose confluence guarantees gate/live agreement).

**Proposed move.** Two independent steps. (1) Give each Fabrication limit a typed `TargetRole` (e.g. `MinTraceWidth | EdgeKeepout | MinSpace`) so rules select by meaning, not commit position — a small, high-value change that removes the silent-misread path. (2) When the satisfaction/search half is built, add a domain store and arc-consistency propagation; until then, label the propagation/fixpoint machinery in the doc as the *checking-half-only* implementation it actually is (the checking half is faithfully implemented as polynomial predicate evaluation).

**Where it slots.** *Entities* (`TargetRole` on `Requirement` targets) + *Use cases* (**Constraint Engine**).

| Finding | Severity | What D2 supplies |
|---|---|---|
| 24 | minor | a typed target role replacing the positional slot contract |
| 28 | info | honest scoping of propagation/arc-consistency as future, checking-half as shipped |

---

## D3. Statistical / worst-case tolerance propagation

**Today.** Every `PhysicalQuantity` carries a first-class `Tolerance` (None/Relative/Absolute), but **no comparison consults it**: `try_compare` orders by nominal `si_magnitude()`, and every DRC/DFM/EMC rule compares nominal magnitudes with a ~1e-9 float epsilon. A design built from toleranced parts is checked at nominal — the "passes on paper, fails at the band edge" failure mode the probability doc names. (No shipped design populates a non-None tolerance today, so there is no *active* mis-decision — the gap is latent and forward-looking.)

**Science it unlocks.** [`probability-and-statistics.md`](../mathematics/probability-and-statistics.md) §2 (worst-case vs. RSS/statistical propagation; nominal-only comparison named as a failure mode) and the open units-and-quantities §9 ADR on propagation method.

**Proposed move.** Make the Constraint Engine tolerance-aware per a stated policy: check safety/hard bounds against the worst-case interval (magnitude ± tolerance) and yield/cost bounds against an RSS/statistical interval; close the §9 ADR with a concrete default. Until then, stop asserting "tolerance-aware comparison" in the runtime-mapping diagram and units-and-quantities §8 (a **G1** item).

**Where it slots.** *Use cases* (**Constraint Engine**). Addresses finding 9.

---

## E1. Via / plated-hole entity + annular-ring / drill / aspect / mask DRC

**Today.** [DFM Verification](../../eak/crates/eak-phases/src/dfm_verification.rs) registers exactly two rules — both pure board-edge keep-out checks. There is **no via/plated-hole entity** anywhere (`Track` is the only copper struct), so the science layer's flagship DFM example — annular ring — is *structurally uncheckable*, along with minimum drill, aspect ratio, and solder-mask sliver. The state-machine and `dfm-principles.md` mapping list these checks as if they run; they do not.

**Science it unlocks.** [`dfm-principles.md`](../manufacturing/dfm-principles.md) §1–§2 (annular-ring tolerance stack-up; process-capability rules); [`manufacturing-constraints.md`](../manufacturing/manufacturing-constraints.md) clauses 2/3/5 (annular ring, aspect ratio, solder-mask sliver); [`ipc-standards.md`](../manufacturing/ipc-standards.md) (IPC-2221/6012/A-600).

**Proposed move.** Add a `Via`/`PlatedHole` domain entity (shared with **C2**'s via work), then annular-ring (`AR = (D_pad − D_hole)/2 − registration`), minimum-drill, and aspect-ratio (`T_board/D_drill ≤ AR_max`) DFM rules, plus mask-sliver checks. Until then, downgrade the [`dfm-verification.md`](../../docs/state-machines/dfm-verification.md) EvaluatingRules list and `dfm-principles.md` to state the implemented scope is board-edge keep-out only.

**Where it slots.** *Entities* (`Via`/`PlatedHole`) + *Use cases* (**DFM Verification**). Addresses finding 21.

---

## E2. Manufacturing-IR completeness: drill, mask, panel, fiducials, DFT, rolled yield

**Today.** The [`ManufacturingIr`](../../eak/crates/eak-compiler/src/lib.rs) is a single-board dataset (board, placements, assignments, copper, line items) with **no drill table, mask geometry, stack-up, panel/array, rail, fiducial, tooling-hole, or breakaway concept**. The pick-and-place output carries no fiducial/coordinate-frame data, so by the science layer's own §4 reasoning it is an incomplete assembly directive. The §6 thesis — `Y_fab = e^(−D·A)`, `Y_asm`, `Y_total` as a *release predicate* — is implemented nowhere; the manufacturing gate is a binary open-blocking-violation count, not a yield/cost predicate.

**Science it unlocks.** [`dfm-principles.md`](../manufacturing/dfm-principles.md) §3 (panel utilization / N-per-panel cost), §4 (DFT coverage; ≥3 non-collinear fiducials for the placement coordinate frame), §6 (yield as a computable design output); [`ipc-standards.md`](../manufacturing/ipc-standards.md) §6 (IPC-2581/ODB++ neutral exchange — completeness with internal consistency); [`manufacturing-constraints.md`](../manufacturing/manufacturing-constraints.md) clause 6.

**Proposed move.** Extend `ManufacturingIr` with the drill/mask/stack-up output sets ([`manufacturing-ir.md`](../../docs/compiler/ir/manufacturing-ir.md) already specifies them) — sourcing stack-up from **A1** and holes from **E1** — and add a fiducial requirement to the assembly set, a panel-fit/utilization check, and a `Y_total` release predicate alongside the open-blocking count. As a minimum first step, remove "panelization" (and the equally-unimplemented "acid traps"/"slivers") from the EvaluatingRules list until implemented.

**Where it slots.** *Entities* + *Use cases* (**Manufacturing Generation**, `eak-compiler`).

| Finding | Severity | What E2 supplies |
|---|---|---|
| 22 | minor | panel/fiducial/DFT representation + a rolled-yield release predicate |
| 23 | minor | drill table, mask geometry, and stack-up in the released package |

---

## F1. Per-net frequency / edge-rate EMC + loop-area model

**Today.** `EmcAntennaLengthRule` ([`eak-engines`](../../eak/crates/eak-engines/src/lib.rs)) takes the single highest Frequency-dimensioned target across **all** requirements and tests **every** track against that one critical length, with no association between a frequency and the net that carries it — so a DC power/ground track is judged an antenna at a radio's frequency, and a slow-clock/fast-edge net is under-classified because the boundary keys off stated sinusoidal frequency, not rise-time. The threshold uses free-space `c` (the on-board limit should use `c/√ε_eff`, ≈ half), and the metric is single-track straight-line length, structurally blind to **loop area** — which is what actually sets differential-mode emission (`|E| ∝ f²·A·I`). These are honestly documented as a lenient, safe-direction proxy.

**Science it unlocks.** [`transmission-lines.md`](../electrical/transmission-lines.md) (the edge, not the clock, sets the boundary: `f_knee ≈ 0.5/t_r`); [`signal-integrity.md`](../electrical/signal-integrity.md) (frequency content is per-net); [`emi-emc.md`](../pcb/emi-emc.md) and [`return-path.md`](../pcb/return-path.md) (radiation is set by loop area, not forward length).

**Proposed move.** Associate an operating/edge-rate attribute with the net (or net class) so the test runs against that net's frequency content; derive the boundary from `f_knee` when a rise time is available; exclude DC power/ground nets; evaluate electrical length per *realized Net* (sum of routed segments), not per straight Track; apply an `ε_eff` velocity factor once **A1**'s dielectric data exists; and extend the analysis toward a loop-area / return-path estimate once **C2** provides the plane model.

**Where it slots.** *Entities* (edge-rate on `Net`) + *Use cases* (**EMC Analysis**).

| Finding | Severity | What F1 supplies |
|---|---|---|
| 16 | minor | per-net frequency + edge-rate (`f_knee`); DC-net exclusion |
| 29 | info | per-realized-net length; velocity-factor tightening |
| 31 | info | loop-area emission model (depends on C2) |

---

## G1. Runtime-mapping documentation-honesty pass

**Today.** Several runtime-mapping documents assert, in present tense and inside sections that promise "exact symbols from the implementation," capabilities the code does not have. This is the one cluster that is *purely editorial* — no code change is required for correctness — but it is load-bearing for trust in the science layer, so it is worth treating as a structural pass rather than scattered one-line fixes.

**Science it unlocks.** Nothing new physically; it restores the **runtime-mapping honesty contract** — a mapping row must resolve to a real runtime symbol, and a present-tense claim must match shipped code — which the crosswalk itself declares binding.

**Proposed move.** A single editorial sweep across [`concept-runtime-crosswalk.md`](../runtime-mapping/concept-runtime-crosswalk.md), [`compiler-ir-mapping.md`](../runtime-mapping/compiler-ir-mapping.md), and the verification/constraint mapping docs, plus the over-stating sentences in the physics/manufacturing docs flagged elsewhere. Each correction either restates a capability as future scope ("the IR *will* carry…") or fixes a misattribution.

**Where it slots.** *Science / docs* (`engineering-science/runtime-mapping/`).

| Finding | Severity | The specific overstatement |
|---|---|---|
| 7 | major | crosswalk calls reasoner `model_id`s (`trace-floor`/`load-only`/`high-speed`) "net classes" and invents a `high-speed` net class + per-class clearance — replace with the real `{Power, Ground, Signal}` |
| 25 | minor | docs claim `width = max(ampacity, IR-drop)` and current-sized widths; runtime uses constants — restate as fixed per-class defaults, fix the `eak-engines`→`eak-phases` crate attribution |
| 26 | minor | P7 row lists "footprint↔symbol agree" and a "typed Layer Stack (copper weight)" as enforced; neither exists — move to a not-yet-implemented note (or land via A1) |
| 27 | minor | crosswalk says rule structs are "implemented in `eak-phases/src/`"; they are defined in [`eak-engines/src/lib.rs`](../../eak/crates/eak-engines/src/lib.rs) and only *registered* per phase — align with the two sibling docs |

---

## Dependency ordering and leverage

The proposals are not independent; they form a short dependency spine. **A1 (the typed stack-up) is the keystone**: it is the sole prerequisite for B1 (ampacity needs `t`), B2 (impedance needs `ε_r`/`h`/`t`), B3 (thermal needs the K/W and m² units), and the velocity-factor and loop-area refinements in F1; it also supplies the stack-up output E2 must lower. **C2 (the reference-plane model)** is the second foundation: it unlocks the return-path checker, multi-layer/via routing, the crosstalk rule, and the loop-area EMC metric, and it shares its `Via` entity with E1's annular-ring work. **C3 (real search)** is what lets C1's Steiner realization, C2's inner-layer routing, and D1's re-pathing actuator become real rather than placeholder. The remaining moves — D2's typed target role, D3's tolerance policy, and the entire G1 documentation sweep — are independent and can land at any time; the typed target role and the G1 sweep are the lowest-cost, highest-trust items in the set.

A reasonable sequencing, were these to be pursued, is: **G1 + D2(role) + D3** (cheap, independent) → **A1** (keystone) → **B1 / B2 / B3** (the electrical/thermal payoff of A1) → **C1** (connectivity correctness, partly independent) → **C2 → C3 → D1** (the routing/return-path/closed-loop chain) → **E1 → E2** and **F1** (manufacturing and analysis completeness). None of this is committed; it is the order in which the science the docs already describe would become *enforceable* rather than *aspirational*.

## Scope note

These thirteen proposals are recommendations surfaced by an architecture audit of a mid-lifecycle product. The shipped Phases 1–3 are internally honest about their boundaries — the per-class widths, the EMC proxy, the fab-sourced floors, and the bounded watchdog are all sound and self-documented. The improvements here are about *raising the altitude of enforcement*: turning entities the spec already names (stack-up, plane, via, plated hole) and computations the science already derives (ampacity, impedance, junction temperature, return continuity, rolled yield) into first-class, gateable objects in the runtime. Adopting any subset is a product decision; this document exists to make the structural options, their dependencies, and the science each one unlocks explicit.
