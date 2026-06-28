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
    Board, BomLineItem, Component, Constraint, Decision, DesignIntent, EntityId, Evidence,
    FunctionalBlock, Net, Part, Pin, Placement, ProvenanceLink, Requirement, Track, Violation,
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
        // Referential integrity at the seam: the subject must be a committed requirement,
        // so a constraint can never dangle (P3, P5).
        if self
            .state
            .requirement(constraint.subject_requirement)
            .is_none()
        {
            return Err(CapabilityError::Rejected(format!(
                "constraint subject requirement {} does not exist",
                constraint.subject_requirement.short()
            )));
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

    fn handle_create_functional_block(
        &mut self,
        block: FunctionalBlock,
        links: Vec<ProvenanceLink>,
    ) -> Result<CapabilityAck, CapabilityError> {
        // The seam (P3): re-validate the block and its requirement links before committing.
        block
            .validate()
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        // A block that realizes no requirement is untraceable to intent (P3) — reject it.
        if block.requirements.is_empty() {
            return Err(CapabilityError::Rejected(
                "functional block realizes no requirement (would be untraceable)".into(),
            ));
        }
        // Referential integrity at the seam: every referenced requirement must exist, so
        // the block-to-requirement trace can never dangle (P3, P5).
        for rid in &block.requirements {
            if self.state.requirement(*rid).is_none() {
                return Err(CapabilityError::Rejected(format!(
                    "functional block references unknown requirement {}",
                    rid.short()
                )));
            }
        }

        let mut events = vec![Event::FunctionalBlockCommitted { block }];
        for link in links {
            events.push(Event::ProvenanceLinked { link });
        }
        let seqs = self
            .commit(events)
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        Ok(CapabilityAck { committed: seqs })
    }

    fn handle_realize_component(
        &mut self,
        component: Component,
        pins: Vec<Pin>,
        links: Vec<ProvenanceLink>,
    ) -> Result<CapabilityAck, CapabilityError> {
        // The seam (P3): re-validate the component and its originating block before committing.
        component
            .validate()
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        // A component minted from no block is untraceable to intent (P3) — reject it.
        if component.from_block == EntityId::NULL {
            return Err(CapabilityError::Rejected(
                "component has no originating functional block".into(),
            ));
        }
        // Referential integrity at the seam: the originating block must exist (P3, P5).
        if self.state.functional_block(component.from_block).is_none() {
            return Err(CapabilityError::Rejected(format!(
                "component originates from unknown functional block {}",
                component.from_block.short()
            )));
        }
        // A component with no pins can never join a net and would pass every ERC rule
        // vacuously — reject it (P13: no silently-inert entities).
        if pins.is_empty() {
            return Err(CapabilityError::Rejected("component has no pins".into()));
        }

        // One atomic realization: the component, then a pin event each, then the links.
        let mut events = vec![Event::ComponentCommitted { component }];
        for pin in pins {
            events.push(Event::PinCommitted { pin });
        }
        for link in links {
            events.push(Event::ProvenanceLinked { link });
        }
        let seqs = self
            .commit(events)
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        Ok(CapabilityAck { committed: seqs })
    }

    fn handle_create_net(
        &mut self,
        net: Net,
        links: Vec<ProvenanceLink>,
    ) -> Result<CapabilityAck, CapabilityError> {
        // The seam (P3): re-validate the net and its membership before committing.
        net.validate()
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        // A net joining no pins carries no connectivity — reject it (P13).
        if net.members.is_empty() {
            return Err(CapabilityError::Rejected(
                "net joins no pins (carries no connectivity)".into(),
            ));
        }
        // Referential integrity at the seam: every member must be a committed pin, so
        // connectivity can never reference a phantom terminal (P3, P5).
        for pid in &net.members {
            if self.state.pin(*pid).is_none() {
                return Err(CapabilityError::Rejected(format!(
                    "net references unknown pin {}",
                    pid.short()
                )));
            }
        }

        let mut events = vec![Event::NetCommitted { net }];
        for link in links {
            events.push(Event::ProvenanceLinked { link });
        }
        let seqs = self
            .commit(events)
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        Ok(CapabilityAck { committed: seqs })
    }

    fn handle_create_part(
        &mut self,
        part: Part,
        links: Vec<ProvenanceLink>,
    ) -> Result<CapabilityAck, CapabilityError> {
        // The seam (P3): re-validate the part (non-empty manufacturer part number) before
        // committing — an unorderable part must never enter the BOM.
        part.validate()
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;

        let mut events = vec![Event::PartCommitted { part }];
        for link in links {
            events.push(Event::ProvenanceLinked { link });
        }
        let seqs = self
            .commit(events)
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        Ok(CapabilityAck { committed: seqs })
    }

    fn handle_create_bom_line_item(
        &mut self,
        item: BomLineItem,
        links: Vec<ProvenanceLink>,
    ) -> Result<CapabilityAck, CapabilityError> {
        // The seam (P3): re-validate intrinsic invariants — non-empty coverage, quantity
        // equal to the component count, and no component listed twice.
        item.validate()
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        if item.part.is_null() {
            return Err(CapabilityError::Rejected(
                "BOM line item has no part".into(),
            ));
        }
        // Referential integrity at the seam: the ordered part must be committed, so a line
        // can never bind a phantom part (P3, P5).
        if self.state.part(item.part).is_none() {
            return Err(CapabilityError::Rejected(format!(
                "BOM line item references unknown part {}",
                item.part.short()
            )));
        }
        // Referential integrity + single-sourcing: every covered component must exist and
        // must not already be claimed by another line — a component is ordered exactly once,
        // so the BOM can never silently double-source or contradict itself (P5).
        for cid in &item.components {
            if self.state.component(*cid).is_none() {
                return Err(CapabilityError::Rejected(format!(
                    "BOM line item references unknown component {}",
                    cid.short()
                )));
            }
            if self
                .state
                .bom_line_items
                .iter()
                .any(|l| l.components.contains(cid))
            {
                return Err(CapabilityError::Rejected(format!(
                    "BOM line item double-covers component {} (already on another line)",
                    cid.short()
                )));
            }
        }

        let mut events = vec![Event::BomLineItemCommitted { item }];
        for link in links {
            events.push(Event::ProvenanceLinked { link });
        }
        let seqs = self
            .commit(events)
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        Ok(CapabilityAck { committed: seqs })
    }

    fn handle_create_board(
        &mut self,
        board: Board,
        links: Vec<ProvenanceLink>,
    ) -> Result<CapabilityAck, CapabilityError> {
        // The seam (P3): re-validate the outline (positive dimensions + at least one layer)
        // before committing.
        board
            .validate()
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        // A design has exactly one outline; a second board would make placement DRC ambiguous
        // (which board is being fit against?) — reject it (P5).
        if self.state.board.is_some() {
            return Err(CapabilityError::Rejected(
                "design already has a board outline".into(),
            ));
        }

        let mut events = vec![Event::BoardCommitted { board }];
        for link in links {
            events.push(Event::ProvenanceLinked { link });
        }
        let seqs = self
            .commit(events)
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        Ok(CapabilityAck { committed: seqs })
    }

    fn handle_place_component(
        &mut self,
        placement: Placement,
        links: Vec<ProvenanceLink>,
    ) -> Result<CapabilityAck, CapabilityError> {
        // The seam (P3): re-validate the courtyard (positive extent) before committing.
        placement
            .validate()
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        // A placement with no component is untraceable to the schematic (P3) — reject it.
        if placement.component.is_null() {
            return Err(CapabilityError::Rejected(
                "placement has no component".into(),
            ));
        }
        // Referential integrity at the seam: the placed component must be realized (P3, P5).
        if self.state.component(placement.component).is_none() {
            return Err(CapabilityError::Rejected(format!(
                "placement references unknown component {}",
                placement.component.short()
            )));
        }
        // A placement is meaningless without an outline to fit against — require the board
        // first, so layout can never precede the floor plan (P5).
        if self.state.board.is_none() {
            return Err(CapabilityError::Rejected(
                "cannot place a component before the board outline exists".into(),
            ));
        }
        // Single-placement: a component sits at exactly one spot on one side, so a second
        // placement of the same component would contradict itself — reject it (P5).
        if self
            .state
            .placements
            .iter()
            .any(|p| p.component == placement.component)
        {
            return Err(CapabilityError::Rejected(
                "component is already placed".into(),
            ));
        }

        let mut events = vec![Event::PlacementCommitted { placement }];
        for link in links {
            events.push(Event::ProvenanceLinked { link });
        }
        let seqs = self
            .commit(events)
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        Ok(CapabilityAck { committed: seqs })
    }

    fn handle_route_net(
        &mut self,
        track: Track,
        links: Vec<ProvenanceLink>,
    ) -> Result<CapabilityAck, CapabilityError> {
        // The seam (P3): re-validate the trace (positive width, finite endpoints) before
        // committing.
        track
            .validate()
            .map_err(|e| CapabilityError::Rejected(e.to_string()))?;
        // A track that realizes no net is untraceable to the schematic (P3) — reject it.
        if track.net.is_null() {
            return Err(CapabilityError::Rejected("track realizes no net".into()));
        }
        // Referential integrity at the seam: the realized net must be committed, so a track can
        // never realize a phantom net (P3, P5).
        if self.state.net(track.net).is_none() {
            return Err(CapabilityError::Rejected(format!(
                "track realizes unknown net {}",
                track.net.short()
            )));
        }
        // A track is copper on a substrate — require the board first, so routing can never
        // precede the floor plan (P5).
        if self.state.board.is_none() {
            return Err(CapabilityError::Rejected(
                "cannot route a net before the board outline exists".into(),
            ));
        }
        // Single-realization: a net is realized by exactly one track, so a second track for the
        // same net would contradict net-realization completeness — reject it (P5).
        if self.state.tracks.iter().any(|t| t.net == track.net) {
            return Err(CapabilityError::Rejected("net is already routed".into()));
        }

        let mut events = vec![Event::TrackCommitted { track }];
        for link in links {
            events.push(Event::ProvenanceLinked { link });
        }
        let seqs = self
            .commit(events)
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

    fn functional_blocks(&self) -> Vec<FunctionalBlock> {
        self.state.functional_blocks.clone()
    }

    fn components(&self) -> Vec<Component> {
        self.state.components.clone()
    }

    fn pins(&self) -> Vec<Pin> {
        self.state.pins.clone()
    }

    fn nets(&self) -> Vec<Net> {
        self.state.nets.clone()
    }

    fn parts(&self) -> Vec<Part> {
        self.state.parts.clone()
    }

    fn bom_line_items(&self) -> Vec<BomLineItem> {
        self.state.bom_line_items.clone()
    }

    fn board(&self) -> Option<Board> {
        self.state.board.clone()
    }

    fn placements(&self) -> Vec<Placement> {
        self.state.placements.clone()
    }

    fn tracks(&self) -> Vec<Track> {
        self.state.tracks.clone()
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
            CapabilityRequest::CreateFunctionalBlock { block, links } => {
                self.handle_create_functional_block(block, links)
            }
            CapabilityRequest::RealizeComponent {
                component,
                pins,
                links,
            } => self.handle_realize_component(component, pins, links),
            CapabilityRequest::CreateNet { net, links } => self.handle_create_net(net, links),
            CapabilityRequest::CreatePart { part, links } => self.handle_create_part(part, links),
            CapabilityRequest::CreateBomLineItem { item, links } => {
                self.handle_create_bom_line_item(item, links)
            }
            CapabilityRequest::CreateBoard { board, links } => {
                self.handle_create_board(board, links)
            }
            CapabilityRequest::PlaceComponent { placement, links } => {
                self.handle_place_component(placement, links)
            }
            CapabilityRequest::RouteNet { track, links } => self.handle_route_net(track, links),
        }
    }

    fn emit(&mut self, events: Vec<Event>) -> Result<Vec<Seq>, StoreError> {
        self.commit(events)
    }
}
