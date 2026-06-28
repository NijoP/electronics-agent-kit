//! Non-deterministic sources isolated behind ports so they can be made deterministic.
//!
//! Time and identity are the two non-deterministic inputs to a run. Both are captured at
//! a boundary: timestamps are recorded in events (never re-read on replay), and entity
//! ids are minted at record time and carried in events (never regenerated on replay).

use eak_domain::EntityId;
use eak_ports::Timestamp;
use std::cell::Cell;

/// Wall-clock source. The runtime stamps each event with `now()` before appending.
pub trait Clock {
    fn now(&self) -> Timestamp;
}

/// Opaque-id source. The only minter of [`EntityId`]s during a run.
pub trait IdSource {
    fn fresh(&mut self) -> EntityId;
}

/// Real wall-clock (used by `eak run`).
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Timestamp(ms)
    }
}

/// Monotonic logical clock (counter) — makes a run fully reproducible for tests.
pub struct LogicalClock {
    next: Cell<i64>,
}
impl LogicalClock {
    pub fn new() -> Self {
        Self { next: Cell::new(0) }
    }
}
impl Default for LogicalClock {
    fn default() -> Self {
        Self::new()
    }
}
impl Clock for LogicalClock {
    fn now(&self) -> Timestamp {
        let v = self.next.get();
        self.next.set(v + 1);
        Timestamp(v)
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Deterministic opaque-id source seeded per run. Spreads bits (splitmix64) so ids carry
/// no meaning; never mints the reserved `EntityId::NULL`.
pub struct SeededIdSource {
    state: u64,
}
impl SeededIdSource {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}
impl IdSource for SeededIdSource {
    fn fresh(&mut self) -> EntityId {
        let hi = splitmix64(&mut self.state) as u128;
        let lo = splitmix64(&mut self.state) as u128;
        let id = (hi << 64) | lo;
        EntityId(if id == 0 { 1 } else { id })
    }
}
