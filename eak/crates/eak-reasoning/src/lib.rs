//! Reasoning adapters (outer ring) — concrete implementations of the single reasoning
//! boundary (P3). `eak-reasoning` is the only place a model/provider is known.

mod fixture;
pub use fixture::{Cassette, CassetteEntry, FixtureEngine};

#[cfg(feature = "live")]
mod anthropic;
#[cfg(feature = "live")]
pub use anthropic::AnthropicEngine;
