# ADR-0007: First-class physical-quantity type system

> **Grounds:** [P9 — Physical Correctness Is Typed](../foundation/principles.md). **Primary documents:** [`engineering/units-and-quantities.md`](../engineering/units-and-quantities.md), [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md).

## Status

Accepted.

## Context

Dimensional and unit errors are a notorious, expensive, and *entirely preventable* class of engineering defect: a `3.3` that is volts in one place and millivolts in another, a clearance compared in mils against a bound stated in millimetres, a capacitance used at its nominal value when DC-bias derating has halved it. The historical record (the canonical metric/imperial spaceflight losses, routine EDA mils-vs-millimetres mistakes) shows these errors are catastrophic and recurrent.

This system multiplies the risk in a specific way: **an AI proposes values.** A language model can emit a perfectly plausible number in the wrong unit or the wrong dimension, and a bare-number representation would accept it silently and carry it all the way to a fabrication output. The runtime also promises [determinism](0009-determinism-and-replay-strategy.md), which requires that equality and ordering of physical values be unambiguous and reproducible — impossible if `3.3 V` and `3300 mV` are different opaque floats.

We must decide how physical values are represented across the entire domain, once, because every entity, IR, engine, and boundary touches them.

## Decision

**No physical value in the system is ever a bare number. Every physical value is a first-class [Physical Quantity](../engineering/units-and-quantities.md): magnitude + unit (with its dimension) + tolerance, subject to dimensional analysis.**

1. **Quantity, not number.** `3.3 V ±5 %` is *one* typed value, not three loose fields. Voltages, currents, lengths, temperatures, capacitances, inductances, impedances, frequencies, power, and material properties are all Physical Quantities.
2. **Dimensional analysis at the type boundary.** Add/subtract only within a dimension; multiply/divide derive new dimensions (V ÷ A = Ω); compare only commensurable quantities. A cross-dimension operation is a **type error surfaced to the engineer, never a silent coercion** — making unit-mismatch bugs *unrepresentable* rather than merely discouraged.
3. **Canonical normalization.** A canonical internal form per dimension makes equality and ordering unambiguous and [reproducible](../core/determinism-and-reproducibility.md) ([P4](../foundation/principles.md)), so `3300 mV` equals `3.3 V`.
4. **Tolerance and derating are explicit and traceable.** Tolerance is part of the quantity, so comparisons near a bound respect worst case, not just nominal; derating is a first-class, [provenance](../core/provenance-and-traceability.md)-linked transformation of a quantity.
5. **The reasoning boundary enforces it.** A model proposal of a physical value is schema-validated into a quantity *with a unit* before it can touch state ([P3](../foundation/principles.md)); a unitless or wrong-dimension proposal is rejected at the [reasoning boundary](../core/reasoning-engine-interface.md).

This decides a *type-system approach*; it names no numeric library or unit framework (Phase 0).

## Consequences

### Positive
- **An entire defect class is eliminated, not just discouraged.** Unit mismatches and cross-dimension comparisons become impossible-to-represent states ([P9](../foundation/principles.md)).
- **AI proposals are made safe at the point of entry.** A plausible-but-wrong-unit model output is rejected at the boundary instead of propagating to a fab ([ADR-0002](0002-runtime-owns-knowledge-llm-as-reasoning-engine.md)).
- **Conversion is centralized and defined once,** not re-implemented (and mis-implemented) per phase; canonical normalization makes equality/ordering deterministic ([ADR-0009](0009-determinism-and-replay-strategy.md)).
- **Honest engineering margins.** Tolerance- and derating-aware comparisons surface "passes nominally, fails at worst case" defects that bare numbers hide ([P5](../foundation/principles.md)).

### Negative
- **Discipline at every numeric boundary.** Every physical value must be constructed with a unit and tolerance; nothing can be a quick float — modest but pervasive friction.
- **Modelling effort.** A correct quantity model (dimensions, conversions, tolerance propagation, derating, canonical form) is real foundational work that must exist before the engines that depend on it.
- **Interop overhead.** Values arriving from external tools/datasheets/simulators must be typed on the way in and de-typed on the way out, with conversion behaviour made explicit.

### Neutral
- The default tolerance-propagation method (worst-case vs. statistical) and whether it is configurable per check are left open to a future ADR; this ADR fixes that tolerance *is* carried, not how it propagates.
- Physical Quantity becomes part of the shared [domain vocabulary](../core/contracts.md) every contract speaks; it defines no new outer-ring port of its own.

## Alternatives considered

- **Bare numbers with a unit *convention* (e.g. "everything in SI base units").** Zero type machinery. *Rejected:* a convention is unenforceable — the first value that ignores it (especially an AI-proposed one) corrupts silently; this is the status quo that causes the disasters.
- **Unit annotations as metadata/comments, not part of the value's type.** Some documentation benefit. *Rejected:* metadata that the type system doesn't check is not validated and drifts from the value; it cannot make a mismatch a type error.
- **Validate units only at the reasoning boundary, store bare numbers internally.** Catches AI mistakes cheaply. *Rejected:* leaves every internal computation and cross-phase comparison unprotected, and loses canonical equality/ordering needed for determinism.
- **A full external scientific-computing/units library as the model.** Powerful. *Rejected for Phase 0:* that is a *technology* selection (deferred); here we decide the *architectural requirement* that quantities are first-class and typed, independent of any library that might later realize it.

## Related documents

[`engineering/units-and-quantities.md`](../engineering/units-and-quantities.md) · [`foundation/engineering-domain-model.md`](../foundation/engineering-domain-model.md) · [`engineering/constraint-engine.md`](../engineering/constraint-engine.md) · [`engineering/verification-engine.md`](../engineering/verification-engine.md) · [`core/reasoning-engine-interface.md`](../core/reasoning-engine-interface.md) · [`foundation/principles.md`](../foundation/principles.md) (P9) · [ADR-0005](0005-ir-as-canonical-phase-boundary-representation.md) · [ADR-0009](0009-determinism-and-replay-strategy.md)
