use serde_json::{Map, Value};

use crate::assistant::{
    AssistantIntent, ClarificationPayload,
    response::{
        AssistantResponse, AssistantResponseType, ResponseAction, ResponseActionType,
        ResponseOption, ResponseTable, ResponseWarning, TableColumn, TableColumnKind,
    },
};
use crate::chat::planner::{ExecutionPlan, PolicyDecision};
use crate::knowledge::model::{KnowledgeCatalog, QueryOutputField};

pub struct ResponseBuilder;

impl ResponseBuilder {
    pub fn from_tool_result(
        _intent: &AssistantIntent,
        plan: &ExecutionPlan,
        policy: &PolicyDecision,
        execution_result: &Value,
        catalog: &KnowledgeCatalog,
    ) -> AssistantResponse {
        let fields = catalog
            .queries
            .iter()
            .find(|query| query.id == plan.query_id)
            .map(|query| query.output_fields.as_slice())
            .unwrap_or(&[]);
        let rows = execution_result
            .get("rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let columns = fields
            .iter()
            .map(|field| table_column(field, policy.can_view_pii))
            .collect::<Vec<_>>();
        let rows = rows
            .into_iter()
            .map(|row| filtered_row(row, fields, policy.can_view_pii))
            .collect::<Vec<_>>();
        let warnings = fields
            .iter()
            .any(|field| is_hidden(field, policy.can_view_pii))
            .then(|| ResponseWarning {
                code: "pii_hidden".into(),
                message: "Some sensitive columns are hidden by policy.".into(),
            })
            .into_iter()
            .collect();

        AssistantResponse {
            response_type: AssistantResponseType::Table,
            title: Some("Lookup results".into()),
            message: format!("Found {} row(s).", rows.len()),
            sections: Vec::new(),
            table: Some(ResponseTable { columns, rows }),
            cards: Vec::new(),
            options: Vec::new(),
            warnings,
            actions: Vec::new(),
        }
    }

    pub fn clarification(payload: ClarificationPayload) -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::Clarification,
            title: Some("Please clarify the report".into()),
            message: payload.question,
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            options: payload
                .options
                .into_iter()
                .map(|option| ResponseOption {
                    id: option.id,
                    label: option.label,
                    description: option.description,
                })
                .collect(),
            warnings: Vec::new(),
            actions: vec![ResponseAction {
                action_type: ResponseActionType::AskFollowUp,
                label: "Clarify request".into(),
            }],
        }
    }

    pub fn selected(capability_id: String) -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::Summary,
            title: Some("Report capability selected".into()),
            message: format!(
                "I found strong evidence for `{capability_id}`, but execution is unavailable in this context."
            ),
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            options: Vec::new(),
            warnings: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub fn greeting() -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::Summary,
            title: Some("Hello".into()),
            message: "Hi — I can help with approved reporting questions for your authorized scope."
                .into(),
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            options: Vec::new(),
            warnings: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub fn help() -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::Help,
            title: Some("Reporting assistant help".into()),
            message: "I can answer approved reporting requests for savings, clients, and organization data within your API key scope.".into(),
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            options: Vec::new(),
            warnings: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub fn context_window_exceeded() -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::Error,
            title: Some("Context window exceeded".into()),
            message: "This conversation is too long to route safely. Please start a new session."
                .into(),
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            options: Vec::new(),
            warnings: Vec::new(),
            actions: vec![ResponseAction {
                action_type: ResponseActionType::StartNewSession,
                label: "Start a new session".into(),
            }],
        }
    }

    pub fn missing_parameter(message: &str) -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::Clarification,
            title: Some("Please clarify the lookup".into()),
            message: message.into(),
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            options: Vec::new(),
            warnings: Vec::new(),
            actions: vec![ResponseAction {
                action_type: ResponseActionType::AskFollowUp,
                label: "Provide the missing detail".into(),
            }],
        }
    }

    pub fn free_form_other_prompt() -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::Clarification,
            title: Some("Describe your request".into()),
            message: "Please describe what you need in your own words. I will treat your next message as a new request.".into(),
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            options: Vec::new(),
            warnings: Vec::new(),
            actions: vec![ResponseAction {
                action_type: ResponseActionType::AskFollowUp,
                label: "Describe request".into(),
            }],
        }
    }

    pub fn unsupported() -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::Unsupported,
            title: Some("Unsupported request".into()),
            message:
                "This request is in scope, but it is not supported by the approved catalog yet."
                    .into(),
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            options: Vec::new(),
            warnings: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub fn out_of_domain() -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::OutOfDomain,
            title: Some("Out of domain".into()),
            message: "This assistant can only help with approved reporting requests.".into(),
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            options: Vec::new(),
            warnings: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub fn policy_blocked(reason: &str) -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::PolicyBlocked,
            title: Some("Blocked by policy".into()),
            message: reason.into(),
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            options: Vec::new(),
            warnings: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub fn error() -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::Error,
            title: Some("Routing failed".into()),
            message: "I could not route this request safely. Please try again.".into(),
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            options: Vec::new(),
            warnings: Vec::new(),
            actions: vec![ResponseAction {
                action_type: ResponseActionType::AskFollowUp,
                label: "Try again".into(),
            }],
        }
    }
}

fn filtered_row(row: Value, fields: &[QueryOutputField], can_view_pii: bool) -> Value {
    let Value::Object(mut source) = row else {
        return row;
    };
    let mut out = Map::new();
    for field in fields {
        if !is_hidden(field, can_view_pii)
            && let Some(value) = source.remove(&field.name)
        {
            out.insert(field.name.clone(), value);
        }
    }
    Value::Object(out)
}

fn table_column(field: &QueryOutputField, can_view_pii: bool) -> TableColumn {
    TableColumn {
        key: field.name.clone(),
        label: field.name.replace('_', " "),
        kind: match field.kind.as_str() {
            "integer" | "bigint" => TableColumnKind::Number,
            "decimal" => TableColumnKind::Decimal,
            "date" => TableColumnKind::Date,
            _ => TableColumnKind::Text,
        },
        hidden: is_hidden(field, can_view_pii),
    }
}

fn is_hidden(field: &QueryOutputField, can_view_pii: bool) -> bool {
    !can_view_pii && field.sensitivity == "pii"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assistant::{
            AssistantConstraints, AssistantDomain, AssistantIntentKind, AssistantLanguage,
            ContextReference,
        },
        chat::planner::{
            AnswerPlan, EvidenceEvaluation, ExecutionPlanType, PolicyDecisionStatus, RetrievalPlan,
        },
        knowledge::model::{KnowledgeCatalog, QueryKnowledge},
    };
    use serde_json::json;

    #[test]
    fn hides_pii_columns_and_values_when_policy_disallows_pii() {
        let response = ResponseBuilder::from_tool_result(
            &intent(),
            &plan(),
            &PolicyDecision {
                status: PolicyDecisionStatus::Allowed,
                reason: None,
                office_ids: vec![1],
                can_view_pii: false,
            },
            &json!({ "rows": [{ "name": "Ada", "national_id": "SECRET" }] }),
            &catalog(),
        );

        let table = response.table.unwrap();
        assert!(
            table
                .columns
                .iter()
                .any(|column| column.key == "national_id" && column.hidden)
        );
        assert_eq!(table.rows, vec![json!({ "name": "Ada" })]);
        assert_eq!(response.warnings[0].code, "pii_hidden");
    }

    fn intent() -> AssistantIntent {
        AssistantIntent {
            intent: AssistantIntentKind::DataLookup,
            domain: AssistantDomain::Client,
            language: AssistantLanguage::En,
            entities: Vec::new(),
            constraints: AssistantConstraints::default(),
            context_reference: ContextReference::None,
            confidence: 1.0,
            reason: "test".into(),
        }
    }

    fn plan() -> ExecutionPlan {
        ExecutionPlan {
            plan_type: ExecutionPlanType::Atomic,
            domain: "client".into(),
            capability: "client_lookup".into(),
            query_id: "client.lookup".into(),
            output_mode: "list".into(),
            params: json!({}),
            retrieval_plan: RetrievalPlan::default(),
            evidence_evaluation: EvidenceEvaluation::default(),
            answer_plan: AnswerPlan::default(),
            requires_policy_check: true,
        }
    }

    fn catalog() -> KnowledgeCatalog {
        KnowledgeCatalog {
            root_path: Default::default(),
            query_path: Default::default(),
            data_areas: Vec::new(),
            domains: Vec::new(),
            schemas: Vec::new(),
            metrics: Vec::new(),
            capabilities: Vec::new(),
            queries: vec![QueryKnowledge {
                id: "client.lookup".into(),
                database: "fineract".into(),
                sql_file: "client.sql".into(),
                data_areas: Vec::new(),
                tables: Vec::new(),
                metrics: Vec::new(),
                parameters: Vec::new(),
                output_fields: vec![
                    QueryOutputField {
                        name: "name".into(),
                        kind: "string".into(),
                        sensitivity: "public_business".into(),
                    },
                    QueryOutputField {
                        name: "national_id".into(),
                        kind: "string".into(),
                        sensitivity: "pii".into(),
                    },
                ],
            }],
            policies: Vec::new(),
            responses: Vec::new(),
            classification: Default::default(),
        }
    }
}
