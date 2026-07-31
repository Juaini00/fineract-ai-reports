use std::sync::Arc;

use chat::{
    assistant::{
        AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage, ContextReference,
        Evidence, LlmReranker, RequestGrouping, RequestOperation, RequestOutput, RequestPii,
        RequestShape, RequestSubject, RerankerVerdict, RetrievalEngine, RetrievalPlan,
        llm::{FakeLlmClient, SharedLlmClient},
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
        canonical_query_en: String::new(),
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
        metadata: json!({"description": id}),
        conflicting: false,
    }
}

fn shared(client: FakeLlmClient) -> SharedLlmClient {
    Arc::new(client)
}

/// Reranker's LLM Select path routes to the chosen capability.
#[tokio::test]
async fn reranker_select_returns_capability() {
    let llm = FakeLlmClient::default();
    llm.push_structured(json!({
        "decision": "select",
        "capability_id": "savings_deposit_total",
        "confidence": 0.85,
        "alternatives": [],
        "reason": "matches total intent",
    }));
    let llm = shared(llm);
    let out = LlmReranker::new(Some(&llm))
        .rerank(
            "total savings deposits",
            &[evidence("savings_deposit_total", 0.7)],
        )
        .await;
    assert_eq!(out.decision, RerankerVerdict::Select);
    assert_eq!(out.capability_id.as_deref(), Some("savings_deposit_total"));
}

/// Reranker's LLM Clarify path returns alternatives for the runtime to route.
#[tokio::test]
async fn reranker_clarify_returns_alternatives() {
    let llm = FakeLlmClient::default();
    llm.push_structured(json!({
        "decision": "clarify",
        "capability_id": null,
        "confidence": 0.0,
        "alternatives": ["savings_deposit_total", "savings_deposit_top_n"],
        "reason": "ambiguous",
    }));
    let llm = shared(llm);
    let out = LlmReranker::new(Some(&llm))
        .rerank(
            "savings report",
            &[
                evidence("savings_deposit_total", 0.6),
                evidence("savings_deposit_top_n", 0.55),
            ],
        )
        .await;
    assert_eq!(out.decision, RerankerVerdict::Clarify);
    assert_eq!(
        out.alternatives,
        vec![
            "savings_deposit_total".to_string(),
            "savings_deposit_top_n".to_string()
        ]
    );
}

/// Reranker's LLM Unsupported path surfaces the semantic-mismatch verdict.
#[tokio::test]
async fn reranker_unsupported_passes_through() {
    let llm = FakeLlmClient::default();
    llm.push_structured(json!({
        "decision": "unsupported",
        "capability_id": null,
        "confidence": 0.0,
        "alternatives": [],
        "reason": "no candidate matches",
    }));
    let llm = shared(llm);
    let out = LlmReranker::new(Some(&llm))
        .rerank("weather report", &[evidence("savings_deposit_total", 0.4)])
        .await;
    assert_eq!(out.decision, RerankerVerdict::Unsupported);
}

/// Empty candidates short-circuit to Unsupported without an LLM call.
#[tokio::test]
async fn reranker_empty_candidates_is_unsupported() {
    let llm = shared(FakeLlmClient::default());
    let out = LlmReranker::new(Some(&llm)).rerank("anything", &[]).await;
    assert_eq!(out.decision, RerankerVerdict::Unsupported);
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
            defaults: Default::default(),
            guards: Default::default(),
            parameter_policies: vec![],
        }],
        queries: vec![],
        policies: vec![],
        responses: vec![],
        parameter_inputs: Vec::new(),
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
    let runtime = include_str!("../src/assistant/execution/runtime/mod.rs");
    assert!(!runtime.contains("capability_matches_prompt_shape"));
    assert!(!runtime.contains("domain_terms"));
}

#[tokio::test]
async fn shape_mismatch_no_longer_empties_random_clients_but_still_narrows_office_savings() {
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
