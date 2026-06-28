//! The Requirement Agent — the two-part split (P8) made concrete.
//!
//! The reasoning adapter (the `ctx.reason` call) asks for candidate requirements as
//! *judgement only*. The deterministic use-case (everything around it) validates each
//! candidate against domain invariants, attaches a justifying Decision + Evidence +
//! provenance links, and proposes the commit via the Capability port. Unvalidated text
//! never becomes a Requirement (the seam, P3). See `docs/agents/requirement-agent.md`.

use eak_domain::{
    Decision, DesignIntent, Evidence, EvidenceKind, ProvenanceLink, RelationType, Requirement,
    RequirementStatus,
};
use eak_engines::PlanningEngine;
use eak_ports::ReasoningRequest;
use eak_runtime::{Agent, AgentActivation, AgentContext, AgentOutcome, CapabilityRequest};
use std::collections::HashSet;

pub struct RequirementAgent;

impl RequirementAgent {
    pub fn new() -> Self {
        Self
    }
}
impl Default for RequirementAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for RequirementAgent {
    fn name(&self) -> &str {
        "RequirementAgent"
    }

    fn activate(
        &mut self,
        ctx: &mut dyn AgentContext,
        _activation: &AgentActivation,
    ) -> AgentOutcome {
        let intent = match ctx.design_intent() {
            Some(i) => i,
            None => return AgentOutcome::NeedsHuman("no design intent to structure".into()),
        };

        // Sequence the work with the Planning Engine (trivial linear plan in Phase 1).
        let _plan = PlanningEngine::new().elicitation_plan();

        // ---- reasoning half: ask for candidates (judgement only) ----
        let request = ReasoningRequest {
            model_id: String::new(), // filled in by the runtime with the real engine id
            system: "You are a requirements engineer. Decompose design intent into \
                     discrete, testable requirements."
                .into(),
            prompt: build_prompt(&intent),
            schema_name: "requirement_candidates_v1".into(),
            temperature: 0.0,
            seed: stable_seed(&intent.statement),
        };
        let (call_seq, response) = match ctx.reason(request) {
            Ok(pair) => pair,
            Err(e) => return AgentOutcome::Failed(format!("reasoning failed: {e}")),
        };
        if response.candidates.is_empty() {
            return AgentOutcome::NeedsHuman("no candidate requirements proposed".into());
        }

        // ---- deterministic half: validate, justify, commit ----
        let mut committed = 0usize;
        let mut seen: HashSet<String> = HashSet::new();
        for cand in &response.candidates {
            // dedup + cheap pre-checks (the validation seam continues at the capability).
            let key = cand.statement.trim().to_lowercase();
            if cand.statement.trim().is_empty() || cand.acceptance_criterion.trim().is_empty() {
                continue;
            }
            if !seen.insert(key) {
                continue;
            }

            let rid = ctx.fresh_id();
            let eid = ctx.fresh_id();
            let did = ctx.fresh_id();

            let requirement = Requirement {
                id: rid,
                statement: cand.statement.clone(),
                category: cand.category,
                priority: cand.priority,
                acceptance_criterion: cand.acceptance_criterion.clone(),
                status: RequirementStatus::Accepted, // autonomous path accepts directly
                source: intent.id,
                targets: cand.targets.clone(),
            };
            // domain validation before proposing — the seam (P3).
            if requirement.validate().is_err() {
                continue;
            }

            let evidence = Evidence {
                id: eid,
                kind: EvidenceKind::DesignIntentSource,
                content_reference: cand.source_hint.clone(),
                source: intent.source.clone(),
                reliability: 1.0,
            };
            let decision = Decision {
                id: did,
                subject: rid,
                rationale: cand.rationale.clone(),
                decider: "RequirementAgent".into(),
                reasoning_call_seq: Some(call_seq),
                evidence: vec![eid],
                confidence: cand.confidence,
            };
            let links = vec![
                ProvenanceLink {
                    id: ctx.fresh_id(),
                    from: rid,
                    to: did,
                    relation: RelationType::JustifiedBy,
                },
                ProvenanceLink {
                    id: ctx.fresh_id(),
                    from: rid,
                    to: intent.id,
                    relation: RelationType::DerivedFrom,
                },
                ProvenanceLink {
                    id: ctx.fresh_id(),
                    from: did,
                    to: eid,
                    relation: RelationType::Supports,
                },
            ];

            if ctx
                .invoke(CapabilityRequest::CreateRequirement {
                    requirement,
                    decision,
                    evidence: vec![evidence],
                    links,
                })
                .is_ok()
            {
                committed += 1;
            }
        }

        if committed == 0 {
            AgentOutcome::NeedsHuman("no candidate survived validation".into())
        } else {
            AgentOutcome::Success { committed }
        }
    }
}

fn build_prompt(intent: &DesignIntent) -> String {
    format!(
        "Decompose the following design intent into discrete, testable engineering \
         requirements. For each: a statement, a category, a priority, an objective \
         acceptance criterion, a short rationale, and the source it derives from.\n\n\
         DESIGN INTENT:\n{}\n",
        intent.statement
    )
}

/// Deterministic seed from the intent text (FNV-1a 64) so a run is reproducible.
fn stable_seed(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
