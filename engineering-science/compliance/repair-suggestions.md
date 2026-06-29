# Repair Suggestions

This is a prioritized, executable backlog to close the 32 confirmed findings from the architecture review (0 critical, 7 major, 20 minor, 5 info). The findings cluster around one structural root cause: the implemented domain model reduces the board cross-section to a bare integer layer count, so copper weight, dielectric height, ε_r, plane roles, and reference adjacency simply do not exist as data. That single gap makes thermal, impedance, return-path, ampacity, differential-pair, and via reasoning *unrepresentable* rather than merely unchecked, and it is the reason a cluster of science-layer docs overstate the shipped runtime in the present tense. Consequently the highest-leverage move is to introduce a typed layer-stack entity first (**R1**); it unblocks five of the seven majors and roughly a dozen minors. Alongside it sit two independent, self-contained correctness rules — a connectivity/short DRC (**R2**) and a copper-to-copper clearance DRC (**R3**) — that close the remaining majors with bounded effort. Everything that needs a *value a human or datasheet would supply* (θ_JA, per-net current, which nets are 50 Ω, edge rates) is deferred to the reasoning-driven "Later" bucket, because the structural and checking machinery can and should be built now while the value sources wait on the deferred `live`/Datasheet-Intelligence work. A cheap, high-honesty documentation pass (**RD**) runs in parallel: every major/minor with a "doc overclaims the code" half can be made truthful today with a few present-tense-to-future edits, independent of when the code lands.

The grouping is: **Do-now (structural code)** → **Do-now (documentation honesty pass)** → **Later (reasoning-driven)**. Within each, order is severity then leverage.

---

## 1. Finding index and traceability

Findings are numbered F1–F32 in review order. "Primary repair" is the code/structural fix that truly closes the finding; "Interim/doc" is the cheap honesty edit that keeps the science layer truthful until the primary repair ships (and, for doc-only findings, *is* the close).

| ID | Sev | Dimension | Finding (short) | Primary repair | Interim / doc |
|----|-----|-----------|-----------------|----------------|---------------|
| F1 | major | math | Net realization checked by track existence, not connectivity; no union-find, no short detection | R2 | RD-math |
| F2 | major | physics | No copper weight/thickness and no K/W or m² units — IPC-2152 cross-section & T_j unrepresentable | R1 + R4 + R9 | RD-phys |
| F3 | major | electrical | No controlled-impedance net class / Z_0 target; signal width is geometry- and stackup-blind | R1 + R6 | RD-phys |
| F4 | major | pcb | Stackup is a bare layer count — no h, ε_r, copper t, or plane/reference adjacency | R1 | RD-phys |
| F5 | major | pcb | No reference-plane / copper-pour entity and no return-path continuity rule | R1 + R5 | — |
| F6 | major | manufacturing | No copper-to-copper clearance / minimum-space / short DRC rule | R3 | — |
| F7 | major | consistency | Crosswalk mislabels test reasoner names as "net classes" and invents a "high-speed" net class | RD-crosswalk | RD-crosswalk |
| F8 | minor | math | Routing/placement are deterministic geometric stubs, not the mapped search/optimization | R3 (+ later L5) | RD-math |
| F9 | minor | math | Tolerances carried but never consulted — comparisons are nominal-only | R-tol (see L-bucket) | RD-math |
| F10 | minor | math | Loop-backs are bounded but the controllers are idempotent (cannot reduce the error) | R11 | RD-math |
| F11 | minor | physics | Trace width is geometric; the only width rule checks the fab floor, not self-heating | R9 | RD-phys |
| F12 | minor | physics | Dielectric / per-layer thickness absent — Z_0 uncomputable | R1 | RD-phys |
| F13 | minor | physics | Return path / reference plane not modeled — ideal-return assumption | R1 + R5 | — |
| F14 | minor | electrical | Stackup carries no copper weight or ε_r — R = ρL/(w·t) and Z_0 uncomputable | R1 | RD-phys |
| F15 | minor | electrical | Width is a fixed per-class constant; no IR-drop budget; nets/tracks carry no current | R9 | RD-crosswalk |
| F16 | minor | electrical | EMC rule applies one global worst-case f to every track (incl. DC) and keys off clock f, not edge rate | R12 | — |
| F17 | minor | electrical | No crosstalk / return-path / reference-plane reasoning; routing is single-side | R3 + R5 | — |
| F18 | minor | pcb | Differential pairs unrepresentable — no pair partner, no diff class, no Z_diff/skew subject | R7 | RD-phys |
| F19 | minor | pcb | Routing locked to one outer layer and one straight segment — no inner layers, no vias | R8 | — |
| F20 | minor | pcb | Per-net-class widths are flat constants disconnected from impedance and current | R6 + R9 | RD-phys |
| F21 | minor | manufacturing | DFM covers only edge keep-out; annular ring, drill, aspect-ratio, mask-sliver absent | R8 | RD-dfm |
| F22 | minor | manufacturing | Panelization, fiducials, DFT not represented; rolled yield never computed | R13 | RD-dfm |
| F23 | minor | manufacturing | Manufacturing IR has no drill table / mask geometry / stack-up, but the doc claims it does | R1 (+ R8) | RD-dfm |
| F24 | minor | manufacturing | Positional fab-limit slot contract can silently misread a lone fab limit's role | R10 | — |
| F25 | minor | consistency | Docs claim trace width is ampacity/IR-drop-sized; runtime uses constants (and wrong crate) | RD-ir-mapping | RD-ir-mapping |
| F26 | minor | consistency | P7 row lists "footprint↔symbol agree" and "typed Layer Stack" as implemented; neither exists | RD-ir-mapping | RD-ir-mapping |
| F27 | minor | consistency | Crosswalk says rule structs live in eak-phases; they are all in eak-engines/src/lib.rs | RD-crosswalk | RD-crosswalk |
| F28 | info | math | Arc-consistency / propagation-to-fixpoint is aspirational; engine is stateless pairwise | RD-math | RD-math |
| F29 | info | physics | EMC rule compounds straight-line length with free-space c — under-reports radiators | R12 | RD-phys |
| F30 | info | electrical | Signal net class is never minted by the deterministic pipeline (power/ground only) | R12 / L | RD-scope |
| F31 | info | pcb | EMC rule uses free-space c and scores forward length, never loop area | R12 (+ R5) | RD-phys |
| F32 | info | manufacturing | Confirmed well-handled: edge keep-out & width floor are genuinely fab-sourced (positive) | none (retire proxy via R1+R9) | — |

---

## 2. Do-now — structural code repairs

Ordered by severity then leverage. Effort key: **S** ≈ hours, **M** ≈ 1–2 days, **L** ≈ multi-day, **XL** ≈ a phase.

| ID | Repair (what / where) | Closes | Effort | Depends on |
|----|-----------------------|--------|--------|------------|
| **R1** | **Typed layer-stack entity (keystone).** Introduce an ordered `LayerStack` on `Board`/`PcbIr` — each layer a `PhysicalQuantity` dielectric height + ε_r + tan δ + copper thickness (from copper weight) + role `Plane`/`Signal`, plus a per-signal-layer reference binding. Populate it in PCB Floor Planning and validate it in the floor-plan gate. | F4 (code); unblocks F2, F3, F5, F12, F13, F14, F18, F19, F20, F23 | L | — |
| **R2** | **Connectivity + short DRC via union-find.** Add a copper-graph disjoint-set rule: union pads joined by a track, then require every net's pads to share one component (else *open*) and no two distinct nets to share a component (else *short*). Replace the single first-to-last centroid `Track` with a ≥k−1-segment spanning/Steiner realization so interior pins land on copper. | F1 | M | — |
| **R3** | **Copper-to-copper clearance / minimum-space DRC.** New rule: any two tracks on the same `BoardSide` whose edge-to-edge gap is below the fab minimum space (a fab-sourced slot, voltage-scaled per IPC-2221 where peak net voltage is known) is a violation, with a same-net exemption. This also gives the stub router a guard so it cannot emit silent shorts. | F6; mitigates F8, F17 | S–M | — |
| **R4** | **Unit-system extension.** Add `ThermalResistance` (K/W) and `Area` (m²) to `Dimension`/`Unit` so θ_JA can be typed and compared and IPC-2152 cross-section can be expressed. | F2 (units half) | S | — |
| **R5** | **Reference-plane/pour entity + continuity rule.** Add a `Plane`/`Pour` domain entity bound to a `Net`; expose plane geometry on `VerificationContext`; add a reference-continuity rule (no plane split/void under a controlled net's trace projection; controlled nets require an adjacent continuous reference), net-class scoped, looping back to Routing on failure. | F5, F13, F17 | M–L | R1 |
| **R6** | **Controlled-impedance class + impedance-driven width.** Add a controlled-impedance net class (or an Ω-dimensioned impedance-target `Constraint`); once the stack carries ε_r/h/t, compute width from the IPC-2141 microstrip/stripline forms; apply an ε_eff velocity factor to the electrically-long threshold and let it gate impedance/termination, not only emissions. | F3, F20 (impedance half) | M–L | R1 |
| **R7** | **Differential-pair representation.** Add a pair binding (partner net/track id) and a `Differential` class carrying Z_diff and ΔL_max as typed constraints; route the pair together at constant spacing and evaluate the skew length bound. | F18 | M | R1, R6 |
| **R8** | **Via/plated-hole entity + multi-layer routing + hole DFM rules.** Add a `Via`/`PlatedHole` entity and multi-segment, inner-layer-capable tracks (layer index into the stack); add annular-ring, minimum-drill, and aspect-ratio DFM rules; require a return/stitching via at each reference change. | F19, F21 | L | R1 |
| **R9** | **Ampacity / IR-drop / junction-temp evaluators.** Add a worst-case current to `Net` (or per-net override); compute width = max(ampacity(I, ΔT, t), IR-drop ρ·L·I/(t·ΔV)) clamped to the process floor; add a rule failing a rail whose I·R_path exceeds its ΔV budget and a thermal rule T_j = T_amb + θ_JA·P so the gate can block over-temperature designs. | F2 (eval half), F11, F15 | L | R1 (copper t), R4 |
| **R10** | **Typed fab-limit roles.** Replace the positional slot contract with a `TargetRole` discriminator (`MinTraceWidth | EdgeKeepout | MinSpace | …`) on the Fabrication target so rules select by meaning, not commit order; reject a Fabrication requirement whose lone limit could be misread as a trace floor. | F24 | S | — |
| **R11** | **Give loop-backs corrective authority.** On re-entry, let Routing widen a sub-floor track and Placement nudge an edge-violating part inward, so an accepted pass strictly decreases the weighted-violation potential instead of re-running an identical violation to retry-exhaustion. | F10 | M | — |
| **R12** | **EMC per-net, edge-rate-aware evaluation.** Evaluate electrical length per realized `Net` (sum of routed segment lengths), exclude DC power/ground nets from the radiator test, and derive the boundary from f_knee ≈ 0.35–0.5/t_r when a rise time is available; apply the ε_eff velocity factor once the stack provides it. | F16, F29, F31; activates F30 once signal nets exist | M | R1 (velocity factor) |
| **R13** | **Assembly/yield completeness.** Add fiducials (≥3 non-collinear) and a coordinate frame to the assembly output, a panel-fit/utilization check, and a rolled-yield predicate (Y_fab·Y_asm) so the Manufacturing IR is a complete handoff rather than a single-board dataset gated only by an open-violation count. | F22 | M–L | — |

> **F32** is a positive confirmation (edge keep-out and the trace-width floor are genuinely fab-sourced and correctly stay silent when no floor is stated). No repair is needed; R1 + R9 are the path to retire the per-class-constant proxy later.

---

## 3. Do-now — documentation honesty pass

These are cheap, code-free edits to the just-authored science layer. Each makes a present-tense overclaim truthful by scoping it to "will / future" or correcting a symbol/location. They can ship immediately and independently of the code repairs, and they fully close the doc-only findings (F7, F25, F26, F27, F28).

| ID | Target doc(s) | Edit | Closes / keeps honest |
|----|---------------|------|------------------------|
| **RD-crosswalk** | [../runtime-mapping/concept-runtime-crosswalk.md](../runtime-mapping/concept-runtime-crosswalk.md) | Replace "net classes (`trace-floor`, `load-only`, `high-speed`)" with the real `NetClass` {Power, Ground, Signal}; delete every "high-speed net class" and per-class "clearance" reference (mark high-speed/diff-pair as future, or map SI/RF rows to `emc-antenna-length` / "not yet implemented"). Change "rule structs … implemented in eak-phases/src/" to "defined in [../../eak/crates/eak-engines/src/lib.rs](../../eak/crates/eak-engines/src/lib.rs) and registered by the per-phase machines in eak-phases." | F7, F27; interim F15 |
| **RD-ir-mapping** | [../runtime-mapping/compiler-ir-mapping.md](../runtime-mapping/compiler-ir-mapping.md) | P9 row: state width is a fixed per-`NetClass` default (Power/Ground 0.50 mm, Signal 0.25 mm) and move `max(ampacity, IR-drop, floor)` to an explicitly labelled target/aspirational note; fix the sizing-crate attribution from eak-engines to eak-phases. P7 row: drop "footprint↔symbol agree" and "typed Layer Stack / copper weight" from the present-tense enforcement columns (Board carries only a layer count) or soften the "exact implemented symbols" table intro; keep invariant #1 as a labelled forward-looking requirement. | F25, F26 |
| **RD-phys** | [../physics/thermal-physics.md](../physics/thermal-physics.md), [../physics/rf-physics.md](../physics/rf-physics.md), [../pcb/stackup.md](../pcb/stackup.md), [../pcb/ground-plane.md](../pcb/ground-plane.md), [../electrical/transmission-lines.md](../electrical/transmission-lines.md), [../electrical/ohms-law.md](../electrical/ohms-law.md), [../pcb/differential-pairs.md](../pcb/differential-pairs.md), [../../docs/compiler/ir/pcb-ir.md](../../docs/compiler/ir/pcb-ir.md) | Restate every "the IR/Board already carries / persists / stores the stack-up (thicknesses, copper weight, ε_r)" and "width is chosen so the trace hits Z_0" / "checkable today as a length bound" / "the width pre-check in ValidatingRouting consults the Constraint Engine for the ampacity bound" assertion as deferred ("the IR will carry…", "Phase-3 widths are constants"). Mark θ_JA/K/W/mm² delivery, the differential-pair partner field, and ValidatingRouting/ValidatingFloorPlan stack-up gating as future until R1/R6/R7/R9 land. | F2, F3, F4, F11, F12, F14, F18, F20 (doc halves); F29, F31 (already honest, keep) |
| **RD-dfm** | [../manufacturing/dfm-principles.md](../manufacturing/dfm-principles.md), [../../docs/state-machines/dfm-verification.md](../../docs/state-machines/dfm-verification.md), [../manufacturing/manufacturing-constraints.md](../manufacturing/manufacturing-constraints.md) | In the "implemented behaviors" mapping, state the shipped DFM scope is **board-edge keep-out only**; move acid traps, solder-mask slivers, annular ring, component-assembly spacing, and panelization to an explicit "deferred — no through-hole/panel geometry modeled" note; reword the line asserting the Manufacturing IR "carries the drill table, mask geometry, … and stack-up" to match the implemented outline + copper + placement/BOM set. | F21, F22, F23 (doc halves) |
| **RD-math** | [../mathematics/graph-theory.md](../mathematics/graph-theory.md), [../mathematics/search-algorithms.md](../mathematics/search-algorithms.md), [../mathematics/optimization-theory.md](../mathematics/optimization-theory.md), [../mathematics/control-theory.md](../mathematics/control-theory.md), [../mathematics/constraint-satisfaction.md](../mathematics/constraint-satisfaction.md), [../mathematics/probability-and-statistics.md](../mathematics/probability-and-statistics.md) | Down-scope the "Mapping to the runtime" sections: connected-components "no opens/no shorts" and `ValidatingRouting` are not yet implemented (until R2); routing/placement are Phase-3 deterministic placeholders, not grid/A*/Steiner/SA search; the loop-backs are termination-only watchdogs, not negative-feedback correctors (until R11); arc-consistency/propagation-to-fixpoint/applicability-index is the future search half (engine implements the checking half); the Constraint Engine compares nominal magnitudes, not tolerance-aware/worst-case/RSS intervals — and close the units-and-quantities §9 ADR with a stated default. | F1, F8, F9, F10, F28 (doc halves) |
| **RD-scope** | [../../docs/state-machines/schematic-planning.md](../../docs/state-machines/schematic-planning.md) and routing notes | Record that Phase-3 deterministic output is power/ground-only — `NetClass::Signal` is not minted end-to-end — so the signal-width path and SI rules have no live subject yet. | F30 |

---

## 4. Later — reasoning-driven (needs `live` / Datasheet Intelligence)

These items are blocked not on structure or algorithms but on a **value or judgement a datasheet/LLM must supply**. Build the structural machinery now (R1, R4, R6, R7, R9, R12) so these become a matter of feeding in a number, then activate them when the deferred `live` feature lands.

| ID | Reasoning-driven input | Activates | Closes (reasoning half) |
|----|------------------------|-----------|--------------------------|
| **L1** | θ_JA / θ_JC per part (Datasheet Intelligence) | R9 thermal evaluator → a real over-temperature gate | F2 |
| **L2** | Per-net worst-case current (KCL load sum + datasheet draw) | R9 ampacity / IR-drop width and budget rule | F11, F15 |
| **L3** | Impedance-target intent per interface — which nets are 50 Ω / 100 Ω-diff | R6 / R7 impedance-driven width and skew | F3, F18, F20 |
| **L4** | Edge rate / rise time per signal net (datasheet) | R12 f_knee electrically-long boundary | F16 |
| **L5** | (Deterministic, larger phase — not strictly reasoning) Real router (A*/Lee + rip-up under negotiated congestion) and analytic/SA placer (HPWL/congestion/thermal) | The full algorithm claims in the math docs | F8 (structural half) |

> **L5 caveat:** unlike L1–L4 this is a deterministic build, not a reasoning gate; it is listed under "Later" only because it is a phase-scale effort. The do-now obligation for F8 is the **RD-math** scope-down plus **R3** so the stub router cannot produce silent shorts in the interim.

---

## 5. Recommended sequencing

1. **Documentation honesty pass first (RD-\*).** Hours of work, no code risk, and it immediately retires F7, F25, F26, F27, F28 and makes the science layer truthful about every other gap. Do this before any code so the docs stop asserting capabilities the next reader will look for.
2. **R1 (layer-stack keystone) next.** It is the single dependency of R5, R6, R7, R8, R9 and the data root of F2/F3/F4/F5/F12/F13/F14/F18/F19/F20/F23. Nothing in the impedance/return-path/thermal cluster can be correct before it exists.
3. **R2 and R3 in parallel with R1.** Both are independent, self-contained correctness rules that each close a major (F1, F6) and remove the two silent-failure paths — an open multi-pin net and an overlapping-copper short — that currently pass DRC and the manufacturing gate. **R4** (units) is a small independent prerequisite for R9 and can land any time.
4. **After R1:** R5 (return-path), R6 (impedance), then R7 (diff-pairs) and R8 (vias/holes); R9 once R1 + R4 are in. R12, R10, R11, R13 are independent and can slot in by priority.
5. **Activate L1–L4** as the `live`/Datasheet-Intelligence feature lands, turning the R6/R9/R12 proxies into true physics checks. Schedule **L5** as its own routing/placement phase.

The two majors that gate electrical correctness on the bench — **R2** (no opens/shorts) and **R3** (no copper shorts) — plus the **R1** keystone are the minimum set a maintainer should land before describing any output as a "DRC-clean, releasable" board, because today a connectivity-incomplete or shorted board clears every shipped check and lowers to a released Manufacturing IR.
