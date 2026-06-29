# PCB — Layout Engineering Principles

This folder is the **layout engineering** wing of the Engineering Science Layer: the physical "why" behind how copper, planes, and components are arranged on a board. The runtime's physical-design phases — floor planning, placement, routing planning, DRC, EMC, and manufacturing generation — realize a [Net](../../docs/foundation/engineering-domain-model.md#net) as geometry while stating only one invariant (connectivity). These documents supply the laws that geometry must obey to be *correct* rather than merely *connected*: stackup and placement that fix routability and impedance before any track exists; routing, return-path, ground-plane, and power-distribution theory that decide where current actually flows and returns; and the specialized regimes — differential pairs, high-speed, analog/mixed-signal, and EMI/EMC — where parasitics, not connectivity, are the signal.

| Document | What it grounds |
|----------|-----------------|
| [./stackup.md](./stackup.md) | The vertical layer/dielectric cross-section decided before routing — precondition for controlled impedance, PI, and EMC; grounds the PCB IR layer stack and floor planning. |
| [./placement.md](./placement.md) | Assigning position, rotation, and side to every part — the most consequential decision; wirelength, Rent's rule, and critical-net-first order that fix routability, SI, PI, and thermal outcomes. |
| [./routing.md](./routing.md) | Turning the logical netlist into physical tracks/vias/layers; the correct-vs-merely-connected distinction, congestion as a finite shared resource, and via cost. |
| [./ground-plane.md](./ground-plane.md) | The continuous copper return conductor and voltage reference; return-current physics, the cost of splits and slots, stitching vias, and single- vs multi-point grounding. |
| [./return-path.md](./return-path.md) | Where signal return current flows — least impedance, not least resistance — and what it costs to break the path; grounds reference-plane continuity and loop-area physics. |
| [./power-distribution.md](./power-distribution.md) | The topology of the power-delivery network — planes vs traces, star vs grid, regulator-to-load path — keeping IR drop budgeted and conductors below ampacity. |
| [./differential-pairs.md](./differential-pairs.md) | Two coupled conductors carrying one signal as a difference; the four impedances, tight coupling, balance/CMRR, and skew that connectivity checks cannot see. |
| [./high-speed-design.md](./high-speed-design.md) | Making timing and waveform fidelity first-class — the edge-rate classification gate, flight-time/skew budgets, via-stub/back-drill cost, and topology choice. |
| [./analog-layout.md](./analog-layout.md) | Arranging copper so microvolt signals survive a volt/amp-scale board; the four coupling mechanisms, guarding/Kelvin/partitioning, and single-point grounding. |
| [./emi-emc.md](./emi-emc.md) | Why electrically-long conductors radiate — every conductor is an antenna; reciprocity ties emissions to immunity and grounds the `emc-antenna-length` rule. |

---

Up to the layer index: [../README.md](../README.md) · Across to the concept→runtime crosswalk: [../runtime-mapping/concept-runtime-crosswalk.md](../runtime-mapping/concept-runtime-crosswalk.md)
