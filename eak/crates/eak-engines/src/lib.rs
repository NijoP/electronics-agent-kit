//! Domain engines (deterministic services). Phase 1 ships only the Planning Engine as a
//! trivial linear step sequencer; the Constraint / Verification / Learning engines are
//! later phases. See `docs/engineering/planning-engine.md`.

/// One step in an agent's elicitation reasoning-plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    pub name: String,
}

/// Sequences an agent's elicitation steps (a *reasoning plan*, distinct from the workflow
/// plan). Phase 1 uses a fixed linear plan; backtracking/branching is deferred.
#[derive(Debug, Clone, Default)]
pub struct PlanningEngine;

impl PlanningEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn elicitation_plan(&self) -> Vec<PlanStep> {
        ["read_intent", "propose_requirements", "validate_and_commit"]
            .into_iter()
            .map(|s| PlanStep {
                name: s.to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_is_linear_and_nonempty() {
        let plan = PlanningEngine::new().elicitation_plan();
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0].name, "read_intent");
    }
}
