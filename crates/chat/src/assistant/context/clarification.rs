use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::assistant::intent::SourceIntentSnapshot;

pub const OTHER_CLARIFICATION_OPTION_ID: &str = "others";
pub const CLARIFICATION_VERSION_1: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClarificationKind {
    #[default]
    SelectOption,
    CollectFields,
    FreeText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClarificationFieldType {
    DateRange,
    Integer,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct ClarificationValidation {
    #[serde(default)]
    pub min_integer: Option<i64>,
    #[serde(default)]
    pub max_integer: Option<i64>,
    #[serde(default)]
    pub max_length: Option<u32>,
    #[serde(default)]
    pub max_range_days: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClarificationField {
    pub id: String,
    pub label: String,
    pub field_type: ClarificationFieldType,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClarificationChoice {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<ClarificationField>,
}

/// The client-safe, versioned clarification contract. It deliberately excludes
/// routing attempts and source intent snapshots retained in `ClarificationPayload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClarificationView {
    pub version: u8,
    #[schemars(with = "String")]
    pub id: Uuid,
    pub revision: u32,
    pub kind: ClarificationKind,
    pub question: String,
    #[serde(default)]
    pub options: Vec<ClarificationChoice>,
    #[serde(default)]
    pub fields: Vec<ClarificationField>,
    #[serde(default)]
    pub allow_free_text: bool,
}

impl ClarificationView {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self.kind {
            ClarificationKind::SelectOption if self.options.is_empty() => {
                Err("select_option clarifications require at least one option")
            }
            ClarificationKind::CollectFields if self.fields.is_empty() => {
                Err("collect_fields clarifications require at least one field")
            }
            ClarificationKind::FreeText if !self.fields.is_empty() => {
                Err("free_text clarifications cannot contain fields")
            }
            _ => Ok(()),
        }
    }
}

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
    #[serde(default)]
    pub fields: Vec<ClarificationField>,
}

/// Turns a snake_case capability id into a human-readable label, e.g.
/// `organization_office_summary` -> `Organization Office Summary`.
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
    #[serde(default = "clarification_version")]
    pub version: u8,
    /// Historical payloads normalize to nil rather than generating an unstable ID.
    #[serde(default)]
    #[schemars(with = "String")]
    pub id: Uuid,
    #[serde(default)]
    pub revision: u32,
    #[serde(default)]
    pub kind: ClarificationKind,
    pub question: String,
    #[serde(default)]
    pub options: Vec<ClarificationOption>,
    #[serde(default)]
    pub fields: Vec<ClarificationField>,
    pub attempt: u32,
    #[serde(default)]
    pub source_intent: Option<SourceIntentSnapshot>,
    #[serde(default)]
    pub allow_free_text: bool,
    #[serde(default)]
    pub is_missing_execution_parameters: bool,
}

impl ClarificationPayload {
    pub fn view(&self) -> ClarificationView {
        ClarificationView {
            version: self.version,
            id: self.id,
            revision: self.revision,
            kind: self.kind.clone(),
            question: self.question.clone(),
            options: self
                .options
                .iter()
                .map(|option| ClarificationChoice {
                    id: option.id.clone(),
                    label: option.label.clone(),
                    description: option.description.clone(),
                    fields: option.fields.clone(),
                })
                .collect(),
            fields: self.fields.clone(),
            allow_free_text: self.allow_free_text,
        }
    }
}

fn clarification_version() -> u8 {
    CLARIFICATION_VERSION_1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PendingClarification {
    pub payload: ClarificationPayload,
    #[serde(default)]
    pub source_intent: Option<SourceIntentSnapshot>,
    #[serde(default)]
    pub created_at: Option<String>,
}
