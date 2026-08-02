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
            canonical_query_en: String::new(),
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
            canonical_query_en: String::new(),
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
        timeout_ms: None,
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
            canonical_query_en: String::new(),
            entities: Vec::new(),
            constraints: AssistantConstraints {
                from_date: Some("2026-01-01".into()),
                to_date: Some("2026-01-31".into()),
                currency_code: Some("USD".into()),
                product_ids: Some(vec![7]),
                office_ids: None,
                metric: None,
                transaction_amount: None,
                quantity: Some(Quantity::TopN { value: 5 }),
            },
            context_reference: ContextReference::None,
            source: None,
            confidence: 0.9,
            reason: "test".into(),
        },
        Some(&extraction),
        &[],
        None,
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
        timeout_ms: None,
    };
    let extraction = extract_message_facts("show top 10 clients");
    let params = params_from_verified(
        &query,
        &intent_with_quantity(None),
        Some(&extraction),
        &[],
        None,
    )
    .unwrap();

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
        timeout_ms: None,
    };
    let params = params_from_verified(
        &query,
        &intent_with_quantity(Some(Quantity::TopN { value: 20 })),
        None,
        &[],
        None,
    )
    .unwrap();

    // The hallucinated 20 is still discarded; the run falls back to the default
    // rather than bouncing a clarification back at the user.
    assert_eq!(params["limit"], super::parameters::DEFAULT_REPORT_LIMIT);
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
        timeout_ms: None,
    };
    let mut intent = intent_with_quantity(None);
    intent.constraints.currency_code = Some("USD".into());

    let params = params_from_verified(&query, &intent, None, &[], None).unwrap();

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
        None,
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
        None,
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
        timeout_ms: None,
    };
    let mut intent = intent_with_quantity(None);
    intent.entities.push(AssistantEntity {
        entity_type: AssistantEntityType::PersonName,
        value: "Tony".into(),
        canonical: None,
        confidence: None,
    });
    let error = params_from_verified(&query, &intent, None, &[], None).unwrap_err();

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
        timeout_ms: None,
    };
    let extraction = extract_message_facts("find client named Tony");
    let params = params_from_verified(
        &query,
        &intent_with_quantity(None),
        Some(&extraction),
        &[],
        None,
    )
    .unwrap();

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
        canonical_query_en: String::new(),
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

#[test]
fn defaults_business_today_when_policy_declares_it() {
    use crate::knowledge::catalog::parameter_policy::{
        DefaultExpr, EvaluationContext, ParameterPolicy, ParameterType,
    };
    let query = QueryKnowledge {
        id: "test.query".into(),
        database: "fineract".into(),
        sql_file: "test.sql".into(),
        data_areas: Vec::new(),
        tables: Vec::new(),
        metrics: Vec::new(),
        parameters: vec![parameter("from_date", true)],
        output_fields: Vec::new(),
        timeout_ms: None,
    };
    let policies = vec![ParameterPolicy {
        name: "from_date".into(),
        kind: ParameterType::Date,
        required: false,
        default: Some(DefaultExpr::BusinessToday),
        fill_when_missing: true,
        user_may_override: true,
        hard_cap: None,
    }];
    let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 24).unwrap();
    let ctx = EvaluationContext {
        business_today: today,
        wall_today: today,
        authorized_office_ids: Vec::new(),
    };

    let params = params_from_verified(
        &query,
        &intent_with_quantity(None),
        None,
        &policies,
        Some(&ctx),
    )
    .unwrap();

    assert_eq!(params["from_date"], "2026-07-24");
}

#[test]
fn unbounded_limit_is_clamped_to_hard_cap() {
    use crate::knowledge::catalog::parameter_policy::{
        DefaultExpr, EvaluationContext, ParameterPolicy, ParameterType,
    };
    let query = QueryKnowledge {
        id: "test.query".into(),
        database: "fineract".into(),
        sql_file: "test.sql".into(),
        data_areas: Vec::new(),
        tables: Vec::new(),
        metrics: Vec::new(),
        parameters: vec![parameter("limit", true)],
        output_fields: Vec::new(),
        timeout_ms: None,
    };
    let policies = vec![ParameterPolicy {
        name: "limit".into(),
        kind: ParameterType::Integer,
        required: false,
        default: Some(DefaultExpr::Unbounded),
        fill_when_missing: true,
        user_may_override: true,
        hard_cap: Some(100),
    }];
    let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 24).unwrap();
    let ctx = EvaluationContext {
        business_today: today,
        wall_today: today,
        authorized_office_ids: Vec::new(),
    };

    let params = params_from_verified(
        &query,
        &intent_with_quantity(None),
        None,
        &policies,
        Some(&ctx),
    )
    .unwrap();

    assert_eq!(params["limit"], 100);
}

#[test]
fn hard_cap_clamps_over_cap_and_preserves_within_cap_values() {
    let policies = [
        crate::knowledge::catalog::parameter_policy::ParameterPolicy {
            name: "limit".into(),
            kind: crate::knowledge::catalog::parameter_policy::ParameterType::Integer,
            required: false,
            default: None,
            fill_when_missing: true,
            user_may_override: true,
            hard_cap: Some(100),
        },
    ];
    let mut over = serde_json::Map::from_iter([("limit".into(), serde_json::json!(5_000))]);
    super::parameters::clamp_hard_caps(&mut over, &policies);
    assert_eq!(over["limit"], 100);

    let mut within = serde_json::Map::from_iter([("limit".into(), serde_json::json!(25))]);
    super::parameters::clamp_hard_caps(&mut within, &policies);
    assert_eq!(within["limit"], 25);
}

#[test]
fn limit_without_hard_cap_is_not_clamped() {
    let mut params = serde_json::Map::from_iter([("limit".into(), serde_json::json!(5_000))]);
    let policies = [
        crate::knowledge::catalog::parameter_policy::ParameterPolicy {
            name: "limit".into(),
            kind: crate::knowledge::catalog::parameter_policy::ParameterType::Integer,
            required: false,
            default: None,
            fill_when_missing: true,
            user_may_override: true,
            hard_cap: None,
        },
    ];

    super::parameters::clamp_hard_caps(&mut params, &policies);

    assert_eq!(params["limit"], 5_000);
}

#[test]
fn defaults_authorized_scope_when_policy_declares_it() {
    use crate::knowledge::catalog::parameter_policy::{
        DefaultExpr, EvaluationContext, ParameterPolicy, ParameterType,
    };
    let query = QueryKnowledge {
        id: "test.query".into(),
        database: "fineract".into(),
        sql_file: "test.sql".into(),
        data_areas: Vec::new(),
        tables: Vec::new(),
        metrics: Vec::new(),
        parameters: vec![parameter("office_ids", true)],
        output_fields: Vec::new(),
        timeout_ms: None,
    };
    let policies = vec![ParameterPolicy {
        name: "office_ids".into(),
        kind: ParameterType::IntegerArray,
        required: false,
        default: Some(DefaultExpr::AuthorizedScope),
        fill_when_missing: true,
        user_may_override: false,
        hard_cap: None,
    }];
    let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 24).unwrap();
    let ctx = EvaluationContext {
        business_today: today,
        wall_today: today,
        authorized_office_ids: vec![1, 2],
    };

    let params = params_from_verified(
        &query,
        &intent_with_quantity(None),
        None,
        &policies,
        Some(&ctx),
    )
    .unwrap();

    assert_eq!(params["office_ids"], serde_json::json!([1, 2]));
}

#[test]
fn still_bails_when_no_policy_and_no_default() {
    let query = QueryKnowledge {
        id: "test.query".into(),
        database: "fineract".into(),
        sql_file: "test.sql".into(),
        data_areas: Vec::new(),
        tables: Vec::new(),
        metrics: Vec::new(),
        parameters: vec![parameter("from_date", true)],
        output_fields: Vec::new(),
        timeout_ms: None,
    };
    let error =
        params_from_verified(&query, &intent_with_quantity(None), None, &[], None).unwrap_err();

    assert!(error.to_string().contains("missing parameter from_date"));
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
