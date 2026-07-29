//! Layer 1 (LLM Gateway) output contract.
//!
//! Matches spec §4.1 exactly. `AssistantIntentKind`, `AssistantDomain`,
//! `AssistantLanguage`, `AssistantEntityType` are reused from `intent.rs` since
//! their variants already cover the spec.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::assistant::understanding::intent::{
    AssistantDomain, AssistantEntityType, AssistantIntentKind, AssistantLanguage,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LlmGatewayExtraction {
    pub intent_kind: AssistantIntentKind,
    pub domain: AssistantDomain,
    pub language: AssistantLanguage,
    #[serde(default)]
    pub entities: Vec<GatewayEntity>,
    #[serde(default)]
    pub temporal_hint: Option<TemporalHint>,
    #[serde(default)]
    pub quantity_hint: Option<QuantityHint>,
    #[serde(default)]
    pub candidates: Vec<GatewayCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GatewayEntity {
    #[serde(rename = "type")]
    pub entity_type: AssistantEntityType,
    pub value: String,
    pub phrase_span: [u32; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TemporalHint {
    pub phrase: String,
    pub phrase_span: [u32; 2],
    pub inferred: TemporalInferred,
    #[serde(default)]
    pub range_hint: Option<TemporalRangeHint>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemporalInferred {
    Today,
    Yesterday,
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    ThisYear,
    LastYear,
    Recent,
    AsOfNow,
    Range,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TemporalRangeHint {
    pub from_phrase: String,
    pub to_phrase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QuantityHint {
    pub phrase: String,
    pub phrase_span: [u32; 2],
    pub inferred: QuantityInferred,
    #[serde(default)]
    pub value: Option<i64>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuantityInferred {
    All,
    TopN,
    Limit,
    Default,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GatewayCandidate {
    pub capability_id: String,
    pub confidence: f32,
    pub why: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../../tests/fixtures/gateway/extraction_sample.json");

    #[test]
    fn extraction_round_trips_through_serde() {
        let parsed: LlmGatewayExtraction =
            serde_json::from_str(FIXTURE).expect("fixture deserializes");
        let reserialized = serde_json::to_string(&parsed).expect("reserializes");
        let reparsed: LlmGatewayExtraction =
            serde_json::from_str(&reserialized).expect("stable round-trip");
        assert_eq!(reparsed.candidates.len(), parsed.candidates.len());
        assert_eq!(reparsed.entities.len(), parsed.entities.len());
        assert!(matches!(
            reparsed.intent_kind,
            AssistantIntentKind::ReportRequest
        ));
    }
}
