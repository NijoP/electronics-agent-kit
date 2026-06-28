//! Deterministic replay (`docs/core/determinism-and-reproducibility.md`, P4).
//!
//! State is the fold of the event log. Replay re-folds the recorded history WITHOUT
//! calling the model or reading the clock — recorded reasoning outputs and ids are reused
//! verbatim — so it reconstructs byte-identical [`EngineeringState`].

use crate::state::EngineeringState;
use eak_ports::{EventLog, StoreError};

pub fn replay(log: &dyn EventLog) -> Result<EngineeringState, StoreError> {
    let mut state = EngineeringState::new();
    for record in log.read_all()? {
        state.apply(&record.event);
    }
    Ok(state)
}
