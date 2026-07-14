use chat::{
    assistant::{
        AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage, ContextReference,
        Evidence, EvidenceDecision, EvidenceEvaluator, RetrievalEngine, RetrievalPlan,
    },
    knowledge::model::{CapabilityKnowledge, ClassificationPolicy, KnowledgeCatalog},
};
use serde_json::json;

fn intent(kind: AssistantIntentKind, domain: AssistantDomain) -> AssistantIntent {
    AssistantIntent {
        intent: kind,
        domain,
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
    let eval = EvidenceEvaluator::default();
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
        EvidenceDecision::Clarify
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
            status: "approved".into(),
            domain: "savings".into(),
            query_id: "q".into(),
            output_mode: "single".into(),
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
