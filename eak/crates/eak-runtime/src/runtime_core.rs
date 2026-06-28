//! The Engineering Runtime kernel — the sole mutator of Engineering State (P2).
//!
//! Every event funnels through the single [`RuntimeCore::commit`] path: stamp (clock) ->
//! append (event log) -> fold (state). The runtime implements [`AgentContext`], so agents
//! reach reasoning and mutation only through it. Capability handlers re-validate proposals
//! at the seam (P3) before committing.

use crate::clock::{Clock, IdSource};
use crate::protocol::{AgentContext, Autonomy, CapabilityAck, CapabilityError, CapabilityRequest};
use crate::state::EngineeringState;
use eak_domain::{
    Constraint, Decision, DesignIntent, EntityId, Evidence, ProvenanceLink, Requirement, Violation,
    Waiver,
};
use eak_ports::{
    Event, EventLog, ReasoningEngine, ReasoningError, ReasoningRequest, ReasoningResponse, Seq,
    StoreError, Timestamp,
};

pub struct RuntimeCore {
    pub state: EngineeringState,
    log: Box<dyn EventLog>,
    reasoning: Box<dyn ReasoningEngine>,
    ids: Box<dyn IdSource>,
    clock: Box<dyn Clock>,
    autonomy: Autonomy,
}

impl RuntimeCore {
    pub fn new(
        log: Box<dyn EventLog>,
        reasoning: Box<dyn ReasoningEngine>,
        ids: Box<dyn IdSource>,
        clock: Box<dyn Clock>,
        autonomy: Autonomy,
    ) -> Self {
        Self {
            state: EngineeringState::new(),
            log,
            reasoning,
            ids,
            clock,
            autonomy,
        }
    }

    /// Read-only access to the log (for replay / inspection).
    pub fn log(&self) -> &dyn EventLog {
        self.log.as_ref()
    }

    /// The single commit path (P2): stamp -> append -> fold. All event production
    /// converges here so every change is recorded and reproducible.
    fn commit(&mut self, events: Vec<Event>) -> Result<Vec<Seq>, StoreError> {
        let stamped: Vec<(Timestamp, Event)> = events
            .iter()
            .map(|e| (self.clock.now(), e.clone()))
            .collect();
        let seqs = self.log.append(&stamped)?;
        for e in &events {
            self.state.apply(e);
        }
        Ok(seqs)
    }

    /// Seed the phase with the engineer's intent (trusted input, not model output).
    pub fn capture_intent(
        &mut self,
        statement: &str,
        source: &str,
    ) -> Result<EntityId, StoreError> {
        let id = self.ids.fresh();
        let intent = DesignIntent {
            id,
            statement: statement.to_string(),
            structured_summary: statement.to_string(),
            source: source.to_string(),
        };
        self.commit(vec![Event::IntentCaptured { intent }])?;
        Ok(id)
    }

    fn handle_create_requirement(
        &mut self,
        requirement: Requirement,
        decision: Decision,
        evidence: Vec<Evidence>,
        links: Vec<ProvenanceLink>,
    ) -> Result<CapabilityAck, CapabilityError> {
        // The seam (P3): the runtime, not the model, commits. Re-validate domain invariants.
        requirement
            .validate()
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        if self.autonomy == Autonomy::Supervised {
            return Err(CapabilityError::Rejected(
                "supervised autonomy requires human approval (HITL deferred to a later phase)"
                    .into(),
            ));
        }

        let mut events = Vec::new();
        for ev in evidence {
            events.push(Event::EvidenceReferenced { evidence: ev });
        }
        events.push(Event::DecisionCreated { decision });
        events.push(Event::RequirementCommitted { requirement });
        for link in links {
            events.push(Event::ProvenanceLinked { link });
        }
        let seqs = self
            .commit(events)
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        Ok(CapabilityAck { committed: seqs })
    }

    fn handle_create_constraint(
        &mut self,
        constraint: Constraint,
        links: Vec<ProvenanceLink>,
    ) -> Result<CapabilityAck, CapabilityError> {
        // The seam (P3): re-validate the constraint and its subject before committing.
        constraint
            .validate()
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        if constraint.subject_requirement.is_null() {
            return Err(CapabilityError::Rejected(
                "constraint has no subject requirement".into(),
            ));
        }

        let mut events = vec![Event::ConstraintCommitted { constraint }];
        for link in links {
            events.push(Event::ProvenanceLinked { link });
        }
        let seqs = self
            .commit(events)
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        Ok(CapabilityAck { committed: seqs })
    }

    fn handle_raise_violation(
        &mut self,
        violation: Violation,
        links: Vec<ProvenanceLink>,
    ) -> Result<CapabilityAck, CapabilityError> {
        // A violation that names no subjects would be untraceable — reject it (P13).
        if violation.subjects.is_empty() {
            return Err(CapabilityError::Rejected(
                "violation names no subjects (would be untraceable)".into(),
            ));
        }

        let mut events = vec![Event::ViolationRaised { violation }];
        for link in links {
            events.push(Event::ProvenanceLinked { link });
        }
        let seqs = self
            .commit(events)
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        Ok(CapabilityAck { committed: seqs })
    }

    fn handle_grant_waiver(&mut self, waiver: Waiver) -> Result<CapabilityAck, CapabilityError> {
        // Accepting a violation is a design-significant judgement (P10): in supervised mode
        // it needs human approval, which is deferred to a later phase.
        if self.autonomy == Autonomy::Supervised {
            return Err(CapabilityError::Rejected(
                "supervised autonomy requires human approval (HITL deferred to a later phase)"
                    .into(),
            ));
        }
        // The target must exist — a waiver for an unknown violation is meaningless.
        if self.state.violation(waiver.violation).is_none() {
            return Err(CapabilityError::Rejected(format!(
                "waiver targets unknown violation {}",
                waiver.violation.short()
            )));
        }

        let seqs = self
            .commit(vec![Event::WaiverGranted { waiver }])
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        Ok(CapabilityAck { committed: seqs })
    }
}

impl AgentContext for RuntimeCore {
    fn autonomy(&self) -> Autonomy {
        self.autonomy
    }

    fn fresh_id(&mut self) -> EntityId {
        self.ids.fresh()
    }

    fn design_intent(&self) -> Option<DesignIntent> {
        self.state.intent.clone()
    }

    fn requirements(&self) -> Vec<Requirement> {
        self.state.requirements.clone()
    }

    fn provenance_links(&self) -> Vec<ProvenanceLink> {
        self.state.links.clone()
    }

    fn constraints(&self) -> Vec<Constraint> {
        self.state.constraints.clone()
    }

    fn violations(&self) -> Vec<Violation> {
        self.state.violations.clone()
    }

    fn reason(
        &mut self,
        mut req: ReasoningRequest,
    ) -> Result<(Seq, ReasoningResponse), ReasoningError> {
        req.model_id = self.reasoning.model_id();
        let response = self.reasoning.request_judgement(&req)?;
        let event = Event::ReasoningCall {
            request: req,
            response: response.clone(),
        };
        let seqs = self
            .commit(vec![event])
            .map_err(|e| ReasoningError::Provider(e.to_string()))?;
        let seq = *seqs.first().expect("reasoning call produced one event");
        Ok((seq, response))
    }

    fn invoke(&mut self, req: CapabilityRequest) -> Result<CapabilityAck, CapabilityError> {
        match req {
            CapabilityRequest::CreateRequirement {
                requirement,
                decision,
                evidence,
                links,
            } => self.handle_create_requirement(requirement, decision, evidence, links),
            CapabilityRequest::CreateConstraint { constraint, links } => {
                self.handle_create_constraint(constraint, links)
            }
            CapabilityRequest::RaiseViolation { violation, links } => {
                self.handle_raise_violation(violation, links)
            }
            CapabilityRequest::GrantWaiver { waiver } => self.handle_grant_waiver(waiver),
        }
    }

    fn emit(&mut self, events: Vec<Event>) -> Result<Vec<Seq>, StoreError> {
        self.commit(events)
    }
}
