# Manufacturing — Engineering Science Layer

**Role.** This folder grounds **fabrication and assembly reality**: the body of process facts that decide whether an *electrically correct* board can also be *physically built* at acceptable yield. It explains why a Design-for-Manufacturability (DFM) limit is a fab-process fact reified as a machine-checkable constraint, why every floor value is *sourced* from a real process rather than chosen, which IPC standard governs each design-to-build seam, and why **yield is a computable design output, not a post-hoc surprise**. These docs supply the *why* behind the runtime's [DFM Verification](../../docs/state-machines/dfm-verification.md) phase, its board-edge keep-out, and the [Manufacturing Generation](../../docs/state-machines/manufacturing-generation.md) release gate.

## Documents

| Document | What it grounds |
|----------|-----------------|
| [./dfm-principles.md](./dfm-principles.md) | DFM as a discipline: tolerance stack-up, process capability (Cp/Cpk), and defect density make first-pass **yield a predictable, constrainable design output** before fabrication. |
| [./ipc-standards.md](./ipc-standards.md) | The IPC standards family (2221, 2152, 7351, A-600/A-610, 2581/ODB++) as the shared grammar between designer and fab — mapping each clause to the runtime requirement, DRC/DFM rule, or Manufacturing IR that embodies it. |
| [./manufacturing-constraints.md](./manufacturing-constraints.md) | The concrete fab/assembly limits (min trace/space, annular ring, drill, aspect ratio, edge clearance, mask sliver, panelization) and the **provenance** rule that each is a fabricator-process limit, never a universal constant. |

## See also

Up to the layer root index: [../README.md](../README.md) · Across to the runtime crosswalk: [../runtime-mapping/concept-runtime-crosswalk.md](../runtime-mapping/concept-runtime-crosswalk.md).
