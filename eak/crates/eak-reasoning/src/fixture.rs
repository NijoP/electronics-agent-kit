//! Fixture (cassette) reasoning adapter — deterministic, offline, no API key.
//!
//! Resolves a request to a recorded [`ReasoningResponse`] by a stable hash of the prompt,
//! schema, and seed, with an optional `default` for any unmatched request. This is the
//! default adapter for tests and offline `eak run`, and the replay-time stand-in.

use eak_ports::{ReasoningEngine, ReasoningError, ReasoningRequest, ReasoningResponse};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassetteEntry {
    pub key: String,
    pub response: ReasoningResponse,
}

/// A recorded set of reasoning responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cassette {
    #[serde(default)]
    pub entries: Vec<CassetteEntry>,
    #[serde(default)]
    pub default: Option<ReasoningResponse>,
}

pub struct FixtureEngine {
    cassette: Cassette,
}

impl FixtureEngine {
    /// One canned response for any request (handy for tests).
    pub fn single(response: ReasoningResponse) -> Self {
        Self {
            cassette: Cassette {
                entries: vec![],
                default: Some(response),
            },
        }
    }

    pub fn from_cassette(cassette: Cassette) -> Self {
        Self { cassette }
    }

    /// Load a cassette JSON file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ReasoningError> {
        let data =
            std::fs::read_to_string(path).map_err(|e| ReasoningError::Provider(e.to_string()))?;
        let cassette: Cassette =
            serde_json::from_str(&data).map_err(|e| ReasoningError::Schema(e.to_string()))?;
        Ok(Self { cassette })
    }

    /// Stable lookup key for a request (FNV-1a 64 over prompt + schema, xor seed).
    pub fn key(req: &ReasoningRequest) -> String {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for part in [req.prompt.as_str(), req.schema_name.as_str()] {
            for b in part.as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h ^= req.seed;
        format!("{h:016x}")
    }
}

impl ReasoningEngine for FixtureEngine {
    fn model_id(&self) -> String {
        "fixture".into()
    }

    fn request_judgement(
        &self,
        req: &ReasoningRequest,
    ) -> Result<ReasoningResponse, ReasoningError> {
        let key = Self::key(req);
        if let Some(entry) = self.cassette.entries.iter().find(|e| e.key == key) {
            return Ok(entry.response.clone());
        }
        if let Some(default) = &self.cassette.default {
            return Ok(default.clone());
        }
        Err(ReasoningError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_domain::{Priority, RequirementCategory};
    use eak_ports::CandidateRequirement;

    fn req() -> ReasoningRequest {
        ReasoningRequest {
            model_id: String::new(),
            system: String::new(),
            prompt: "p".into(),
            schema_name: "s".into(),
            temperature: 0.0,
            seed: 1,
        }
    }

    #[test]
    fn single_returns_canned() {
        let canned = ReasoningResponse {
            candidates: vec![CandidateRequirement {
                statement: "x".into(),
                category: RequirementCategory::Functional,
                priority: Priority::Low,
                acceptance_criterion: "y".into(),
                source_hint: String::new(),
                confidence: 1.0,
                rationale: String::new(),
                targets: vec![],
            }],
            clarifying_questions: vec![],
            raw: String::new(),
        };
        let engine = FixtureEngine::single(canned);
        let r = engine.request_judgement(&req()).unwrap();
        assert_eq!(r.candidates.len(), 1);
        assert_eq!(engine.model_id(), "fixture");
    }

    #[test]
    fn empty_cassette_is_unavailable() {
        let engine = FixtureEngine::from_cassette(Cassette::default());
        assert!(engine.request_judgement(&req()).is_err());
    }
}
