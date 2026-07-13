use anyhow::{Result, bail};
use app_core::auth::model::ClientContext;
use serde_json::{Value, json};

use crate::{
    assistant::{AssistantEntityType, AssistantIntent, Quantity},
    chat::planner::{
        AnswerPlan, EvidenceEvaluation, ExecutionPlan, ExecutionPlanType, RetrievalPlan,
        evaluate_policy,
    },
    knowledge::model::{KnowledgeCatalog, QueryKnowledge},
};

pub fn plan_selected_capability(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    intent: &AssistantIntent,
) -> Result<ExecutionPlan> {
    let capability = catalog
        .capabilities
        .iter()
        .find(|item| item.id == capability_id && item.status == "approved_mvp")
        .ok_or_else(|| anyhow::anyhow!("selected capability is not executable"))?;
    let query = catalog
        .queries
        .iter()
        .find(|item| item.id == capability.query_id)
        .ok_or_else(|| anyhow::anyhow!("selected capability has no approved query"))?;
    let params = params_from_intent(query, intent)?;

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

pub fn guard_selected_capability(
    client: &ClientContext,
    catalog: &KnowledgeCatalog,
    plan: &ExecutionPlan,
) -> crate::chat::planner::PolicyDecision {
    evaluate_policy(client, Some(plan), catalog)
}

fn params_from_intent(query: &QueryKnowledge, intent: &AssistantIntent) -> Result<Value> {
    let mut params = serde_json::Map::new();
    let person_name = entity_value(intent, AssistantEntityType::PersonName);
    let office = entity_value(intent, AssistantEntityType::Office);
    let product = entity_value(intent, AssistantEntityType::Product);
    let currency = intent
        .constraints
        .currency_code
        .as_deref()
        .or_else(|| entity_value(intent, AssistantEntityType::Currency));

    for parameter in &query.parameters {
        if parameter.source.as_deref() == Some("authorized_scope") {
            continue;
        }
        let value = match parameter.name.as_str() {
            "from_date" => intent
                .constraints
                .from_date
                .as_ref()
                .map(|value| json!(value)),
            "to_date" => intent
                .constraints
                .to_date
                .as_ref()
                .map(|value| json!(value)),
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
            "limit" | "top_n" => Some(json!(quantity_limit(intent).unwrap_or(20))),
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

fn entity_value(intent: &AssistantIntent, entity_type: AssistantEntityType) -> Option<&str> {
    intent
        .entities
        .iter()
        .find(|entity| entity.entity_type == entity_type)
        .map(|entity| entity.value.trim())
        .filter(|value| !value.is_empty())
}

fn quantity_limit(intent: &AssistantIntent) -> Option<i64> {
    match intent.constraints.quantity.as_ref()? {
        Quantity::Limit { value } | Quantity::TopN { value } => Some(*value),
        Quantity::Default => Some(20),
        Quantity::All => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assistant::{
            AssistantConstraints, AssistantDomain, AssistantEntity, AssistantIntentKind,
            AssistantLanguage, ContextReference, Quantity,
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
                language: AssistantLanguage::En,
                entities: vec![AssistantEntity {
                    entity_type: AssistantEntityType::PersonName,
                    value: "Tony".into(),
                }],
                constraints: Default::default(),
                context_reference: ContextReference::None,
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
                language: AssistantLanguage::En,
                entities: vec![],
                constraints: Default::default(),
                context_reference: ContextReference::None,
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
        let params = params_from_intent(
            &query,
            &AssistantIntent {
                intent: AssistantIntentKind::ReportRequest,
                domain: AssistantDomain::Savings,
                language: AssistantLanguage::En,
                entities: Vec::new(),
                constraints: AssistantConstraints {
                    from_date: Some("2026-01-01".into()),
                    to_date: Some("2026-01-31".into()),
                    currency_code: Some("USD".into()),
                    product_ids: Some(vec![7]),
                    quantity: Some(Quantity::TopN { value: 5 }),
                },
                context_reference: ContextReference::None,
                confidence: 0.9,
                reason: "test".into(),
            },
        )
        .unwrap();

        assert_eq!(params["from_date"], "2026-01-01");
        assert_eq!(params["to_date"], "2026-01-31");
        assert_eq!(params["currency_code"], "USD");
        assert_eq!(params["product_id"], 7);
        assert_eq!(params["limit"], 5);
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
