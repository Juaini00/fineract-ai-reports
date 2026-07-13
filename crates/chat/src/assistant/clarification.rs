use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const OTHER_CLARIFICATION_OPTION_ID: &str = "others";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum ClarificationOutcome {
    SelectedOption {
        option_id: String,
        confidence: f32,
    },
    RefinedConstraints {
        constraints: serde_json::Value,
        confidence: f32,
    },
    NewRequest {
        message: String,
        confidence: f32,
    },
    FreeFormOther {
        text: String,
        confidence: f32,
    },
    Cancelled,
    Unresolved {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClarificationOption {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClarificationPayload {
    pub question: String,
    #[serde(default)]
    pub options: Vec<ClarificationOption>,
    pub attempt: u32,
}
