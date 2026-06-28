//! Constraint Extraction state machine (instance) — DETERMINISTIC in Phase 2.
//!
//! It derives a machine-checkable [`Constraint`] from each requirement that carries a typed
//! physical target (P9): one constraint per requirement, bounded by `targets[0]`, with its
//! comparison sense inferred from the requirement's wording. It makes NO reasoning calls —
//! the stochastic boundary (P3) is untouched, so a reasoning-assisted extraction is a clean
//! future enhancement. Extraction is idempotent: a constraint already derived for a
//! requirement is skipped, so re-running on a correctness-loop back commits nothing new and
//! keeps the run replay-identical. See `docs/state-machines/constraint-extraction.md`.

use eak_domain::{Constraint, ConstraintKind, ConstraintStatus, ProvenanceLink, RelationType};
use eak_ports::Event;
use eak_runtime::{AgentContext, CapabilityRequest, Machine, MachineError, StepResult};

pub struct ConstraintExtractionMachine;

impl ConstraintExtractionMachine {
    pub fn new() -> Self {
        Self
    }
}
impl Default for ConstraintExtractionMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine for ConstraintExtractionMachine {
    fn name(&self) -> &str {
        "ConstraintExtraction"
    }

    fn initial(&self) -> String {
        "Idle".into()
    }

    fn step(
        &mut self,
        state: &str,
        ctx: &mut dyn AgentContext,
    ) -> Result<StepResult, MachineError> {
        match state {
            "Idle" => Ok(StepResult::Continue("Extracting".into())),

            "Extracting" => {
                let requirements = ctx.requirements();
                let existing = ctx.constraints();
                let mut committed = 0usize;

                for req in &requirements {
                    // Only requirements with a typed target yield a constraint (P9).
                    let Some(bound) = req.targets.first().copied() else {
                        continue;
                    };
                    // Idempotent: skip requirements that already have a derived constraint
                    // (so a loop-back re-extraction is a no-op).
                    if existing.iter().any(|c| c.subject_requirement == req.id) {
                        continue;
                    }

                    let kind = infer_kind(&req.statement);
                    let op = match kind {
                        ConstraintKind::Max => "<=",
                        ConstraintKind::Min => ">=",
                        ConstraintKind::Equal => "==",
                    };
                    let constraint = Constraint {
                        id: ctx.fresh_id(),
                        statement: format!("{} (value {} {})", req.statement, op, bound),
                        subject_requirement: req.id,
                        kind,
                        bound,
                        source: req.id,
                        status: ConstraintStatus::Active,
                    };
                    let cid = constraint.id;
                    // Provenance: the constraint derives from its requirement (the next hop
                    // toward full traceability of any violation it later participates in).
                    let link = ProvenanceLink {
                        id: ctx.fresh_id(),
                        from: cid,
                        to: req.id,
                        relation: RelationType::DerivedFrom,
                    };
                    if ctx
                        .invoke(CapabilityRequest::CreateConstraint {
                            constraint,
                            links: vec![link],
                        })
                        .is_ok()
                    {
                        committed += 1;
                    }
                }

                ctx.emit(vec![Event::ConstraintsExtracted { count: committed }])
                    .map_err(|e| MachineError::Internal(e.to_string()))?;
                Ok(StepResult::Done)
            }

            other => Err(MachineError::Internal(format!("unknown state {other}"))),
        }
    }
}

/// Infer a constraint's comparison sense from the requirement wording. Lower-bound cues
/// ("at least", "minimum", ...) mean [`ConstraintKind::Min`]; everything else is treated as
/// an upper bound ([`ConstraintKind::Max`]) — the common case for ceilings and limits.
fn infer_kind(statement: &str) -> ConstraintKind {
    let s = statement.to_lowercase();
    const MIN_CUES: [&str; 6] = [
        "at least",
        "minimum",
        "no less than",
        "not less than",
        ">=",
        "≥",
    ];
    if MIN_CUES.iter().any(|cue| s.contains(cue)) {
        ConstraintKind::Min
    } else {
        ConstraintKind::Max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_kind_reads_wording() {
        assert_eq!(
            infer_kind("Operating power shall not exceed 5 W"),
            ConstraintKind::Max
        );
        assert_eq!(
            infer_kind("Supply current shall be at least 8 W"),
            ConstraintKind::Min
        );
        assert_eq!(infer_kind("Voltage >= 3.3 V"), ConstraintKind::Min);
        assert_eq!(
            infer_kind("Board outline shall fit within 50 mm"),
            ConstraintKind::Max
        );
    }
}
