# Electronics Agent Kit — Phase 1 (kernel + Requirement Planning)

The smallest thing that is unmistakably *this architecture working*: the deterministic
runtime kernel, the shared engineering state, and one engineering phase
(**Requirement Planning**) running end-to-end with full provenance and deterministic
replay. Design docs live in [`../docs`](../docs); the phasing is in
[`../docs/foundation/roadmap.md`](../docs/foundation/roadmap.md).

## Workspace layout (one crate per architecture ring; deps point only inward)

| Crate | Ring | Responsibility |
|-------|------|----------------|
| `eak-units` | Entities | Physical-quantity type system (P9) |
| `eak-domain` | Entities | Domain entities, opaque `EntityId`, invariants |
| `eak-ports` | Use-case | Port traits (`EventLog`, `ReasoningEngine`) + `Event` |
| `eak-runtime` | Use-case | Kernel: state/fold, FSM framework, execution engine, orchestrator, capability handler, replay |
| `eak-engines` | Domain | Planning Engine (trivial sequencer) |
| `eak-compiler` | Domain | Requirement IR + stub Engineering IR lowering |
| `eak-phases` | Instances | Requirement Agent (two-part split) + FSMs |
| `eak-store` | Adapters | Append-only JSON-lines event log |
| `eak-reasoning` | Adapters | Fixture + live Anthropic reasoning adapters |
| `eak-cli` | Drivers | `eak` binary + composition root |

`eak-runtime` depends only on `eak-ports`/`eak-domain`/`eak-units` — enforced at compile
time by a guard test (the [Dependency Rule, P1](../docs/foundation/principles.md)).

## Build & verify

```sh
cargo build
cargo clippy --all-targets -- -D warnings
cargo test                       # unit + the exit-criterion integration tests
```

## Run

```sh
# Offline, deterministic (built-in fixture): run both phases, write the event log.
cargo run --bin eak -- run \
  --intent "USB-C powered IoT sensor node, < 5 W, < 50x50 mm" \
  --log /tmp/eak.jsonl --deterministic

# Replay the recorded history into reconstructed state (no model, no clock).
cargo run --bin eak -- replay --log /tmp/eak.jsonl

# Show the provenance chain for a requirement (short id from the run output).
cargo run --bin eak -- trace --log /tmp/eak.jsonl <requirement-id>
```

### Live reasoning (real model, recorded then replayable)

```sh
export ANTHROPIC_API_KEY=...
cargo run --features live --bin eak -- run \
  --intent "..." --reasoning live --model claude-opus-4-8 --log /tmp/eak-live.jsonl
cargo test --features live -- --ignored   # the live smoke test
```

## What Phase 1 proves
P1 (rings/contracts, compiler-enforced) · P2 (runtime owns knowledge) · P3 (reasoning
behind one port) · P4 (recorded history replays to identical state) · P8 (two-part agent).
See [`../docs/decisions/0011`–`0015`](../docs/decisions/).
