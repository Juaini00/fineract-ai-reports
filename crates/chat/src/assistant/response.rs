use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    #[serde(default)]
    pub options: Vec<ResponseOption>,
    #[serde(default)]
    pub warnings: Vec<ResponseWarning>,
    #[serde(default)]
    pub actions: Vec<ResponseAction>,
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
#[serde(rename_all = "snake_case")]
pub enum ResponseActionType {
    StartNewSession,
    Refine,
    Export,
    AskFollowUp,
}

pub trait ResponseRenderer {
    fn render(&self, response: &AssistantResponse) -> String;
}

pub struct MarkdownRenderer;

impl ResponseRenderer for MarkdownRenderer {
    fn render(&self, response: &AssistantResponse) -> String {
        let mut out = String::new();
        if let Some(title) = &response.title {
            out.push_str("# ");
            out.push_str(title);
            out.push_str("\n\n");
        }
        out.push_str(&response.message);
        for section in &response.sections {
            out.push_str("\n\n## ");
            out.push_str(&section.title);
            out.push('\n');
            out.push_str(&section.body);
        }
        if let Some(table) = &response.table {
            render_table(&mut out, table);
        }
        if !response.cards.is_empty() {
            out.push_str("\n\n## Metrics");
            for card in &response.cards {
                out.push_str("\n- **");
                out.push_str(&card.label);
                out.push_str("**: ");
                out.push_str(&card.value);
                if let Some(unit) = &card.unit {
                    out.push(' ');
                    out.push_str(unit);
                }
            }
        }
        if !response.options.is_empty() {
            out.push_str("\n\n## Options");
            for option in &response.options {
                out.push_str("\n- ");
                out.push_str(&option.label);
                if let Some(description) = &option.description {
                    out.push_str(": ");
                    out.push_str(description);
                }
            }
        }
        if !response.warnings.is_empty() {
            out.push_str("\n\n## Warnings");
            for warning in &response.warnings {
                out.push_str("\n- ");
                out.push_str(&warning.message);
            }
        }
        if !response.actions.is_empty() {
            out.push_str("\n\n## Actions");
            for action in &response.actions {
                out.push_str("\n- ");
                out.push_str(&action.label);
            }
        }
        out
    }
}

fn render_table(out: &mut String, table: &ResponseTable) {
    let visible: Vec<_> = table
        .columns
        .iter()
        .filter(|column| !column.hidden)
        .collect();
    if visible.is_empty() {
        return;
    }
    out.push_str("\n\n");
    out.push('|');
    for column in &visible {
        out.push_str(&column.label);
        out.push('|');
    }
    out.push_str("\n|");
    for _ in &visible {
        out.push_str("---|");
    }
    for row in &table.rows {
        out.push_str("\n|");
        for column in &visible {
            out.push_str(&cell(row, &column.key));
            out.push('|');
        }
    }
}

fn cell(row: &Value, key: &str) -> String {
    match row.get(key) {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_response() -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::Table,
            title: None,
            message: "Report".into(),
            sections: vec![],
            table: None,
            cards: vec![],
            options: vec![],
            warnings: vec![],
            actions: vec![],
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
