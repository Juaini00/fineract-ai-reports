use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StrictPipelineState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_context: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieval_plan: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieval_evidence: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reranker: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_evaluation: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer_plan: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_answer: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParsedIntentKind {
    Report,
    ClarificationAnswer,
    Unsupported,
    ToolAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedIntent {
    pub intent: ParsedIntentKind,
    pub domain: Option<String>,
    #[serde(default)]
    pub entities: Vec<ParsedEntity>,
    pub constraints: ParsedConstraints,
    pub requires_retrieval: bool,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedEntity {
    pub entity_type: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ParsedConstraints {
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub quantity: Option<QuantityConstraint>,
    pub currency_code: Option<String>,
    pub product_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum QuantityConstraint {
    All,
    Default,
    Limit { value: i64 },
    TopN { value: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteDecision {
    Report,
    Clarify,
    Unsupported,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResolvedConstraints {
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub quantity: Option<QuantityConstraint>,
    pub currency_code: Option<String>,
    pub product_ids: Option<Vec<i64>>,
    pub office_scope: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalEvidence {
    pub source_type: String,
    pub source_id: String,
    pub title: String,
    pub score: f32,
    pub metadata_json: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrictPipelineError {
    pub code: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantity_all_serializes_as_all_mode_without_value() {
        let quantity = QuantityConstraint::All;
        let json = serde_json::to_value(&quantity).unwrap();
        assert_eq!(json["mode"], "all");
        assert!(json.get("value").is_none() || json["value"].is_null());
    }

    #[test]
    fn pipeline_state_records_stage_outputs() {
        let state = StrictPipelineState {
            parser: Some(serde_json::json!({ "intent": "report" })),
            route: Some(serde_json::json!({ "workflow": "report" })),
            ..Default::default()
        };
        assert_eq!(state.parser.as_ref().unwrap()["intent"], "report");
        assert_eq!(state.route.as_ref().unwrap()["workflow"], "report");
    }
}
