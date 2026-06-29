# Electrical

Circuit- and signal-level laws that bridge raw physics to the schematic and the PCB. Where the [physics](../physics/) layer describes fields in a medium, this folder reduces those fields to the **lumped network the schematic actually lives in** — nodes, branches, voltages, currents — and states precisely when that reduction holds (the `λ/10` electrical-smallness boundary) and where it breaks back into distributed behavior. These documents ground the runtime's connectivity checks, current/voltage/power budgets, trace-width and impedance constraints, and the regulator rail split: every time the engine treats a net as one voltage, sizes copper, or admits an impedance target, it is invoking a law stated here.

## Documents

| Document | What it grounds |
|----------|-----------------|
| [./circuit-theory.md](./circuit-theory.md) | The lumped-element abstraction itself — reducing a board's continuous field to a solvable network of idealized elements at equipotential nodes; the single `λ/10` precondition under which the Schematic IR is a valid lumped network, and where it breaks. |
| [./ohms-law.md](./ohms-law.md) | DC reasoning over copper — `V = I·R` and `P = V·I` turn trace geometry into IR-drop and `I²·R` self-heating budgets behind per-net-class widths, the DRC trace-width floor, ampacity, and rail sizing. |
| [./kirchhoff-laws.md](./kirchhoff-laws.md) | The axioms of connectivity — KCL (charge conserved at a node) and KVL (energy conserved around a loop) make a Net a meaningful object, justify every ERC/power-rail check, and prove a regulator's VIN and VOUT must be distinct nets. |
| [./transmission-lines.md](./transmission-lines.md) | The distributed model that supersedes the single node once a conductor is electrically long — characteristic impedance `Z_0`, propagation delay, and reflections; grounds controlled-impedance constraints, stack-up, and length matching. |
| [./signal-integrity.md](./signal-integrity.md) | Preserving an edge's shape and timing across the interconnect — reflections, crosstalk, ISI, ringing, ground bounce, and jitter, all derived from the rise-time-set knee frequency that fixes every SI budget. |
| [./power-integrity.md](./power-integrity.md) | The Power Delivery Network as a frequency-dependent impedance `Z(f)` that must stay below a target `Z_target` from DC to load bandwidth — decoupling, plane capacitance, and loop inductance; the AC sequel to Ohm's law. |

---

Up: [Engineering Science Layer index](../README.md) · Across: [Concept → Runtime crosswalk](../runtime-mapping/concept-runtime-crosswalk.md)
