//! Live Anthropic reasoning adapter (feature `live`). The ONLY crate that knows the
//! provider (P3) — the domain core stays provider-independent.
//!
//! Calls the Anthropic Messages API in tool/JSON mode constrained to the requirement
//! schema, then maps the structured tool output to [`CandidateRequirement`]s. The runtime
//! records the response as a `ReasoningCall` event so the run can be replayed without the
//! model (P4).

use eak_domain::{Priority, RequirementCategory};
use eak_ports::{
    CandidateRequirement, ReasoningEngine, ReasoningError, ReasoningRequest, ReasoningResponse,
};
use serde::Deserialize;

pub struct AnthropicEngine {
    api_key: String,
    model: String,
}

impl AnthropicEngine {
    pub fn new(api_key: String, model: impl Into<String>) -> Self {
        Self {
            api_key,
            model: model.into(),
        }
    }

    /// Construct from `ANTHROPIC_API_KEY`.
    pub fn from_env(model: impl Into<String>) -> Result<Self, ReasoningError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| ReasoningError::Provider("ANTHROPIC_API_KEY not set".into()))?;
        Ok(Self::new(api_key, model))
    }
}

#[derive(Deserialize)]
struct RawCandidate {
    statement: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    priority: String,
    #[serde(default)]
    acceptance_criterion: String,
    #[serde(default)]
    source_hint: String,
    #[serde(default = "default_confidence")]
    confidence: f64,
    #[serde(default)]
    rationale: String,
}
fn default_confidence() -> f64 {
    0.5
}

#[derive(Deserialize)]
struct RawOutput {
    #[serde(default)]
    candidates: Vec<RawCandidate>,
    #[serde(default)]
    clarifying_questions: Vec<String>,
}

fn map_category(s: &str) -> RequirementCategory {
    match s.to_lowercase().as_str() {
        "electrical" => RequirementCategory::Electrical,
        "mechanical" => RequirementCategory::Mechanical,
        "thermal" => RequirementCategory::Thermal,
        "regulatory" => RequirementCategory::Regulatory,
        "cost" => RequirementCategory::Cost,
        "schedule" => RequirementCategory::Schedule,
        _ => RequirementCategory::Functional,
    }
}

fn map_priority(s: &str) -> Priority {
    match s.to_lowercase().as_str() {
        "high" => Priority::High,
        "low" => Priority::Low,
        _ => Priority::Medium,
    }
}

impl ReasoningEngine for AnthropicEngine {
    fn model_id(&self) -> String {
        format!("anthropic:{}", self.model)
    }

    fn request_judgement(
        &self,
        req: &ReasoningRequest,
    ) -> Result<ReasoningResponse, ReasoningError> {
        let tool_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "candidates": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "statement": {"type": "string"},
                            "category": {"type": "string", "enum": ["functional","electrical","mechanical","thermal","regulatory","cost","schedule"]},
                            "priority": {"type": "string", "enum": ["high","medium","low"]},
                            "acceptance_criterion": {"type": "string"},
                            "source_hint": {"type": "string"},
                            "confidence": {"type": "number"},
                            "rationale": {"type": "string"}
                        },
                        "required": ["statement","category","priority","acceptance_criterion"]
                    }
                },
                "clarifying_questions": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["candidates"]
        });
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 2048,
            "temperature": req.temperature,
            "system": req.system,
            "messages": [{"role": "user", "content": req.prompt}],
            "tools": [{
                "name": "emit_requirements",
                "description": "Return structured, testable requirement candidates.",
                "input_schema": tool_schema
            }],
            "tool_choice": {"type": "tool", "name": "emit_requirements"}
        });

        let response = ureq::post("https://api.anthropic.com/v1/messages")
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", "2023-06-01")
            .set("content-type", "application/json")
            .send_json(body);

        let value: serde_json::Value = match response {
            Ok(r) => r
                .into_json()
                .map_err(|e| ReasoningError::Provider(e.to_string()))?,
            Err(ureq::Error::Status(code, r)) => {
                let txt = r.into_string().unwrap_or_default();
                return Err(ReasoningError::Provider(format!("HTTP {code}: {txt}")));
            }
            Err(e) => return Err(ReasoningError::Provider(e.to_string())),
        };

        let content = value
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or_else(|| ReasoningError::Schema("response has no content array".into()))?;
        let input = content
            .iter()
            .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
            .and_then(|b| b.get("input"))
            .ok_or_else(|| ReasoningError::Schema("no tool_use block in response".into()))?;
        let raw: RawOutput = serde_json::from_value(input.clone())
            .map_err(|e| ReasoningError::Schema(e.to_string()))?;

        let candidates = raw
            .candidates
            .into_iter()
            .map(|c| CandidateRequirement {
                statement: c.statement,
                category: map_category(&c.category),
                priority: map_priority(&c.priority),
                acceptance_criterion: c.acceptance_criterion,
                source_hint: c.source_hint,
                confidence: c.confidence,
                rationale: c.rationale,
                targets: vec![],
            })
            .collect();

        Ok(ReasoningResponse {
            candidates,
            clarifying_questions: raw.clarifying_questions,
            raw: value.to_string(),
        })
    }
}
