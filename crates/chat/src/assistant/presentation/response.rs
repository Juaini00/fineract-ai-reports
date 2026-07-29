use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::assistant::clarification::ClarificationView;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AssistantResponse {
    pub response_type: AssistantResponseType,
    pub title: Option<String>,
    pub message: String,
    #[serde(default)]
    pub sections: Vec<ResponseSection>,
    pub table: Option<ResponseTable>,
    #[serde(default)]
    pub cards: Vec<ResponseCard>,
    /// Deprecated compatibility projection of clarification options.
    #[serde(default)]
    pub options: Vec<ResponseOption>,
    /// Versioned client-safe clarification contract.
    #[serde(default)]
    pub clarification: Option<ClarificationView>,
    #[serde(default)]
    pub warnings: Vec<ResponseWarning>,
    #[serde(default)]
    pub actions: Vec<ResponseAction>,
    #[serde(default)]
    pub evidence_refs: Vec<EvidenceReference>,
    #[serde(default)]
    pub rendered_markdown: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssistantResponseType {
    Summary,
    Table,
    MetricCards,
    Clarification,
    Help,
    Unsupported,
    OutOfDomain,
    PolicyBlocked,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResponseSection {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResponseTable {
    pub columns: Vec<TableColumn>,
    #[serde(default)]
    pub rows: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TableColumn {
    pub key: String,
    pub label: String,
    pub kind: TableColumnKind,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TableColumnKind {
    Text,
    Number,
    Decimal,
    Date,
    Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResponseCard {
    pub label: String,
    pub value: String,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResponseOption {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResponseWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResponseAction {
    pub action_type: ResponseActionType,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceReference {
    pub id: String,
    pub source_type: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResponseActionType {
    StartNewSession,
    Refine,
    Export,
    AskFollowUp,
}

#[cfg(test)]
mod tests {
    use super::super::renderer::{MarkdownRenderer, ResponseRenderer};
    use super::*;
    use crate::assistant::clarification::{
        ClarificationField, ClarificationFieldType, ClarificationKind, ClarificationValidation,
        ClarificationView,
    };
    use serde_json::json;
    use uuid::Uuid;

    fn base_response() -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::Table,
            title: None,
            message: "Report".into(),
            sections: vec![],
            table: None,
            cards: vec![],
            options: vec![],
            clarification: None,
            warnings: vec![],
            actions: vec![],
            evidence_refs: vec![],
            rendered_markdown: None,
        }
    }

    #[test]
    fn renders_table_without_hidden_columns() {
        let mut response = base_response();
        response.table = Some(ResponseTable {
            columns: vec![
                TableColumn {
                    key: "name".into(),
                    label: "Name".into(),
                    kind: TableColumnKind::Text,
                    hidden: false,
                },
                TableColumn {
                    key: "national_id".into(),
                    label: "National ID".into(),
                    kind: TableColumnKind::Text,
                    hidden: true,
                },
            ],
            rows: vec![json!({ "name": "Ada", "national_id": "SECRET" })],
        });
        let rendered = MarkdownRenderer.render(&response);
        assert!(rendered.contains("|Name|"));
        assert!(!rendered.contains("National ID"));
        assert!(!rendered.contains("SECRET"));
    }

    #[test]
    fn renders_table_cells_safely_and_bounds_markdown_rows() {
        let mut response = base_response();
        response.table = Some(ResponseTable {
            columns: vec![TableColumn {
                key: "value".into(),
                label: "Value".into(),
                kind: TableColumnKind::Text,
                hidden: false,
            }],
            rows: (0..51)
                .map(|index| {
                    json!({
                        "value": if index == 0 { "A|B\r\nC".to_string() } else { format!("row-{index}") }
                    })
                })
                .collect(),
        });

        let rendered = MarkdownRenderer.render(&response);
        assert_eq!(response.table.as_ref().unwrap().rows.len(), 51);
        assert!(rendered.contains("A\\|B<br>C"));
        assert!(rendered.contains("row-49"));
        assert!(!rendered.contains("row-50"));
    }

    #[test]
    fn renders_clarification_question_fields_and_safe_label_fallback() {
        let mut response = base_response();
        response.response_type = AssistantResponseType::Clarification;
        response.clarification = Some(ClarificationView {
            version: 1,
            id: Uuid::nil(),
            revision: 2,
            kind: ClarificationKind::CollectFields,
            question: "Which period should I use?".into(),
            options: vec![],
            fields: vec![ClarificationField {
                key: "date_range".into(),
                label: "".into(),
                field_type: ClarificationFieldType::DateRange,
                required: true,
                value: None,
                default_value: None,
                help_text: Some("Use ISO dates.".into()),
                validation: ClarificationValidation::default(),
                errors: vec!["Choose a range of 31 days or fewer.".into()],
            }],
            allow_free_text: false,
        });

        let rendered = MarkdownRenderer.render(&response);
        assert!(rendered.contains("## Question\nWhich period should I use?"));
        assert!(rendered.contains("date_range (required): Use ISO dates."));
        assert!(rendered.contains("Error: Choose a range of 31 days or fewer."));
        assert!(!rendered.contains("00000000-0000-0000-0000-000000000000"));
    }

    #[test]
    fn renders_options_warnings_and_actions() {
        let mut response = base_response();
        response.response_type = AssistantResponseType::Clarification;
        response.options = vec![ResponseOption {
            id: "a".into(),
            label: "A".into(),
            description: Some("Alpha".into()),
        }];
        response.warnings = vec![ResponseWarning {
            code: "limited".into(),
            message: "Limited data".into(),
        }];
        response.actions = vec![ResponseAction {
            action_type: ResponseActionType::Refine,
            label: "Refine".into(),
        }];
        let rendered = MarkdownRenderer.render(&response);
        assert!(rendered.contains("- A: Alpha"));
        assert!(rendered.contains("- Limited data"));
        assert!(rendered.contains("- Refine"));
    }
}
