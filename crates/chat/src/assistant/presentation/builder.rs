use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use rust_decimal::{Decimal, RoundingStrategy};
use serde_json::{Map, Value};

use crate::assistant::execution::plan::{ExecutionPlan, PolicyDecision};
use crate::assistant::temporal::BusinessDateSource;
use crate::assistant::{
    AssistantIntent, ClarificationPayload,
    execution::tool::ToolResult,
    presentation::renderer::{MarkdownRenderer, ResponseRenderer},
    response::{
        AssistantResponse, AssistantResponseType, EvidenceReference, ResponseAction,
        ResponseActionType, ResponseCard, ResponseOption, ResponseTable, ResponseWarning,
        TableColumn, TableColumnKind,
    },
};
use crate::knowledge::model::{KnowledgeCatalog, QueryOutputField, Sensitivity};

pub struct ResponseBuilder;

impl ResponseBuilder {
    pub fn reporting_date_note(
        business_today: NaiveDate,
        source: BusinessDateSource,
        wall_today: NaiveDate,
    ) -> Option<ResponseWarning> {
        (source == BusinessDateSource::Fineract && business_today != wall_today).then(|| {
            ResponseWarning {
                code: "reporting_date".into(),
                message: format!(
                    "Reporting date is the Fineract business date {business_today}, which differs from the calendar date {wall_today}."
                ),
            }
        })
    }

    pub fn from_tool_result(
        _intent: &AssistantIntent,
        plan: &ExecutionPlan,
        policy: &PolicyDecision,
        tool_result: &ToolResult,
        catalog: &KnowledgeCatalog,
    ) -> AssistantResponse {
        let dataset_fields = plan.dataset_selection.as_ref().and_then(|selection| {
            catalog
                .datasets
                .iter()
                .find(|dataset| dataset.id == selection.dataset_id)
                .and_then(|dataset| {
                    dataset.shape(&selection.shape_id).map(|shape| {
                        shape
                            .output_fields(dataset)
                            .iter()
                            .filter(|field| {
                                field.core
                                    || selection.projection.iter().any(|name| name == &field.name)
                            })
                            .map(|field| QueryOutputField {
                                name: field.name.clone(),
                                kind: field.kind.clone(),
                                sensitivity: field.sensitivity,
                            })
                            .collect::<Vec<_>>()
                    })
                })
        });
        let query_fields = catalog
            .queries
            .iter()
            .find(|query| query.id == plan.query_id)
            .map(|query| query.output_fields.as_slice())
            .unwrap_or(&[]);
        let fields = dataset_fields.as_deref().unwrap_or(query_fields);
        let rows = tool_result.rows.clone();
        let columns = fields
            .iter()
            .map(|field| table_column(field, policy.can_view_pii, has_currency_metadata(fields)))
            .collect::<Vec<_>>();
        let rows = rows
            .into_iter()
            .map(|row| filtered_row(row, fields, policy.can_view_pii))
            .collect::<Vec<_>>();
        let mut warnings = fields
            .iter()
            .any(|field| is_hidden(field, policy.can_view_pii))
            .then(|| ResponseWarning {
                code: "pii_hidden".into(),
                message: "Some sensitive columns are hidden by policy.".into(),
            })
            .into_iter()
            .collect::<Vec<_>>();
        if let Some(shown) = tool_result.truncated {
            warnings.push(ResponseWarning {
                code: "result_truncated".into(),
                message: format!(
                    "Showing the first {shown} rows. More than {shown} rows match; \
                     narrow your request (add a date range, office, or lower limit) to see the rest."
                ),
            });
        }

        let cards = response_cards(&rows, has_currency_metadata(fields));
        if has_currency_metadata(fields) && currency_codes(&rows).len() > 1 {
            warnings.push(ResponseWarning {
                code: "multi_currency".into(),
                message: "Amounts are shown separately by currency; no combined total is provided."
                    .into(),
            });
        }

        let row_count = rows.len();
        let message = if plan.capability == "client_name_lookup" {
            match row_count {
                0 => "No matching client was found in your authorized office scope.".into(),
                1 => "Found one matching client in your authorized office scope.".into(),
                _ => format!(
                    "Found {row_count} matching clients. Please use the table to disambiguate."
                ),
            }
        } else {
            format!("Found {row_count} row(s).")
        };
        finish(AssistantResponse {
            response_type: AssistantResponseType::Table,
            title: Some("Lookup results".into()),
            message,
            sections: Vec::new(),
            table: Some(ResponseTable { columns, rows }),
            cards,
            options: Vec::new(),
            clarification: None,
            warnings,
            actions: Vec::new(),
            evidence_refs: tool_result
                .evidence_refs
                .iter()
                .map(|id| EvidenceReference {
                    id: id.clone(),
                    source_type: "retrieval_evidence".into(),
                    label: None,
                })
                .collect(),
            rendered_markdown: None,
        })
    }

    pub fn clarification(payload: ClarificationPayload) -> AssistantResponse {
        AssistantResponse {
            response_type: AssistantResponseType::Clarification,
            title: Some("Please clarify the report".into()),
            message: payload.question.clone(),
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            // Retain this deprecated projection so existing clients can render V1.
            options: payload
                .options
                .iter()
                .map(|option| ResponseOption {
                    id: option.id.clone(),
                    label: option.label.clone(),
                    description: option.description.clone(),
                })
                .collect(),
            clarification: Some(payload.view()),
            warnings: Vec::new(),
            actions: vec![ResponseAction {
                action_type: ResponseActionType::AskFollowUp,
                label: "Clarify request".into(),
            }],
            evidence_refs: Vec::new(),
            rendered_markdown: None,
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
            clarification: None,
            warnings: Vec::new(),
            actions: Vec::new(),
            evidence_refs: Vec::new(),
            rendered_markdown: None,
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
            clarification: None,
            warnings: Vec::new(),
            actions: Vec::new(),
            evidence_refs: Vec::new(),
            rendered_markdown: None,
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
            clarification: None,
            warnings: Vec::new(),
            actions: Vec::new(),
            evidence_refs: Vec::new(),
            rendered_markdown: None,
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
            clarification: None,
            warnings: Vec::new(),
            actions: vec![ResponseAction {
                action_type: ResponseActionType::StartNewSession,
                label: "Start a new session".into(),
            }],
            evidence_refs: Vec::new(),
            rendered_markdown: None,
        }
    }

    pub fn missing_parameter(message: &str) -> AssistantResponse {
        Self::clarification(ClarificationPayload {
            version: crate::assistant::clarification::CLARIFICATION_VERSION_1,
            id: uuid::Uuid::new_v4(),
            revision: 0,
            kind: crate::assistant::clarification::ClarificationKind::FreeText,
            question: message.into(),
            options: Vec::new(),
            fields: Vec::new(),
            attempt: 1,
            source_intent: None,
            allow_free_text: true,
            is_missing_execution_parameters: true,
        })
    }

    pub fn free_form_other_prompt() -> AssistantResponse {
        Self::clarification(ClarificationPayload {
            version: crate::assistant::clarification::CLARIFICATION_VERSION_1,
            id: uuid::Uuid::new_v4(),
            revision: 0,
            kind: crate::assistant::clarification::ClarificationKind::FreeText,
            question: "Please describe what you need in your own words. I will treat your next message as a new request.".into(),
            options: Vec::new(),
            fields: Vec::new(),
            attempt: 1,
            source_intent: None,
            allow_free_text: true,
            is_missing_execution_parameters: false,
        })
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
            clarification: None,
            warnings: Vec::new(),
            actions: Vec::new(),
            evidence_refs: Vec::new(),
            rendered_markdown: None,
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
            clarification: None,
            warnings: Vec::new(),
            actions: Vec::new(),
            evidence_refs: Vec::new(),
            rendered_markdown: None,
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
            clarification: None,
            warnings: Vec::new(),
            actions: Vec::new(),
            evidence_refs: Vec::new(),
            rendered_markdown: None,
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
            clarification: None,
            warnings: Vec::new(),
            actions: vec![ResponseAction {
                action_type: ResponseActionType::AskFollowUp,
                label: "Try again".into(),
            }],
            evidence_refs: Vec::new(),
            rendered_markdown: None,
        }
    }
}

pub fn finish(mut response: AssistantResponse) -> AssistantResponse {
    response.rendered_markdown = Some(MarkdownRenderer.render(&response));
    response
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

fn has_currency_metadata(fields: &[QueryOutputField]) -> bool {
    [
        "currency_code",
        "currency_digits",
        "currency_display_symbol",
    ]
    .into_iter()
    .all(|name| fields.iter().any(|field| field.name == name))
}

fn currency_codes(rows: &[Value]) -> BTreeSet<String> {
    rows.iter()
        .filter_map(|row| row.get("currency_code").and_then(Value::as_str))
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .map(str::to_owned)
        .collect()
}

fn response_cards(rows: &[Value], has_currency_metadata: bool) -> Vec<ResponseCard> {
    let mut cards = vec![ResponseCard {
        label: "Rows".into(),
        value: rows.len().to_string(),
        unit: None,
    }];
    if !has_currency_metadata {
        return cards;
    }

    let mut totals = BTreeMap::<String, (u32, Option<String>, Decimal)>::new();
    for row in rows {
        let Some(code) = row
            .get("currency_code")
            .and_then(Value::as_str)
            .map(str::trim)
        else {
            continue;
        };
        if code.is_empty() {
            continue;
        }
        let Some(digits) = row.get("currency_digits").and_then(Value::as_u64) else {
            continue;
        };
        let Some(amount) = row
            .get("amount_outstanding")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<Decimal>().ok())
        else {
            continue;
        };
        let symbol = row
            .get("currency_display_symbol")
            .and_then(Value::as_str)
            .filter(|symbol| !symbol.trim().is_empty())
            .map(str::to_owned);
        let entry = totals
            .entry(code.to_owned())
            .or_insert((digits as u32, symbol, Decimal::ZERO));
        entry.2 += amount;
    }
    for (code, (digits, symbol, amount)) in totals {
        cards.push(ResponseCard {
            label: format!("Outstanding ({code})"),
            value: format_money(amount, digits, symbol.as_deref(), &code),
            unit: None,
        });
    }
    cards
}

pub(crate) fn format_money(
    value: Decimal,
    digits: u32,
    symbol: Option<&str>,
    code: &str,
) -> String {
    let rounded = value.round_dp_with_strategy(digits, RoundingStrategy::MidpointAwayFromZero);
    let amount = format!("{rounded:.precision$}", precision = digits as usize);
    let prefix = symbol
        .filter(|symbol| !symbol.trim().is_empty())
        .unwrap_or(code);
    format!("{prefix} {amount}").replace("$ ", "$")
}

fn table_column(
    field: &QueryOutputField,
    can_view_pii: bool,
    has_currency_metadata: bool,
) -> TableColumn {
    TableColumn {
        key: field.name.clone(),
        label: field.name.replace('_', " "),
        kind: match field.kind.as_str() {
            "integer" | "bigint" => TableColumnKind::Number,
            "decimal" if has_currency_metadata => TableColumnKind::Money,
            "decimal" => TableColumnKind::Decimal,
            "date" => TableColumnKind::Date,
            _ => TableColumnKind::Text,
        },
        hidden: is_hidden(field, can_view_pii),
    }
}

fn is_hidden(field: &QueryOutputField, can_view_pii: bool) -> bool {
    match field.sensitivity {
        Sensitivity::Pii => !can_view_pii,
        Sensitivity::PublicBusiness | Sensitivity::MaskedOutput => false,
        Sensitivity::FilterOnly | Sensitivity::NeverUse => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assistant::execution::plan::{
            EvidenceEvaluation, ExecutionPlanType, PolicyDecisionStatus, RetrievalPlan,
        },
        assistant::{
            AssistantConstraints, AssistantDomain, AssistantIntentKind, AssistantLanguage,
            ContextReference,
        },
        knowledge::model::{KnowledgeCatalog, QueryKnowledge, Sensitivity},
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
            &ToolResult {
                tool_name: "approved_catalog_sql".into(),
                ok: true,
                rows: vec![json!({ "name": "Ada", "national_id": "SECRET" })],
                summary: None,
                error: None,
                evidence_refs: vec!["ev1".into()],
                truncated: None,
            },
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
        assert!(response.rendered_markdown.unwrap().contains("Ada"));
    }

    #[test]
    fn warns_when_result_is_truncated() {
        let response = ResponseBuilder::from_tool_result(
            &intent(),
            &plan(),
            &PolicyDecision {
                status: PolicyDecisionStatus::Allowed,
                reason: None,
                office_ids: vec![1],
                can_view_pii: true,
            },
            &ToolResult {
                tool_name: "approved_catalog_sql".into(),
                ok: true,
                rows: vec![json!({ "name": "Ada" })],
                summary: None,
                error: None,
                evidence_refs: vec![],
                truncated: Some(1),
            },
            &catalog(),
        );

        assert!(
            response
                .warnings
                .iter()
                .any(|warning| warning.code == "result_truncated")
        );
    }

    #[test]
    fn reporting_date_note_only_when_fineract_and_differing() {
        let business = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
        let wall = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();
        let note =
            ResponseBuilder::reporting_date_note(business, BusinessDateSource::Fineract, wall)
                .expect("Fineract date differs");
        assert_eq!(note.code, "reporting_date");
        assert!(note.message.contains("2026-07-23"));
        assert!(note.message.contains("2026-07-25"));
        assert!(
            ResponseBuilder::reporting_date_note(business, BusinessDateSource::Fineract, business,)
                .is_none()
        );
        assert!(
            ResponseBuilder::reporting_date_note(
                business,
                BusinessDateSource::WallClockFallback,
                wall,
            )
            .is_none()
        );
    }

    #[test]
    fn formats_charge_money_per_currency_without_cross_currency_total() {
        let response = ResponseBuilder::from_tool_result(
            &intent(),
            &charge_plan(),
            &allowed_policy(),
            &ToolResult {
                tool_name: "approved_catalog_sql".into(),
                ok: true,
                rows: vec![
                    json!({
                        "currency_code": "USD",
                        "currency_digits": 2,
                        "currency_display_symbol": "$",
                        "amount_outstanding": "100.000000"
                    }),
                    json!({
                        "currency_code": "AED",
                        "currency_digits": 0,
                        "currency_display_symbol": null,
                        "amount_outstanding": "20.500000"
                    }),
                ],
                summary: None,
                error: None,
                evidence_refs: vec![],
                truncated: None,
            },
            &charge_catalog(),
        );

        let table = response.table.as_ref().unwrap();
        assert_eq!(table.rows[0]["amount_outstanding"], "100.000000");
        assert_eq!(
            table
                .columns
                .iter()
                .find(|column| column.key == "amount_outstanding")
                .unwrap()
                .kind,
            TableColumnKind::Money
        );
        assert!(
            response
                .rendered_markdown
                .as_ref()
                .unwrap()
                .contains("$100.00")
        );
        assert!(
            response
                .rendered_markdown
                .as_ref()
                .unwrap()
                .contains("AED 21")
        );
        assert!(
            response
                .warnings
                .iter()
                .any(|warning| warning.code == "multi_currency")
        );
        assert!(
            response
                .cards
                .iter()
                .any(|card| card.label == "Rows" && card.value == "2")
        );
        assert!(
            response
                .cards
                .iter()
                .any(|card| card.label == "Outstanding (USD)" && card.value == "$100.00")
        );
        assert!(
            response
                .cards
                .iter()
                .any(|card| card.label == "Outstanding (AED)" && card.value == "AED 21")
        );
        assert!(!response.cards.iter().any(|card| card.value.contains("120")));
    }

    #[test]
    fn public_business_columns_are_never_hidden_even_without_pii_access() {
        let field = QueryOutputField {
            name: "amount".into(),
            kind: "decimal".into(),
            sensitivity: Sensitivity::PublicBusiness,
        };

        assert!(!is_hidden(&field, false));
        assert!(!is_hidden(&field, true));
    }

    fn allowed_policy() -> PolicyDecision {
        PolicyDecision {
            status: PolicyDecisionStatus::Allowed,
            reason: None,
            office_ids: vec![1],
            can_view_pii: true,
        }
    }

    fn charge_plan() -> ExecutionPlan {
        ExecutionPlan {
            query_id: "savings.pending_charges_clients".into(),
            capability: "savings_pending_charges_clients".into(),
            ..plan()
        }
    }

    fn charge_catalog() -> KnowledgeCatalog {
        let mut catalog = catalog();
        catalog.queries[0].id = "savings.pending_charges_clients".into();
        catalog.queries[0].output_fields = vec![
            QueryOutputField {
                name: "currency_code".into(),
                kind: "string".into(),
                sensitivity: Sensitivity::PublicBusiness,
            },
            QueryOutputField {
                name: "currency_digits".into(),
                kind: "integer".into(),
                sensitivity: Sensitivity::PublicBusiness,
            },
            QueryOutputField {
                name: "currency_display_symbol".into(),
                kind: "string".into(),
                sensitivity: Sensitivity::PublicBusiness,
            },
            QueryOutputField {
                name: "amount_outstanding".into(),
                kind: "decimal".into(),
                sensitivity: Sensitivity::PublicBusiness,
            },
        ];
        catalog
    }

    fn intent() -> AssistantIntent {
        AssistantIntent {
            intent: AssistantIntentKind::DataLookup,
            domain: AssistantDomain::Client,
            request_shape: Default::default(),
            language: AssistantLanguage::En,
            canonical_query_en: String::new(),
            entities: Vec::new(),
            constraints: AssistantConstraints::default(),
            context_reference: ContextReference::None,
            source: None,
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
            dataset_selection: None,
            output_mode: "list".into(),
            params: json!({}),
            retrieval_plan: RetrievalPlan::default(),
            evidence_evaluation: EvidenceEvaluation::default(),
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
                        sensitivity: Sensitivity::PublicBusiness,
                    },
                    QueryOutputField {
                        name: "national_id".into(),
                        kind: "string".into(),
                        sensitivity: Sensitivity::Pii,
                    },
                ],
                timeout_ms: None,
            }],
            policies: Vec::new(),
            responses: Vec::new(),
            parameter_inputs: Vec::new(),
            classification: Default::default(),
            datasets: Vec::new(),
        }
    }
}
