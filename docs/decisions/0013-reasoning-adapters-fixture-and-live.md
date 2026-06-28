# ADR-0013 — Reasoning adapters: one port, fixture + live Anthropic

**Status:** Accepted (Phase 1)

## Context
The [Reasoning Engine port (P3)](../core/reasoning-engine-interface.md) is the single
boundary to stochastic judgement. Phase 1 must prove deterministic replay despite a
stochastic model, and should also exercise a real provider.

## Decision
Define one `ReasoningEngine` trait in `eak-ports` and ship **two** adapters in
`eak-reasoning`: a **fixture/cassette** adapter (deterministic, offline, no API key) and a
**live Anthropic** adapter (feature `live`). The runtime records every call as a
`ReasoningCall` event; replay serves recorded outputs and never calls the model.

## Consequences
- The full pipeline and deterministic replay run with zero external dependency (fixtures);
  the live adapter slots in behind the same port and is the only crate that knows the
  provider, keeping the domain provider-independent (P3).
- The default build/test path needs no network or key; the live path is opt-in.

## Alternatives considered
- **Fixture only** — would not exercise a real model boundary.
- **Live only** — needs a key and is non-reproducible until recorded; poor for CI.
