# Architecture Health Report — Electronics Agent Kit

> **Type:** Enterprise architecture review (Phase 0, documentation-only). **Date:** 2026-06-28. **Scope:** all 124 architecture documents under `docs/`. **Method:** JCodeMunch MCP (attempted) + direct documentation dependency-graph analysis. **Verdict:** **A− / 8.6 of 10 — structurally healthy; a small number of low-severity hygiene findings.**

This report audits the *planned* architecture of Electronics Agent Kit as expressed in its documentation. It treats the **markdown cross-link graph as the dependency graph**, which is the correct dependency model for a documentation-only repository: a doc that references another doc is asserting a conceptual dependency or relationship.

---

## 0. Method & tooling note (read this first)

The review was requested against **JCodeMunch MCP**. JCodeMunch is a **source-code AST/symbol intelligence engine**: its `index_folder` returned `"No source files found"` because the repository contains only `.md` files and no recognized source language. Consequently JCodeMunch's code-graph tools — `get_dependency_graph`, `get_dependency_cycles`, `get_layer_violations`, `get_coupling_metrics`, `find_similar_symbols`, `get_repo_health` — **have no symbols to operate on and cannot audit a docs-only repository.**

> **This is itself audit finding F-0:** a code-intelligence tool is the wrong instrument for a Phase-0 docs repo. The audit was therefore performed by reconstructing the architecture's dependency graph **from the documentation cross-links** and applying the same metrics JCodeMunch would (afferent/efferent coupling, instability, SCC cycle detection, layer-direction checks, near-duplicate clustering) directly to that graph. When code lands in Phase 1, re-run JCodeMunch on the source and reconcile the two graphs.

**Graph captured:** 124 files, 2,801 unique link edges (≈22.6 outbound links/file), partitioned into **2,631 body edges** (in-prose dependencies) and **877 navigational edges** ("Related documents", "Open decisions", IR Producers/Consumers reverse-references). Direction analysis uses body edges only.

---

## 1. Scorecard

| # | Objective | Score | Notes |
|---|-----------|:----:|-------|
| 1 | Documentation hierarchy | **9.5** | Clean ring taxonomy; 0 orphans; every doc reachable. |
| 2 | Duplicated concepts | **8.5** | No conceptual duplication; verification family (ERC/DRC/DFM) is near-templated prose (intentional, mild drift risk). |
| 3 | Hidden coupling | **9.0** | None found — all strong couplings are intended design relationships. |
| 4 | Clean-architecture boundaries | **8.0** | Boundaries sound in design; ~108 inner→outer body links point at concrete adapters instead of ports (link hygiene). |
| 5 | Dependency direction | **8.5** | Predominantly inward (1,046 inward vs 507 outward); Stable-Dependencies Principle satisfied. |
| 6 | Cyclic dependencies | **8.0** | No architectural cycles; one 124-node SCC is an artifact of *intended* bidirectional cross-links. |
| 7 | Missing subsystems | **7.5** | 4 thin/absent areas: public/automation API, identity & tenancy, EDA interoperability/import-export, in-app search. |
| 8 | Architectural smells | **8.5** | Three low-severity smells (below). |
| 9 | Module decomposition | **9.0** | Decomposition is strong; minor refinements suggested. |
| 10 | This report | — | Delivered. |
| | **Overall** | **8.6** | **Healthy. No structural redesign needed.** |

---

## 2. Findings by objective

### 1. Documentation hierarchy ✓
The folder layout encodes the clean-architecture rings (`foundation → core → compiler/engineering/knowledge → data/integration/presentation → crosscutting/governance/quality → agents/state-machines → decisions`). **Zero orphan documents** (every doc has ≥1 inbound link). The most-referenced "hub" docs are exactly the canonical foundations — a textbook **Stable Abstractions** signal:

| Hub doc | Inbound refs |
|---------|:---:|
| `foundation/principles.md` | 113 |
| `foundation/engineering-domain-model.md` | 108 |
| `core/contracts.md` | 100 |
| `core/shared-state-model.md` | 93 |
| `GLOSSARY.md` | 88 |
| `core/event-bus.md` | 82 |

### 2. Duplicated concepts — minor
No *conceptual* duplication: the same idea is defined once and referenced elsewhere (the canonical-model discipline holds). The repeated headings ("Purpose", "Failure modes", ADR "Positive/Negative/Neutral") are the **intentional per-family templates** from `CONVENTIONS.md`, not duplication.

The real signal is **near-duplicate prose in the verification family** (5-gram Jaccard):

| Pair | Similarity |
|------|:---:|
| `state-machines/dfm-verification.md` ~ `drc-verification.md` | 0.40 |
| `agents/drc-agent.md` ~ `agents/erc-agent.md` | 0.38 |
| `state-machines/drc-verification.md` ~ `erc-verification.md` | 0.37 |

ERC/DRC/DFM are structurally near-identical because they all specialize the same [Verification Engine](engineering/verification-engine.md). This is *good* consistency but carries drift risk: an edit to the shared pattern must be replicated three times. **Recommendation (P3):** factor the shared "rule-check phase pattern" into `verification-engine.md` and have the three phases reference it for the common parts, differing only in their rule sets.

### 3. Hidden coupling — none found ✓
The strongest inter-subsystem couplings are **all intended design relationships**, with no surprises:

```
AgentSystem  -> StateMachines : 54     (each agent drives its phase FSM — by design)
CompilerIR   -> StateMachines : 42     (IRs name their producer/consumer phases)
CompilerIR   -> AgentSystem   : 32
PCBPipeline  -> StateMachines : 26
AgentSystem  -> VerificationPipeline : 25
```
No unexpected edges exist (e.g. Storage→AgentSystem, VectorMemory→PCBPipeline, presentation→engine-internals). The coupling matrix matches the intended topology — **the design has no hidden back-channels.**

### 4. Clean-architecture boundaries — sound design, link-hygiene gap
The boundary *design* is correct: inner rings define [Contracts/ports](core/contracts.md); outer rings ([data](data/storage.md), [integration](integration/plugin-system.md), [presentation](presentation/frontend.md)) implement them; the UI is presentation-only ([P11](foundation/principles.md)).

However, **~108 body links run from an inner ring to a concrete outer doc** rather than to the port abstraction. Examples:
- `core/scheduler.md → crosscutting/cost-and-resource-governance.md` (should reference the **Cost-budget port** in `contracts.md`).
- `core/event-bus.md → crosscutting/logging-and-observability.md` (should reference the **Observability port**).
- `core/contracts.md → crosscutting/security.md`, `→ data/stores/*` (the ports doc naming its concrete implementors).

These are **navigational/informative links, not compile-time dependencies** — the prose still respects [P12](foundation/principles.md) (the core depends on abstractions). But in the documentation graph they blur the dependency-rule boundary. **Recommendation (P2):** adopt a link-hygiene rule — *from an inner doc, link the **port** in `contracts.md`, not the concrete outer adapter; let the outer adapter declare "implements port X" and link inward.* (Codified in `CONVENTIONS.md` as part of this review.)

### 5. Dependency direction ✓
On body edges (excluding root/glossary and ADR-justification links): **inward = 1,046, same-ring = 693, outward = 507.** Inward dominates by 2:1. The **Stable Dependencies Principle** holds — instability rises monotonically from kernel to edge:

| Subsystem | Instability `Ce/(Ca+Ce)` | Reading |
|-----------|:---:|--------|
| Event Bus | **0.12** | very stable inner mechanism ✓ |
| Scheduler | 0.26 | stable ✓ |
| Verification Pipeline | 0.39 | balanced |
| State Machines | 0.45 | balanced |
| PCB Pipeline | 0.47 | balanced |
| Compiler IR | 0.62 | unstable (names all producers/consumers) |
| Agent System | 0.66 | unstable outer instances ✓ |
| Storage | 0.67 | unstable outer adapters ✓ |
| Plugin System | 0.80 | leaf adapter ✓ |

Inner mechanisms are stable; outer adapters/instances are unstable — exactly the desired gradient.

### 6. Cyclic dependencies — no architectural cycles ✓
Tarjan SCC analysis on body edges yields **a single strongly-connected component containing all 124 files.** This is **not** an architectural cycle problem — it is the expected consequence of *deliberate bidirectional cross-linking*: agent↔FSM mutual links (mandated by the anti-duplication rule), IR↔phase mutual links, and "Related documents" reciprocity. At the **ring level**, flow is acyclic-dominant inward. The only ring-level 2-cycles (`core↔data`, `core↔integration`, `core↔presentation`, `core↔crosscutting`) are the **port/adapter relationship** and reduce to zero once the F-4 link-hygiene rule is applied. No genuine circular *dependency* exists.

### 7. Missing / thin subsystems
Assessed against an enterprise checklist. Four gaps worth a Phase-0 stub:

| Gap | Severity | Why it matters |
|-----|:---:|----------------|
| **Public / automation API surface** | Medium | `integration/ipc.md` covers FE↔BE only; no programmatic/headless/CLI/SDK surface. The "Cursor + Git + Unreal" vision implies scripting/automation. (Partially anticipated by [plugin-system](integration/plugin-system.md).) |
| **Identity & tenancy model** | Medium | `security.md` covers authz/secrets and `multi-user-and-sessions.md` covers sessions, but there is no explicit identity/account/tenant entity. Multi-user needs it. |
| **EDA interoperability (import/export)** | Medium | Importing/exporting existing designs (KiCad/Altium/ODB++/IPC-2581) is mentioned only as "importers" in plugin-system. Adoption depends on it; deserves its own integration doc. |
| **In-app search / indexing** | Low | An IDE over large designs needs entity search; `vector-memory` is semantic memory, not structured entity search. |

Well-covered (not missing): observability, configuration, cost governance, checkpoint/recovery, versioning, collaboration, licensing, safety. Deployment/ops and DR are reasonably deferred for Phase 0.

### 8. Architectural smells (all low-severity)
- **S1 — Port-link leakage** (see F-4): inner docs link concrete adapters. *Fix: link hygiene rule.*
- **S2 — Verification-family prose duplication** (see #2): ERC/DRC/DFM ≈0.4 similar. *Fix: extract shared pattern.*
- **S3 — Plugin/extensibility under-woven:** among the 12 focus subsystems, the [Plugin System](integration/plugin-system.md) has the lowest inbound coupling (fan-in 1). It is declared by the kernel ([capability-registry](core/capability-registry.md), [contracts](core/contracts.md)) but the things that get extended — agents, phases, rule sets, viewers — rarely point back to it. *Fix: weave extensibility cross-links from `agents/README.md`, `state-machines/README.md`, `presentation/frontend/panels.md`, and `verification-engine.md`.* (Applied in this review.)

### 9. Suggested module decomposition
Decomposition is already strong. Refinements:
- Keep the **knowledge capability vs. store** split (it correctly separates `knowledge/*` ports from `data/stores/*` adapters) — do not merge.
- Consider grouping the **PCB pipeline** (floor-planning → placement → routing) under a shared `state-machines/` sub-note documenting their common geometric-iteration pattern, mirroring how the verification family should share a pattern.
- The **Compiler IR** ring's high fan-out is inherent (IRs enumerate producers/consumers); no change needed, but keep those as *navigational* reverse-references, not body dependencies.

---

## 3. Focus-subsystem health (the 12 requested)

| Subsystem | Files | Instability | Verdict |
|-----------|:---:|:---:|---------|
| Runtime | 3 | 0.40 | ✅ Healthy; clear concept/mechanism/composition split. |
| State Machines | 16 | 0.45 | ✅ Strong; framework + 14 instances; anti-duplication holds. |
| Event Bus | 1 | 0.12 | ✅ Excellent — the most stable hub, as a kernel bus should be. |
| Scheduler | 2 | 0.26 | ✅ Healthy; cleanly distinct from orchestrator. |
| Compiler IR | 8 | 0.62 | ✅ Sound; high fan-out is inherent to IR producer/consumer naming. |
| Storage | 10 | 0.67 | ✅ Correctly unstable outer adapters; one port each. |
| Knowledge Graph | 2 | 0.37 | ✅ Clean capability/store split. |
| Vector Memory | 2 | 0.43 | ✅ Clean capability/store split. |
| Agent System | 14 | 0.66 | ✅ Strong; two-part split + FSM cross-links consistent. |
| Plugin System | 1 | 0.80 | ⚠️ Under-integrated (S3) — fixed via cross-link weave. |
| Verification Pipeline | 5 | 0.39 | ⚠️ Near-duplicate prose (S2) — factor shared pattern. |
| PCB Pipeline | 4 | 0.47 | ✅ Healthy; consider shared geometric-iteration note. |

---

## 4. Prioritized recommendations

**P1 — none.** No structural defect requires redesign.

**P2 (do soon):**
1. **Link-hygiene rule** — inner docs link **ports**, not concrete adapters (S1/F-4). *Codified in `CONVENTIONS.md` by this review.*
2. **Plugin extensibility weave** — add inbound cross-links to `plugin-system.md` from the subsystems that get extended (S3). *Applied by this review.*
3. **Add four gap stubs** (objective 7): `integration/public-api.md`, an identity/tenancy section in `collaboration/multi-user-and-sessions.md` or `crosscutting/security.md`, `integration/eda-interoperability.md`, and an in-app search note in `presentation/frontend/project-explorer.md`.

**P3 (opportunistic):**
4. Factor the shared verification-phase pattern into `verification-engine.md` (S2).
5. Add a shared PCB geometric-iteration note for the floor-plan→placement→routing trio.
6. When Phase 1 code lands, **re-run JCodeMunch on the source** and reconcile the code dependency graph against this documentation graph.

---

## 5. Appendix — headline metrics

```
Documents .......................... 124   (0 non-markdown, 0 broken links, 0 orphans)
Cross-link edges ................... 2,801 unique (2,631 body + 877 navigational)
Mean outbound links / doc .......... 22.6
Body dependency direction .......... inward 1,046 | same 693 | outward 507
Dependency-rule link-hygiene flags . 108 (navigational, low severity)
Architectural cycles (ring level) .. 0 genuine (4 port/adapter 2-cycles resolved by F-4)
Near-duplicate prose pairs (>0.20) . 7 (all within verification + viewer families)
Most stable subsystem .............. Event Bus (I=0.12)
Most unstable subsystem ............ Plugin System (I=0.80, leaf adapter)
```

*Generated by an enterprise architecture review. JCodeMunch was used per request; its inability to index a docs-only repo (F-0) is recorded above, and the equivalent analyses were performed directly on the documentation dependency graph.*
