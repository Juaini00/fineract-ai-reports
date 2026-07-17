use chat::{
    assistant::{
        AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage, ContextReference,
        Evidence, EvidenceDecision, EvidenceEvaluator, RequestGrouping, RequestOperation,
        RequestOutput, RequestPii, RequestShape, RequestSubject, RetrievalEngine, RetrievalPlan,
    },
    knowledge::catalog::loader::KnowledgeLoader,
    knowledge::model::{CapabilityKnowledge, ClassificationPolicy, KnowledgeCatalog},
};
use serde_json::json;

fn intent(kind: AssistantIntentKind, domain: AssistantDomain) -> AssistantIntent {
    AssistantIntent {
        intent: kind,
        domain,
        request_shape: Default::default(),
        language: AssistantLanguage::En,
        entities: vec![],
        constraints: Default::default(),
        context_reference: ContextReference::None,
        source: None,
        confidence: 0.9,
        reason: "test".into(),
    }
}

fn evidence(id: &str, score: f32) -> Evidence {
    Evidence {
        capability_id: id.into(),
        title: id.into(),
        score,
        source_type: "capability".into(),
        metadata: json!({}),
        conflicting: false,
    }
}

#[test]
fn evidence_decisions_cover_phase8_states() {
    let eval = EvidenceEvaluator;
    let plan = RetrievalPlan::new(
        "savings",
        &intent(AssistantIntentKind::ReportRequest, AssistantDomain::Savings),
        false,
        vec!["cap".into()],
    );
    assert_eq!(
        eval.evaluate(&plan, &[evidence("cap", 0.9)]),
        EvidenceDecision::Select {
            capability_id: "cap".into()
        }
    );
    assert_eq!(
        eval.evaluate(&plan, &[evidence("cap", 0.3)]),
        EvidenceDecision::Select {
            capability_id: "cap".into()
        }
    );
    assert_eq!(
        eval.evaluate(&plan, &[]),
        EvidenceDecision::UnsupportedInDomain
    );
    assert_eq!(
        eval.evaluate(
            &RetrievalPlan::new(
                "x",
                &intent(AssistantIntentKind::OutOfDomain, AssistantDomain::Unknown),
                true,
                vec![]
            ),
            &[]
        ),
        EvidenceDecision::OutOfDomain
    );
    assert_eq!(
        eval.evaluate(
            &RetrievalPlan::new(
                "x",
                &intent(AssistantIntentKind::UnsafeRequest, AssistantDomain::Savings),
                true,
                vec![]
            ),
            &[]
        ),
        EvidenceDecision::BlockedByPolicy
    );
    let mut conflict = evidence("cap", 0.9);
    conflict.conflicting = true;
    assert_eq!(eval.evaluate(&plan, &[conflict]), EvidenceDecision::Clarify);
}

#[tokio::test]
async fn catalog_fallback_retrieves_without_vector_repo() {
    let catalog = std::sync::Arc::new(KnowledgeCatalog {
        root_path: Default::default(),
        query_path: Default::default(),
        data_areas: vec![],
        domains: vec![],
        schemas: vec![],
        metrics: vec![],
        capabilities: vec![CapabilityKnowledge {
            id: "savings_deposit_total".into(),
            status: "approved_mvp".into(),
            domain: "savings".into(),
            query_id: "q".into(),
            output_mode: "single".into(),
            request_shape: RequestShape::default(),
            display_name: Some("Savings deposit total".into()),
            description: Some("deposit total".into()),
            data_areas: vec![],
            metrics: vec![],
            examples: vec!["show savings deposits".into()],
            required_parameters: vec![],
            optional_parameters: vec![],
        }],
        queries: vec![],
        policies: vec![],
        responses: vec![],
        classification: ClassificationPolicy::default(),
    });
    let plan = RetrievalPlan::new(
        "show savings deposits",
        &intent(AssistantIntentKind::ReportRequest, AssistantDomain::Savings),
        true,
        vec![],
    );
    let evidence = RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
        .await
        .unwrap();
    assert_eq!(evidence[0].capability_id, "savings_deposit_total");
}

#[test]
fn primary_runtime_does_not_use_legacy_prompt_shape_helpers() {
    let runtime = include_str!("../src/assistant/runtime/mod.rs");
    assert!(!runtime.contains("capability_matches_prompt_shape"));
    assert!(!runtime.contains("domain_terms"));
}

#[tokio::test]
async fn shape_mismatch_no_longer_empties_random_clients_but_still_narrows_office_savings() {
    // Issue 01 (retrieval-pipeline-rework): shape is now a scoring signal, not a
    // hard gate. No capability in the real catalog has operation=RandomSample,
    // so retrieval must still surface catalog_fallback candidates (ranked by
    // keyword/semantic score) instead of collapsing to empty; ambiguity is a
    // downstream (EvidenceEvaluator) concern now, not retrieval's.
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = std::sync::Arc::new(
        KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
            .load()
            .unwrap(),
    );
    let mut random = intent(AssistantIntentKind::ReportRequest, AssistantDomain::Client);
    random.request_shape = RequestShape {
        operation: RequestOperation::RandomSample,
        subject: RequestSubject::Client,
        grouping: RequestGrouping::None,
        output: RequestOutput::List,
        pii: RequestPii::ClientIdentity,
    };
    for prompt in [
        "coba berikan saya 5 client sembarang pada tahun ini",
        "give me 5 random clients this year",
    ] {
        let plan = RetrievalPlan::new(prompt, &random, true, vec![]);
        assert!(
            !RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
                .await
                .unwrap()
                .is_empty(),
            "shape mismatch alone must not collapse retrieval to empty for prompt: {prompt}"
        );
    }

    let mut office = intent(
        AssistantIntentKind::ReportRequest,
        AssistantDomain::Organization,
    );
    office.request_shape = RequestShape {
        operation: RequestOperation::Rank,
        subject: RequestSubject::Office,
        grouping: RequestGrouping::Office,
        output: RequestOutput::Ranking,
        pii: RequestPii::None,
    };
    office.constraints.metric = Some("savings balance".into());
    let evidence = RetrievalEngine::retrieve(
        &RetrievalPlan::new("office savings summary top 5 in IDR", &office, true, vec![]),
        None,
        None,
        Some(&catalog),
    )
    .await
    .unwrap();
    // Issue 01: catalog_fallback no longer gates on shape/domain, so several
    // office-shaped capabilities now qualify by keyword overlap alone (ranking
    // precision among ties is issue 02's reranker concern, not retrieval's).
    // Assert the correct capability is surfaced rather than the sole result.
    assert!(
        evidence
            .iter()
            .any(|item| item.capability_id == "organization_office_savings_summary"),
        "expected organization_office_savings_summary among {:?}",
        evidence
            .iter()
            .map(|item| item.capability_id.as_str())
            .collect::<Vec<_>>()
    );
}
