//! Engineering State — the runtime's owned model, built by folding the event log.
//!
//! State is the fold of an ordered [`Event`] sequence (determinism, P4). Entity-bearing
//! events are state deltas; the rest are audit-only. The same fold runs live during a
//! run and during [`crate::replay`], guaranteeing identical reconstruction.

use eak_domain::{Decision, DesignIntent, EntityId, Evidence, ProvenanceLink, Requirement};
use eak_ports::Event;
use serde::{Deserialize, Serialize};

/// The single canonical instance of everything Phase 1 knows about a design. Entities are
/// kept in insertion (event) order so a run and its replay serialize byte-identically.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EngineeringState {
    pub intent: Option<DesignIntent>,
    pub requirements: Vec<Requirement>,
    pub decisions: Vec<Decision>,
    pub evidence: Vec<Evidence>,
    pub links: Vec<ProvenanceLink>,
}

impl EngineeringState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one event. State-delta variants mutate; audit variants are ignored here
    /// (they still live in the log for provenance).
    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::IntentCaptured { intent } => self.intent = Some(intent.clone()),
            Event::RequirementCommitted { requirement } => {
                self.requirements.push(requirement.clone())
            }
            Event::DecisionCreated { decision } => self.decisions.push(decision.clone()),
            Event::EvidenceReferenced { evidence } => self.evidence.push(evidence.clone()),
            Event::ProvenanceLinked { link } => self.links.push(link.clone()),
            _ => {}
        }
    }

    pub fn requirement(&self, id: EntityId) -> Option<&Requirement> {
        self.requirements.iter().find(|r| r.id == id)
    }

    pub fn decision(&self, id: EntityId) -> Option<&Decision> {
        self.decisions.iter().find(|d| d.id == id)
    }

    pub fn evidence_item(&self, id: EntityId) -> Option<&Evidence> {
        self.evidence.iter().find(|e| e.id == id)
    }

    /// Deterministic serialization used to assert byte-identity between a run and its
    /// replay (the Phase-1 exit criterion).
    pub fn canonical_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("engineering state serializes")
    }
}
