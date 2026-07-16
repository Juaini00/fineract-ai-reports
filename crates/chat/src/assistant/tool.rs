use anyhow::{Result, bail};
use app_core::auth::model::PrincipalContext;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    assistant::{
        AssistantEntityType, AssistantIntent, ConstraintField, DeterministicExtraction,
        EffectiveConstraints, LimitMode, ListPatch, PlannerInputSnapshot, Quantity, TypedFactValue,
    },
    chat::planner::{
        AnswerPlan, EvidenceEvaluation, ExecutionPlan, ExecutionPlanType, RetrievalPlan,
        evaluate_policy,
    },
    knowledge::model::{KnowledgeCatalog, QueryKnowledge},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolRequest {
    pub tool_name: String,
    pub capability_id: Option<String>,
    pub query_id: Option<String>,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolResult {
    pub tool_name: String,
    pub ok: bool,
    #[serde(default)]
    pub rows: Vec<Value>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error: Option<ToolValidationError>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

pub const APPROVED_SQL_TOOL: &str = "approved_catalog_sql";

pub fn tool_request_from_plan(plan: &ExecutionPlan, evidence_refs: Vec<String>) -> ToolRequest {
    ToolRequest {
        tool_name: APPROVED_SQL_TOOL.into(),
        capability_id: Some(plan.capability.clone()),
        query_id: Some(plan.query_id.clone()),
        params: plan.params.clone(),
        evidence_refs,
    }
}

pub fn tool_result_from_execution(request: &ToolRequest, execution_result: Value) -> ToolResult {
    ToolResult {
        tool_name: request.tool_name.clone(),
        ok: true,
        rows: execution_result
            .get("rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        summary: execution_result
            .get("row_count")
            .and_then(Value::as_u64)
            .map(|count| format!("{count} row(s) returned")),
        error: None,
        evidence_refs: request.evidence_refs.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolValidationError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub field: Option<String>,
}

pub fn plan_selected_capability(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    intent: &AssistantIntent,
) -> Result<ExecutionPlan> {
    let legacy_extraction = DeterministicExtraction {
        entities: intent.entities.clone(),
        ..Default::default()
    };
    plan_selected_capability_verified(catalog, capability_id, intent, Some(&legacy_extraction))
}

pub fn plan_selected_capability_verified(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    intent: &AssistantIntent,
    deterministic_extraction: Option<&DeterministicExtraction>,
) -> Result<ExecutionPlan> {
    if let Some(error) = deterministic_extraction.and_then(|value| value.temporal_error.as_ref()) {
        bail!("{}: {}", error.code, error.message);
    }
    let capability = catalog
        .capabilities
        .iter()
        .find(|item| item.id == capability_id && item.status == "approved_mvp")
        .ok_or_else(|| anyhow::anyhow!("selected capability is not executable"))?;
    verify_capability_metric(capability.metrics.as_slice(), deterministic_extraction)?;
    let query = catalog
        .queries
        .iter()
        .find(|item| item.id == capability.query_id)
        .ok_or_else(|| anyhow::anyhow!("selected capability has no approved query"))?;
    let params = params_from_verified(query, intent, deterministic_extraction)?;

    Ok(ExecutionPlan {
        plan_type: ExecutionPlanType::Atomic,
        domain: capability.domain.clone(),
        capability: capability.id.clone(),
        query_id: query.id.clone(),
        output_mode: capability.output_mode.clone(),
        params,
        retrieval_plan: RetrievalPlan {
            vector_query: intent.reason.clone(),
            keyword_query: intent
                .entities
                .iter()
                .map(|entity| entity.value.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            graph_query: format!("{} -> {}", capability.id, query.id),
            metadata_filter: [("capability".into(), capability.id.clone())].into(),
        },
        evidence_evaluation: EvidenceEvaluation {
            enough: true,
            source_count: 1,
            source_types: vec!["capability".into()],
            reason: None,
        },
        answer_plan: AnswerPlan {
            sections: vec!["Result".into(), "Scope".into(), "Evidence".into()],
        },
        requires_policy_check: true,
    })
}

pub fn normalize_effective_parameters(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    effective: &EffectiveConstraints,
) -> Result<Value> {
    let capability = executable_capability(catalog, capability_id)?;
    let query = catalog
        .queries
        .iter()
        .find(|item| item.id == capability.query_id)
        .ok_or_else(|| anyhow::anyhow!("selected capability has no approved query"))?;
    if let Some(TypedFactValue::Metric(metric)) = effective.values.get(&ConstraintField::Metric) {
        let trusted = normalize_metric(metric);
        if !capability
            .metrics
            .iter()
            .any(|item| normalize_metric(item) == trusted)
        {
            bail!("selected capability does not match requested metric {metric}");
        }
    }
    validate_effective_date_range(effective)?;
    let mut params = serde_json::Map::new();
    for parameter in &query.parameters {
        if parameter.source.as_deref() == Some("authorized_scope") {
            continue;
        }
        let value = effective_parameter(effective, &parameter.name);
        if let Some(value) = value {
            params.insert(parameter.name.clone(), value);
        } else if parameter.required {
            bail!("missing parameter {}", parameter.name);
        }
    }
    Ok(Value::Object(params))
}

fn validate_effective_date_range(effective: &EffectiveConstraints) -> Result<()> {
    let from = effective.values.get(&ConstraintField::FromDate);
    let to = effective.values.get(&ConstraintField::ToDate);
    if let (Some(TypedFactValue::Date(from)), Some(TypedFactValue::Date(to))) = (from, to) {
        let from = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("temporal_invalid_date: invalid from_date"))?;
        let to = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("temporal_invalid_date: invalid to_date"))?;
        if from > to {
            bail!("temporal_range_reversed: start date is after end date");
        }
    }
    Ok(())
}

pub fn plan_from_snapshot(
    catalog: &KnowledgeCatalog,
    snapshot: &PlannerInputSnapshot,
) -> Result<ExecutionPlan> {
    let capability = executable_capability(catalog, &snapshot.selected_capability_id)?;
    let query = catalog
        .queries
        .iter()
        .find(|item| item.id == capability.query_id)
        .ok_or_else(|| anyhow::anyhow!("selected capability has no approved query"))?;
    validate_snapshot_parameters(query, &snapshot.normalized_parameters)?;
    Ok(ExecutionPlan {
        plan_type: ExecutionPlanType::Atomic,
        domain: capability.domain.clone(),
        capability: capability.id.clone(),
        query_id: query.id.clone(),
        output_mode: capability.output_mode.clone(),
        params: snapshot.normalized_parameters.clone(),
        retrieval_plan: RetrievalPlan::default(),
        evidence_evaluation: EvidenceEvaluation {
            enough: true,
            source_count: 1,
            source_types: vec!["planner_input_snapshot".into()],
            reason: None,
        },
        answer_plan: AnswerPlan {
            sections: vec!["Result".into(), "Scope".into(), "Evidence".into()],
        },
        requires_policy_check: true,
    })
}

fn executable_capability<'a>(
    catalog: &'a KnowledgeCatalog,
    capability_id: &str,
) -> Result<&'a crate::knowledge::model::CapabilityKnowledge> {
    catalog
        .capabilities
        .iter()
        .find(|item| item.id == capability_id && item.status == "approved_mvp")
        .ok_or_else(|| anyhow::anyhow!("selected capability is not executable"))
}

fn effective_parameter(effective: &EffectiveConstraints, name: &str) -> Option<Value> {
    let value = |field| effective.values.get(&field);
    match name {
        "from_date" => typed_string(value(ConstraintField::FromDate)).map(Value::String),
        "to_date" => typed_string(value(ConstraintField::ToDate)).map(Value::String),
        "currency_code" => typed_string(value(ConstraintField::CurrencyCode)).map(Value::String),
        "product_ids" => typed_ids(value(ConstraintField::ProductIds)).map(|ids| json!(ids)),
        "product_id" => typed_ids(value(ConstraintField::ProductIds))
            .and_then(|ids| ids.first().copied())
            .map(|id| json!(id))
            .or_else(|| {
                typed_string(value(ConstraintField::Product))
                    .and_then(|v| v.parse().ok())
                    .map(|id: i64| json!(id))
            }),
        "search" | "name" => [
            ConstraintField::PersonName,
            ConstraintField::Office,
            ConstraintField::Product,
        ]
        .into_iter()
        .find_map(|field| typed_string(value(field)))
        .map(Value::String),
        "office" | "office_name" => typed_string(value(ConstraintField::Office)).map(Value::String),
        "limit" | "top_n" => match (
            value(ConstraintField::LimitMode),
            value(ConstraintField::LimitValue),
        ) {
            (
                Some(TypedFactValue::LimitMode(LimitMode::Limit | LimitMode::TopN)),
                Some(TypedFactValue::Integer(limit)),
            ) => Some(json!(limit)),
            _ => None,
        },
        _ => None,
    }
}

fn typed_string(value: Option<&TypedFactValue>) -> Option<String> {
    match value? {
        TypedFactValue::Date(v)
        | TypedFactValue::CurrencyCode(v)
        | TypedFactValue::Metric(v)
        | TypedFactValue::PersonName(v)
        | TypedFactValue::Office(v)
        | TypedFactValue::Product(v) => Some(v.clone()),
        _ => None,
    }
}

fn typed_ids(value: Option<&TypedFactValue>) -> Option<Vec<i64>> {
    match value? {
        TypedFactValue::IdList(ListPatch::Replace(ids)) => Some(ids.clone()),
        _ => None,
    }
}

fn validate_snapshot_parameters(query: &QueryKnowledge, params: &Value) -> Result<()> {
    let params = params
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("snapshot parameters must be an object"))?;
    for name in params.keys() {
        if !query.parameters.iter().any(|parameter| {
            parameter.name == *name && parameter.source.as_deref() != Some("authorized_scope")
        }) {
            bail!("snapshot contains unexpected parameter {name}");
        }
    }
    for parameter in &query.parameters {
        if parameter.source.as_deref() == Some("authorized_scope") {
            continue;
        }
        let Some(value) = params.get(&parameter.name) else {
            if parameter.required {
                bail!("snapshot missing parameter {}", parameter.name);
            }
            continue;
        };
        let valid = match parameter.kind.as_str() {
            "date" | "string" => value.is_string(),
            "integer" => value.as_i64().is_some(),
            "array_bigint" => value
                .as_array()
                .is_some_and(|items| items.iter().all(|item| item.as_i64().is_some())),
            _ => false,
        };
        if !valid {
            bail!("snapshot parameter {} has invalid type", parameter.name);
        }
    }
    Ok(())
}

pub fn guard_selected_capability(
    client: &PrincipalContext,
    catalog: &KnowledgeCatalog,
    plan: &ExecutionPlan,
) -> crate::chat::planner::PolicyDecision {
    evaluate_policy(client, Some(plan), catalog)
}

fn params_from_verified(
    query: &QueryKnowledge,
    intent: &AssistantIntent,
    deterministic_extraction: Option<&DeterministicExtraction>,
) -> Result<Value> {
    let mut params = serde_json::Map::new();
    let person_name = deterministic_extraction.and_then(|extraction| {
        entity_value_from(&extraction.entities, AssistantEntityType::PersonName)
    });
    let office = entity_value(intent, AssistantEntityType::Office);
    let product = entity_value(intent, AssistantEntityType::Product);
    let trusted = deterministic_extraction.map(|extraction| &extraction.constraints);
    let currency = trusted.and_then(|constraints| constraints.currency_code.as_deref());

    for parameter in &query.parameters {
        if parameter.source.as_deref() == Some("authorized_scope") {
            continue;
        }
        let value = match parameter.name.as_str() {
            "from_date" => trusted
                .and_then(|constraints| constraints.from_date.as_ref().map(|value| json!(value))),
            "to_date" => trusted
                .and_then(|constraints| constraints.to_date.as_ref().map(|value| json!(value))),
            "currency_code" => currency.map(|value| json!(value)),
            "product_ids" => intent
                .constraints
                .product_ids
                .as_ref()
                .map(|value| json!(value)),
            "product_id" => intent
                .constraints
                .product_ids
                .as_ref()
                .and_then(|ids| ids.first())
                .map(|value| json!(value))
                .or_else(|| {
                    product
                        .and_then(|value| value.parse::<i64>().ok())
                        .map(|value| json!(value))
                }),
            "search" | "name" => person_name.or(office).or(product).map(|value| json!(value)),
            "office" | "office_name" => office.map(|value| json!(value)),
            "limit" | "top_n" => trusted.and_then(quantity_limit).map(|value| json!(value)),
            _ => None,
        };
        if let Some(value) = value {
            params.insert(parameter.name.clone(), value);
        } else if parameter.required {
            bail!("missing parameter {}", parameter.name);
        }
    }

    Ok(Value::Object(params))
}

fn verify_capability_metric(
    capability_metrics: &[String],
    deterministic_extraction: Option<&DeterministicExtraction>,
) -> Result<()> {
    let Some(metric) =
        deterministic_extraction.and_then(|extraction| extraction.constraints.metric.as_deref())
    else {
        return Ok(());
    };
    let trusted = normalize_metric(metric);
    if capability_metrics
        .iter()
        .any(|metric| normalize_metric(metric) == trusted)
    {
        Ok(())
    } else {
        bail!("selected capability does not match requested metric {metric}")
    }
}

fn normalize_metric(metric: &str) -> String {
    match metric.replace('_', ".").as_str() {
        "savings.account.count" => "savings.account_count".into(),
        "savings.balance" => "savings.balance_total".into(),
        "deposit.volume" | "savings.deposit.volume" => "savings.deposit_total".into(),
        other => other.into(),
    }
}

fn entity_value(intent: &AssistantIntent, entity_type: AssistantEntityType) -> Option<&str> {
    entity_value_from(&intent.entities, entity_type)
}

fn entity_value_from(
    entities: &[crate::assistant::AssistantEntity],
    entity_type: AssistantEntityType,
) -> Option<&str> {
    entities
        .iter()
        .find(|entity| entity.entity_type == entity_type)
        .map(|entity| entity.value.trim())
        .filter(|value| !value.is_empty())
}

fn quantity_limit(constraints: &crate::assistant::AssistantConstraints) -> Option<i64> {
    quantity_limit_from(constraints.quantity.as_ref())
}

fn quantity_limit_from(quantity: Option<&Quantity>) -> Option<i64> {
    match quantity? {
        Quantity::Limit { value } | Quantity::TopN { value } => Some(*value),
        Quantity::Default => None,
        Quantity::All => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assistant::{
            AssistantConstraints, AssistantDomain, AssistantEntity, AssistantIntentKind,
            AssistantLanguage, ContextReference, Quantity, extract_message_facts,
        },
        knowledge::catalog::{loader::KnowledgeLoader, validator::KnowledgeValidator},
    };

    #[test]
    fn extracts_tony_for_client_name_lookup_plan() {
        let catalog = catalog();
        let plan = plan_selected_capability(
            &catalog,
            "client_name_lookup",
            &AssistantIntent {
                intent: AssistantIntentKind::DataLookup,
                domain: AssistantDomain::Client,
                request_shape: Default::default(),
                language: AssistantLanguage::En,
                entities: vec![AssistantEntity {
                    entity_type: AssistantEntityType::PersonName,
                    value: "Tony".into(),
                    canonical: None,
                    confidence: None,
                }],
                constraints: Default::default(),
                context_reference: ContextReference::None,
                source: None,
                confidence: 0.9,
                reason: "test".into(),
            },
        )
        .unwrap();

        assert_eq!(plan.query_id, "client.name_lookup");
        assert_eq!(plan.params["search"], "Tony");
    }

    #[test]
    fn missing_person_name_requires_clarification() {
        let catalog = catalog();
        let error = plan_selected_capability(
            &catalog,
            "client_name_lookup",
            &AssistantIntent {
                intent: AssistantIntentKind::DataLookup,
                domain: AssistantDomain::Client,
                request_shape: Default::default(),
                language: AssistantLanguage::En,
                entities: vec![],
                constraints: Default::default(),
                context_reference: ContextReference::None,
                source: None,
                confidence: 0.9,
                reason: "test".into(),
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("missing parameter search"));
    }

    #[test]
    fn extracts_dates_currency_products_and_limit() {
        let query = QueryKnowledge {
            id: "test.query".into(),
            database: "fineract".into(),
            sql_file: "test.sql".into(),
            data_areas: Vec::new(),
            tables: Vec::new(),
            metrics: Vec::new(),
            parameters: vec![
                parameter("from_date", true),
                parameter("to_date", true),
                parameter("currency_code", false),
                parameter("product_id", false),
                parameter("limit", false),
            ],
            output_fields: Vec::new(),
        };
        let extraction =
            extract_message_facts("show top 5 savings in USD from 2026-01-01 to 2026-01-31");
        let params = params_from_verified(
            &query,
            &AssistantIntent {
                intent: AssistantIntentKind::ReportRequest,
                domain: AssistantDomain::Savings,
                request_shape: Default::default(),
                language: AssistantLanguage::En,
                entities: Vec::new(),
                constraints: AssistantConstraints {
                    from_date: Some("2026-01-01".into()),
                    to_date: Some("2026-01-31".into()),
                    currency_code: Some("USD".into()),
                    product_ids: Some(vec![7]),
                    office_ids: None,
                    metric: None,
                    quantity: Some(Quantity::TopN { value: 5 }),
                },
                context_reference: ContextReference::None,
                source: None,
                confidence: 0.9,
                reason: "test".into(),
            },
            Some(&extraction),
        )
        .unwrap();

        assert_eq!(params["from_date"], "2026-01-01");
        assert_eq!(params["to_date"], "2026-01-31");
        assert_eq!(params["currency_code"], "USD");
        assert_eq!(params["product_id"], 7);
        assert_eq!(params["limit"], 5);
    }

    #[test]
    fn verified_quantity_overrides_missing_llm_quantity() {
        let query = QueryKnowledge {
            id: "test.query".into(),
            database: "fineract".into(),
            sql_file: "test.sql".into(),
            data_areas: Vec::new(),
            tables: Vec::new(),
            metrics: Vec::new(),
            parameters: vec![parameter("limit", true)],
            output_fields: Vec::new(),
        };
        let extraction = extract_message_facts("show top 10 clients");
        let params =
            params_from_verified(&query, &intent_with_quantity(None), Some(&extraction)).unwrap();

        assert_eq!(params["limit"], 10);
    }

    #[test]
    fn hallucinated_required_quantity_is_rejected_without_verified_extraction() {
        let query = QueryKnowledge {
            id: "test.query".into(),
            database: "fineract".into(),
            sql_file: "test.sql".into(),
            data_areas: Vec::new(),
            tables: Vec::new(),
            metrics: Vec::new(),
            parameters: vec![parameter("limit", true)],
            output_fields: Vec::new(),
        };
        let error = params_from_verified(
            &query,
            &intent_with_quantity(Some(Quantity::TopN { value: 20 })),
            None,
        )
        .unwrap_err();

        assert!(error.to_string().contains("missing parameter limit"));
    }

    #[test]
    fn hallucinated_optional_currency_is_omitted_without_verified_extraction() {
        let query = QueryKnowledge {
            id: "test.query".into(),
            database: "fineract".into(),
            sql_file: "test.sql".into(),
            data_areas: Vec::new(),
            tables: Vec::new(),
            metrics: Vec::new(),
            parameters: vec![parameter("currency_code", false)],
            output_fields: Vec::new(),
        };
        let mut intent = intent_with_quantity(None);
        intent.constraints.currency_code = Some("USD".into());

        let params = params_from_verified(&query, &intent, None).unwrap();

        assert!(params.get("currency_code").is_none());
    }

    #[test]
    fn metric_mismatch_rejected() {
        let catalog = catalog();
        let extraction =
            extract_message_facts("show top 10 clients with the most savings accounts");
        let error = plan_selected_capability_verified(
            &catalog,
            "client_top_n_by_deposit_volume",
            &intent_with_quantity(None),
            Some(&extraction),
        )
        .unwrap_err();

        assert!(error.to_string().contains("requested metric"));
    }

    #[test]
    fn metric_match_accepted() {
        let catalog = catalog();
        let extraction =
            extract_message_facts("show top 10 clients with the most savings accounts");
        let plan = plan_selected_capability_verified(
            &catalog,
            "client_top_n_by_savings_account_count",
            &intent_with_quantity(None),
            Some(&extraction),
        )
        .unwrap();

        assert_eq!(plan.params["limit"], 10);
    }

    #[test]
    fn hallucinated_required_search_rejected_without_trusted_entity() {
        let query = QueryKnowledge {
            id: "test.query".into(),
            database: "fineract".into(),
            sql_file: "test.sql".into(),
            data_areas: Vec::new(),
            tables: Vec::new(),
            metrics: Vec::new(),
            parameters: vec![parameter("search", true)],
            output_fields: Vec::new(),
        };
        let mut intent = intent_with_quantity(None);
        intent.entities.push(AssistantEntity {
            entity_type: AssistantEntityType::PersonName,
            value: "Tony".into(),
            canonical: None,
            confidence: None,
        });
        let error = params_from_verified(&query, &intent, None).unwrap_err();

        assert!(error.to_string().contains("missing parameter search"));
    }

    #[test]
    fn trusted_named_tony_fills_search() {
        let query = QueryKnowledge {
            id: "test.query".into(),
            database: "fineract".into(),
            sql_file: "test.sql".into(),
            data_areas: Vec::new(),
            tables: Vec::new(),
            metrics: Vec::new(),
            parameters: vec![parameter("search", true)],
            output_fields: Vec::new(),
        };
        let extraction = extract_message_facts("find client named Tony");
        let params =
            params_from_verified(&query, &intent_with_quantity(None), Some(&extraction)).unwrap();

        assert_eq!(params["search"], "Tony");
    }

    #[test]
    fn canonical_snapshot_rejects_malformed_parameters() {
        let catalog = catalog();
        let snapshot = PlannerInputSnapshot {
            id: uuid::Uuid::new_v4(),
            job_id: uuid::Uuid::new_v4(),
            revision: 0,
            original_intent_id: uuid::Uuid::new_v4(),
            effective_constraints_id: uuid::Uuid::new_v4(),
            capability_catalog_version: uuid::Uuid::new_v4(),
            principal_projection: crate::assistant::PrincipalProjection {
                user_id: uuid::Uuid::new_v4(),
                role: "admin".into(),
                capability_ids: vec![],
                office_ids: vec![],
                can_view_pii: false,
                legacy_api_key_id: None,
            },
            reference_instant: chrono::Utc::now(),
            timezone: "UTC".into(),
            selected_capability_id: "savings_deposit_total".into(),
            normalized_parameters: json!([]),
            created_at: chrono::Utc::now(),
        };
        assert!(plan_from_snapshot(&catalog, &snapshot).is_err());
    }

    fn intent_with_quantity(quantity: Option<Quantity>) -> AssistantIntent {
        AssistantIntent {
            intent: AssistantIntentKind::ReportRequest,
            domain: AssistantDomain::Client,
            request_shape: Default::default(),
            language: AssistantLanguage::En,
            entities: Vec::new(),
            constraints: AssistantConstraints {
                quantity,
                ..Default::default()
            },
            context_reference: ContextReference::None,
            source: None,
            confidence: 0.9,
            reason: "test".into(),
        }
    }

    fn parameter(name: &str, required: bool) -> crate::knowledge::model::QueryParameter {
        crate::knowledge::model::QueryParameter {
            name: name.into(),
            kind: "text".into(),
            required,
            source: None,
        }
    }

    fn catalog() -> KnowledgeCatalog {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalog = KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
            .load()
            .unwrap();
        KnowledgeValidator::validate(&catalog).unwrap();
        catalog
    }
}
