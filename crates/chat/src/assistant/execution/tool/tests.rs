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
    let extraction = extract_message_facts("show top 10 clients with the most savings accounts");
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
    let extraction = extract_message_facts("show top 10 clients with the most savings accounts");
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
