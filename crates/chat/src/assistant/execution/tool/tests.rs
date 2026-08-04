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
            parameter("product_ids", false),
            parameter("limit", false),
        ],
        output_fields: Vec::new(),
        timeout_ms: None,
    };
    let extraction =
        extract_message_facts("show top 5 savings in USD from 2026-01-01 to 2026-01-31");
    let params = params_from_verified(
        &catalog(),
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
        None,
    )
    .unwrap();

    assert_eq!(params["from_date"], "2026-01-01");
    assert_eq!(params["to_date"], "2026-01-31");
    assert_eq!(params["currency_code"], "USD");
    assert_eq!(params["product_ids"], serde_json::json!([7]));
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
        &catalog(),
        &query,
        &intent_with_quantity(None),
        Some(&extraction),
        &[],
        None,
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
        &catalog(),
        &query,
        &intent_with_quantity(Some(Quantity::TopN { value: 20 })),
        None,
        &[],
        None,
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

    let params = params_from_verified(&catalog(), &query, &intent, None, &[], None, None).unwrap();

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
        None,
    )
    .unwrap();

    assert_eq!(plan.params["limit"], 10);
}

/// No message means no way to check the model's claim against the user's own
/// words, so the claim is refused. `None` is the safe default, not a bypass.
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
    let error =
        params_from_verified(&catalog(), &query, &intent, None, &[], None, None).unwrap_err();

    assert!(error.to_string().contains("missing parameter search"));
}

/// The model may point at a name the user typed; it may not invent one.
///
/// Both directions matter and they are not symmetric. Accepting a name the user
/// typed rescues every lowercase spelling the extractor cannot anchor on;
/// accepting one the user did *not* type binds a substring match that silently
/// returns a different customer's rows. The surface check is the whole licence,
/// so it is asserted here rather than assumed.
#[test]
fn a_model_person_name_binds_only_when_the_user_typed_it() {
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

    // Case-insensitively present in the message: admissible.
    let params = params_from_verified(
        &catalog(),
        &query,
        &intent,
        None,
        &[],
        None,
        Some("berapa tabungan tony?"),
    )
    .unwrap();
    assert_eq!(params["search"], "Tony");

    // Absent from the message: refused, and the run asks rather than answering
    // with somebody else's rows.
    let error = params_from_verified(
        &catalog(),
        &query,
        &intent,
        None,
        &[],
        None,
        Some("how many clients do we have?"),
    )
    .unwrap_err();
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
        &catalog(),
        &query,
        &intent_with_quantity(None),
        Some(&extraction),
        &[],
        None,
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
        &catalog(),
        &query,
        &intent_with_quantity(None),
        None,
        &policies,
        Some(&ctx),
        None,
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
        &catalog(),
        &query,
        &intent_with_quantity(None),
        None,
        &policies,
        Some(&ctx),
        None,
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
        &catalog(),
        &query,
        &intent_with_quantity(None),
        None,
        &policies,
        Some(&ctx),
        None,
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
    let error = params_from_verified(
        &catalog(),
        &query,
        &intent_with_quantity(None),
        None,
        &[],
        None,
        None,
    )
    .unwrap_err();

    assert!(error.to_string().contains("missing parameter from_date"));
}

/// Phase 0 measurement. Every other test in this file hands the planner a value
/// it built itself; this one hands it the capability's *own* documented example
/// sentence and the real deterministic extractor, which is what production runs
/// (`runtime/execution.rs` calls `plan_selected_capability_verified` with the
/// extraction, never the `plan_selected_capability` shim).
///
/// A capability whose own example cannot reach a bound plan cannot be reached by
/// any user phrasing either — it is catalog-only. `catalog_validation.rs` skips
/// exactly these with a `continue`, so its coverage is the complement of the bug.
#[test]
fn every_approved_capability_can_execute_its_own_example() {
    let catalog = catalog();
    let ctx = crate::knowledge::catalog::parameter_policy::EvaluationContext {
        business_today: chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        wall_today: chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        authorized_office_ids: vec![1],
    };

    let mut unreachable: Vec<String> = Vec::new();
    for capability in catalog
        .capabilities
        .iter()
        .filter(|c| c.status == "approved_mvp")
        // A continuation is entered from a clarification the assistant itself
        // raised, never from a first-turn message, so its example is a
        // continuation phrasing and cannot bind the parameter the selection
        // supplies. The catalog declares which capabilities those are.
        .filter(|c| !c.continuation)
    {
        let Some(example) = capability.examples.first() else {
            unreachable.push(format!("{}: no examples declared", capability.id));
            continue;
        };
        let extraction = extract_message_facts(example);
        let intent = AssistantIntent {
            intent: AssistantIntentKind::ReportRequest,
            domain: AssistantDomain::Unknown,
            request_shape: capability.request_shape.clone(),
            language: AssistantLanguage::En,
            canonical_query_en: example.clone(),
            entities: Vec::new(),
            constraints: AssistantConstraints::default(),
            context_reference: ContextReference::None,
            source: None,
            confidence: 0.9,
            reason: example.clone(),
        };

        match plan_selected_capability_verified(
            &catalog,
            &capability.id,
            &intent,
            Some(&extraction),
            Some(&ctx),
            Some(example),
        ) {
            Err(error) => {
                unreachable.push(format!("{}: {error} — example: {example:?}", capability.id));
            }
            Ok(plan) => {
                let query = catalog
                    .queries
                    .iter()
                    .find(|q| q.id == capability.query_id)
                    .expect("validated catalog resolves query_id");
                for parameter in query.parameters.iter().filter(|p| {
                    p.required
                        && !matches!(
                            p.source.as_deref(),
                            Some("authorized_scope" | "transient_sensitive_input")
                        )
                }) {
                    if plan.params.get(&parameter.name).is_none() {
                        unreachable.push(format!(
                            "{}: required parameter {} unfilled — example: {example:?}",
                            capability.id, parameter.name
                        ));
                    }
                }
            }
        }
    }

    assert!(
        unreachable.is_empty(),
        "{} of {} approved capabilities cannot execute their own documented example:\n  {}",
        unreachable.len(),
        catalog
            .capabilities
            .iter()
            .filter(|c| c.status == "approved_mvp" && !c.continuation)
            .count(),
        unreachable.join("\n  ")
    );
}

/// The measured matrix: real sentences, the real extractor, the real planner.
///
/// One table rather than fifteen tests, because the thing under test is a single
/// decision — what kind of thing a captured phrase names — and the interesting
/// evidence is the *spread* of phrasings it has to survive: one word or two,
/// capitalised or not, English or Indonesian, anchored on a domain noun, on a
/// locative, or on a bare "nama".
///
/// Nine of these fifteen produced the wrong parameters before this table
/// existed. Seven named an office and none of the seven bound one: the office
/// simply vanished and the report came back covering every office the caller
/// could see, `terminal_state: "completed"`.
#[test]
fn named_entities_bind_the_filter_the_sentence_actually_names() {
    let catalog = catalog();
    let ctx = crate::knowledge::catalog::parameter_policy::EvaluationContext {
        business_today: chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        wall_today: chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        authorized_office_ids: vec![1, 2, 3, 4, 5, 6, 7, 40],
    };
    // (message, capability, parameters that must be bound exactly)
    let matrix: &[(&str, &str, &[(&str, &str)])] = &[
        // A one-word office name, anchored on a domain noun.
        (
            "pada office Foo ada berapa client?",
            "client_summary_by_office",
            &[("office_name", "Foo")],
        ),
        (
            "ada berapa client di office Foo",
            "client_summary_by_office",
            &[("office_name", "Foo")],
        ),
        (
            "how many clients in office Foo?",
            "client_summary_by_office",
            &[("office_name", "Foo")],
        ),
        // A one-word office name anchored only on a locative, alongside a limit.
        (
            "berikan saya 5 client yg ada pada Foo",
            "client_random_sample",
            &[("office_name", "Foo"), ("limit", "5")],
        ),
        // `nama` announces a name; the sentence says what kind of thing it names.
        (
            "apakah ada office dengan nama Foo",
            "organization_office_name_lookup",
            &[("office_name", "Foo")],
        ),
        (
            "is there an office named Foo?",
            "organization_office_name_lookup",
            &[("office_name", "Foo")],
        ),
        // The deployment spells child offices `Parent - Child`.
        (
            "berikan saya client yg ada pada Foo - Dubai Branch",
            "client_random_sample",
            &[("office_name", "Foo - Dubai Branch")],
        ),
        // A charge type typed entirely in lower case — this capability's own
        // documented example phrasing.
        (
            "ada berapa saving weekly charge yg dimiliki oleh system sekarang",
            "savings_charge_count_by_type",
            &[("charge_name", "weekly charge")],
        ),
        (
            "tipe weekly charge ada berapa di savings?",
            "savings_charge_count_by_type",
            &[("charge_name", "weekly charge")],
        ),
        // The six that already worked and must keep working.
        (
            "berapa savings account milik client Hiroshi Tanaka",
            "client_name_lookup",
            &[("search", "Hiroshi Tanaka")],
        ),
        (
            "berapa savings account milik client HIROSHI TANAKA",
            "client_name_lookup",
            &[("search", "HIROSHI TANAKA")],
        ),
        (
            "cari client dengan nama hiroshi tanaka",
            "client_name_lookup",
            &[("search", "hiroshi tanaka")],
        ),
        (
            "how many savings accounts does Hiroshi Tanaka have?",
            "client_savings_overview",
            &[("search", "Hiroshi Tanaka")],
        ),
        (
            "tipe Weekly Charge ada berapa di savings?",
            "savings_charge_count_by_type",
            &[("charge_name", "Weekly Charge")],
        ),
        (
            "how many Weekly Charge charges are on savings accounts?",
            "savings_charge_count_by_type",
            &[("charge_name", "Weekly Charge")],
        ),
    ];

    let mut failures = Vec::new();
    for (message, capability_id, expected) in matrix {
        let extraction = extract_message_facts(message);
        let intent = AssistantIntent {
            intent: AssistantIntentKind::ReportRequest,
            domain: AssistantDomain::Unknown,
            request_shape: Default::default(),
            language: AssistantLanguage::En,
            canonical_query_en: (*message).into(),
            entities: Vec::new(),
            constraints: AssistantConstraints::default(),
            context_reference: ContextReference::None,
            source: None,
            confidence: 0.9,
            reason: (*message).into(),
        };
        match plan_selected_capability_verified(
            &catalog,
            capability_id,
            &intent,
            Some(&extraction),
            Some(&ctx),
            Some(message),
        ) {
            Err(error) => failures.push(format!("{message:?}: planning failed: {error}")),
            Ok(plan) => {
                for (name, value) in *expected {
                    let bound = plan.params.get(*name).map(|bound| match bound {
                        serde_json::Value::String(text) => text.clone(),
                        other => other.to_string(),
                    });
                    if bound.as_deref() != Some(*value) {
                        failures.push(format!(
                            "{message:?}: {name} = {bound:?}, expected {value:?} \
                             (all params: {})",
                            plan.params
                        ));
                    }
                }
            }
        }

        // A named office also has to *reach* the sufficiency gate: it was inert
        // for exactly these sentences, because the extractor handed it nothing.
        if expected.iter().any(|(name, _)| *name == "office_name") {
            let expressed = crate::assistant::retrieval::sufficiency::expressed_filters(
                message,
                Some(&intent),
                Some(&extraction),
            );
            if !expressed.contains(&crate::assistant::ConstraintField::Office) {
                failures.push(format!(
                    "{message:?}: office filter never reached the sufficiency gate: {expressed:?}"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} sentences bound the wrong parameters:\n  {}",
        failures.len(),
        matrix.len(),
        failures.join("\n  ")
    );
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
