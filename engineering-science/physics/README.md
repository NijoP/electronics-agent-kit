# Physics

First-principles physics the board obeys. These documents state the laws of fields, waves, heat, materials, and semiconductor devices that the EAK runtime never solves directly yet silently assumes every time it routes a net, sizes a power trace, checks a clearance, picks a part, or gates manufacturing. They are the root layer of the Engineering Science stack: where the higher [electrical](../electrical/) and [PCB](../pcb/) docs reason in lumped circuits and layout rules, these ground *why* those rules are theorems rather than conventions — when the runtime asserts a board will pass EMC, stay within ratings, or run cool, it is asserting a claim about physics.

| Document | What it grounds |
|----------|-----------------|
| [./maxwell-equations.md](./maxwell-equations.md) | The four coupled field laws plus charge continuity — the root axioms from which SI, PI, and EMC derive; yields the load-bearing rule that return currents must form closed loops and follow the reference plane. |
| [./electromagnetics.md](./electromagnetics.md) | The field theory of `E`/`H` around copper: a trace stores energy in its dielectric, carries a return current, and radiates — grounding loop-area, controlled-impedance, clearance, and stack-up reasoning. |
| [./rf-physics.md](./rf-physics.md) | High-frequency interconnect — when copper becomes a transmission line or unintentional antenna; the electrically-long `c/(10·f)` criterion behind the EMC antenna-length rule, plus impedance, reflections, VSWR, and S-parameters. |
| [./semiconductor-physics.md](./semiconductor-physics.md) | Device-level physics behind schematic symbols: the `pn` junction, MOSFET threshold/Miller/SOA, and the non-ideal real capacitor — grounding absolute-maximum ratings, pin driver rules, and junction-temperature limits. |
| [./thermal-physics.md](./thermal-physics.md) | How heat is generated, stored, and transported: conduction/convection/radiation, the `θJA`/`θJC` thermal-resistance network, copper-as-heatsink and thermal vias, and derating against `T_j,max`. |
| [./materials-science.md](./materials-science.md) | What the board is actually made of — FR-4, copper foil, solder — and how its numbers move: frequency-dependent Dk/Df, roughness-driven copper loss, intermetallic solder joints, and `Tg`/CTE/MSL reliability limits. |

---

Up: [Engineering Science Layer index](../README.md) · Across: [Concept ↔ Runtime crosswalk](../runtime-mapping/concept-runtime-crosswalk.md)
