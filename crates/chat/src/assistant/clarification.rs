use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::intent::SourceIntentSnapshot;

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

/// Turns a snake_case capability id into a human-readable label, e.g.
/// `organization_office_summary` -> `Organization Office Summary`. Used as
/// the label fallback when a capability's YAML has no `display_name`
/// (issue 08 Bug A).
pub fn humanize_id(id: &str) -> String {
    id.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClarificationPayload {
    pub question: String,
    #[serde(default)]
    pub options: Vec<ClarificationOption>,
    pub attempt: u32,
    #[serde(default)]
    pub source_intent: Option<SourceIntentSnapshot>,
    #[serde(default)]
    pub allow_free_text: bool,
    #[serde(default)]
    pub is_missing_execution_parameters: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PendingClarification {
    pub payload: ClarificationPayload,
    #[serde(default)]
    pub source_intent: Option<SourceIntentSnapshot>,
    #[serde(default)]
    pub created_at: Option<String>,
}
