use serde::{Deserialize, Serialize};

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
}
