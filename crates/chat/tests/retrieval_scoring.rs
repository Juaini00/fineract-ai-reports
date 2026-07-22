use chat::assistant::evidence::RetrievalPlan;
use chat::assistant::retrieval::compatible_ids;
use chat::assistant::{
    AssistantConstraints, AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage,
    ContextReference, RequestGrouping, RequestOperation, RequestOutput, RequestPii, RequestShape,
    RequestSubject,
};
use chat::knowledge::model::{CapabilityKnowledge, KnowledgeCatalog};

fn make_intent(domain: AssistantDomain, subject: RequestSubject) -> AssistantIntent {
    AssistantIntent {
        intent: AssistantIntentKind::ReportRequest,
        domain,
        request_shape: RequestShape {
            operation: RequestOperation::Rank,
            subject,
            grouping: RequestGrouping::None,
            output: RequestOutput::Ranking,
            pii: RequestPii::ClientIdentity,
        },
        language: AssistantLanguage::En,
        entities: Vec::new(),
        constraints: AssistantConstraints::default(),
        context_reference: ContextReference::None,
        source: None,
        confidence: 0.9,
        reason: "test".into(),
    }
}

fn make_capability(id: &str, domain: &str, subject: RequestSubject) -> CapabilityKnowledge {
    CapabilityKnowledge {
        id: id.into(),
        status: "approved_mvp".into(),
        domain: domain.into(),
        display_name: Some(id.into()),
        description: Some(format!("test capability {id}")),
        data_areas: vec![],
        query_id: format!("{id}.query"),
        metrics: vec!["savings.account_count".into()],
        output_mode: "top_n".into(),
        request_shape: RequestShape {
            operation: RequestOperation::Rank,
            subject,
            grouping: RequestGrouping::None,
            output: RequestOutput::Ranking,
            pii: RequestPii::ClientIdentity,
        },
        examples: vec![],
        required_parameters: vec![],
        optional_parameters: vec![],
    }
}

fn catalog_with(capability: CapabilityKnowledge) -> KnowledgeCatalog {
    KnowledgeCatalog {
        root_path: Default::default(),
        query_path: Default::default(),
        data_areas: vec![],
        domains: vec![],
        schemas: vec![],
        metrics: vec![],
        capabilities: vec![capability],
        queries: vec![],
        policies: vec![],
        responses: vec![],
        classification: Default::default(),
    }
}

#[test]
fn domain_mismatch_does_not_exclude_capability_when_subject_matches() {
    // Regression for issue 04: router misclassifies domain as Savings for
    // "top clients by savings account" queries while subject is correctly Client.
    // Previously this filtered out client_top_n_by_savings_account_count.
    let intent = make_intent(AssistantDomain::Savings, RequestSubject::Client);
    let plan = RetrievalPlan::new(
        "top 3 clients by savings account",
        &intent,
        false,
        vec!["client_top_n_by_savings_account_count".to_string()],
    );
    let catalog = catalog_with(make_capability(
        "client_top_n_by_savings_account_count",
        "client",
        RequestSubject::Client,
    ));

    let compat = compatible_ids(&plan, &catalog);
    assert_eq!(
        compat,
        vec!["client_top_n_by_savings_account_count".to_string()],
        "capability with domain=client must survive when plan.domain=Savings and subject matches"
    );
}

#[test]
fn shape_score_ranks_full_match_over_partial_match() {
    use chat::assistant::retrieval::shape_score;

    let intent = make_intent(AssistantDomain::Client, RequestSubject::Client);
    let plan = RetrievalPlan::new("top clients", &intent, false, vec![]);

    let full = make_capability("full", "client", RequestSubject::Client);
    let partial = make_capability("partial", "client", RequestSubject::Office);
    // partial mismatches subject only

    let full_score = shape_score(&plan, &full);
    let partial_score = shape_score(&plan, &partial);

    assert!(
        full_score > partial_score,
        "full={full_score} partial={partial_score}"
    );
    assert!((0.0..=1.0).contains(&full_score));
    assert!((0.0..=1.0).contains(&partial_score));
}

#[test]
fn retrieve_returns_candidates_when_no_shape_matches_but_catalog_non_empty() {
    // Regression for issue 01: previously an empty compatible_ids collapsed
    // the entire pipeline. Now retrieve must still surface catalog_fallback
    // candidates, letting downstream (reranker / evaluator) decide.
    use chat::assistant::retrieval::RetrievalEngine;

    let intent = make_intent(AssistantDomain::Organization, RequestSubject::Office);
    let mut shape = intent.request_shape.clone();
    shape.operation = RequestOperation::RandomSample;
    let mut intent = intent;
    intent.request_shape = shape;

    let plan = RetrievalPlan::new(
        "berikan 3 office",
        &intent,
        false,
        vec!["organization_office_summary".to_string()],
    );
    let mut cap = make_capability(
        "organization_office_summary",
        "organization",
        RequestSubject::Office,
    );
    cap.request_shape.operation = RequestOperation::Summary;
    let catalog = catalog_with(cap);
    let catalog = std::sync::Arc::new(catalog);

    let evidence = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async { RetrievalEngine::retrieve(&plan, None, None, Some(&catalog)).await })
        .expect("retrieve should not error");

    assert!(
        !evidence.is_empty(),
        "shape mismatch alone must not collapse retrieval to empty"
    );
    assert_eq!(evidence[0].capability_id, "organization_office_summary");
}

#[test]
fn top_n_by_savings_account_count_selected_for_rank_query() {
    // Query from prod log 2026-07-17: "3 clients where have the most savings account for this year"
    let intent = make_intent(AssistantDomain::Savings, RequestSubject::Client); // domain misclassified — must not matter
    let plan = RetrievalPlan::new(
        "3 clients where have the most savings account for this year",
        &intent,
        false,
        vec![
            "client_top_n_by_savings_account_count".to_string(),
            "savings_deposit_total".to_string(),
        ],
    );
    let mut target = make_capability(
        "client_top_n_by_savings_account_count",
        "client",
        RequestSubject::Client,
    );
    target.description = Some("Top clients by number of active savings accounts".into());
    let mut distractor = make_capability(
        "savings_deposit_total",
        "savings",
        RequestSubject::SavingsTransaction,
    );
    distractor.request_shape.operation = RequestOperation::Total;
    distractor.request_shape.output = RequestOutput::Scalar;

    let catalog = std::sync::Arc::new(KnowledgeCatalog {
        capabilities: vec![target, distractor],
        ..catalog_with(make_capability("_", "_", RequestSubject::Client))
    });
    let evidence = tokio::runtime::Runtime::new().unwrap().block_on(async {
        chat::assistant::retrieval::RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
            .await
            .unwrap()
    });
    assert_eq!(
        evidence[0].capability_id,
        "client_top_n_by_savings_account_count"
    );
}

#[test]
fn build_retrieval_trace_emits_expected_top_level_keys() {
    use chat::assistant::evidence::Evidence;
    use chat::assistant::reranker::RerankerDecision;
    use chat::assistant::runtime::build_retrieval_trace;

    let intent = make_intent(AssistantDomain::Client, RequestSubject::Client);
    let plan = RetrievalPlan::new("top 3 clients", &intent, false, vec!["capability_a".into()]);
    let evidence = vec![Evidence {
        capability_id: "capability_a".into(),
        title: "Cap A".into(),
        score: 0.82,
        source_type: "capability".into(),
        metadata: serde_json::json!({}),
        conflicting: false,
    }];
    let decision = RerankerDecision::select("capability_a", 0.9);

    let trace = build_retrieval_trace(&intent, &plan, &evidence, &decision);

    let obj = trace.as_object().expect("trace must be a JSON object");
    for key in ["router_intent", "plan", "candidates", "decision"] {
        assert!(obj.contains_key(key), "missing key {key}");
    }
    let candidates = trace["candidates"].as_array().expect("candidates array");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["capability_id"], "capability_a");
    assert_eq!(trace["decision"]["kind"], "select");
    assert_eq!(trace["decision"]["capability_id"], "capability_a");
}

#[test]
fn clarification_option_falls_back_to_humanized_id_when_display_name_missing() {
    // Regression for issue 08 Bug A: organization_office_summary rendered in
    // a clarification prompt with label = raw id ("organization_office_summary")
    // because the YAML lacked display_name and the option builder had no
    // humanization fallback. The fix is a humanize_id() helper used wherever
    // a ClarificationOption label is derived from a capability.
    use chat::assistant::clarification::{ClarificationOption, humanize_id};

    let mut cap = make_capability(
        "organization_office_summary",
        "organization",
        RequestSubject::Office,
    );
    cap.display_name = None;

    let option = ClarificationOption {
        id: cap.id.clone(),
        label: cap
            .display_name
            .clone()
            .unwrap_or_else(|| humanize_id(&cap.id)),
        description: cap.description.clone(),
        fields: Vec::new(),
    };

    assert_eq!(option.label, "Organization Office Summary");
    assert_ne!(
        option.label, cap.id,
        "label must not be the raw capability id"
    );
}

/// Loads the real `knowledge/` + `queries/` catalog from the workspace, the
/// same way `catalog_validation.rs` does. Using the real catalog (rather than
/// a synthetic one) is what makes these three tests genuinely RED before the
/// issue-03 capability YAMLs exist: the target id simply isn't in the catalog
/// yet, so `evidence[0].capability_id` cannot equal it.
fn load_real_catalog() -> std::sync::Arc<KnowledgeCatalog> {
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let catalog = chat::knowledge::catalog::loader::KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("load knowledge catalog");
    std::sync::Arc::new(catalog)
}

#[test]
fn office_list_basic_selected_for_berikan_office_query() {
    // Issue 03: "berikan 3 office yg ada pada system saat ini" has no ranking
    // metric — must route to the plain browse/list capability, not a summary
    // or top-n-by-metric capability.
    use chat::assistant::retrieval::RetrievalEngine;

    let intent = make_intent(AssistantDomain::Organization, RequestSubject::Office);
    let mut shape = intent.request_shape.clone();
    shape.operation = RequestOperation::List;
    shape.output = RequestOutput::List;
    shape.pii = RequestPii::None;
    let mut intent = intent;
    intent.request_shape = shape;

    let plan = RetrievalPlan::new(
        "berikan 3 office yg ada pada system saat ini",
        &intent,
        false,
        vec![
            "office_list_basic".to_string(),
            "organization_office_summary".to_string(),
        ],
    );

    let catalog = load_real_catalog();

    let evidence = tokio::runtime::Runtime::new().unwrap().block_on(async {
        RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
            .await
            .unwrap()
    });

    assert_eq!(evidence[0].capability_id, "office_list_basic");
}

#[test]
fn client_list_recent_selected_for_new_clients_query() {
    // "recently activated clients" has no ranking metric — must route to the
    // plain browse/list capability, not a lifecycle summary or top-n capability.
    use chat::assistant::retrieval::RetrievalEngine;

    let intent = make_intent(AssistantDomain::Client, RequestSubject::Client);
    let mut shape = intent.request_shape.clone();
    shape.operation = RequestOperation::List;
    shape.output = RequestOutput::List;
    let mut intent = intent;
    intent.request_shape = shape;

    let plan = RetrievalPlan::new(
        "show me the most recently activated clients",
        &intent,
        false,
        vec![
            "client_list_recent".to_string(),
            "client_top_n_by_savings_account_count".to_string(),
        ],
    );

    let catalog = load_real_catalog();

    let evidence = tokio::runtime::Runtime::new().unwrap().block_on(async {
        RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
            .await
            .unwrap()
    });

    assert_eq!(evidence[0].capability_id, "client_list_recent");
}

#[test]
fn client_random_sample_selected_for_sembarang_query() {
    // "coba berikan saya 5 client sembarang pada tahun ini" — "sembarang"
    // (Indonesian for "random/arbitrary") must route to the random-sample
    // capability, not a ranked or plain-list capability.
    use chat::assistant::retrieval::RetrievalEngine;

    let intent = make_intent(AssistantDomain::Client, RequestSubject::Client);
    let mut shape = intent.request_shape.clone();
    shape.operation = RequestOperation::RandomSample;
    shape.output = RequestOutput::List;
    let mut intent = intent;
    intent.request_shape = shape;

    let plan = RetrievalPlan::new(
        "coba berikan saya 5 client sembarang pada tahun ini",
        &intent,
        false,
        vec![
            "client_random_sample".to_string(),
            "client_list_recent".to_string(),
        ],
    );

    let catalog = load_real_catalog();

    let evidence = tokio::runtime::Runtime::new().unwrap().block_on(async {
        RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
            .await
            .unwrap()
    });

    assert_eq!(evidence[0].capability_id, "client_random_sample");
}

#[test]
fn retrieval_evidence_title_is_humanized_when_display_name_missing() {
    // Same regression, exercised through the actual production fallback
    // path (RetrievalEngine::retrieve -> catalog_fallback) that feeds
    // clarification_payload's ClarificationOption labels.
    use chat::assistant::retrieval::RetrievalEngine;

    let intent = make_intent(AssistantDomain::Organization, RequestSubject::Office);
    let plan = RetrievalPlan::new(
        "berikan office summary",
        &intent,
        false,
        vec!["organization_office_summary".to_string()],
    );
    let mut cap = make_capability(
        "organization_office_summary",
        "organization",
        RequestSubject::Office,
    );
    cap.display_name = None;
    let catalog = std::sync::Arc::new(catalog_with(cap));

    let evidence = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async { RetrievalEngine::retrieve(&plan, None, None, Some(&catalog)).await })
        .expect("retrieve should not error");

    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].title, "Organization Office Summary");
    assert_ne!(evidence[0].title, "organization_office_summary");
}
