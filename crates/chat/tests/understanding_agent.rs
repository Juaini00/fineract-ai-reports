use std::{path::PathBuf, sync::Arc};

use app_core::auth::model::PrincipalContext;
use chat::{
    assistant::{
        llm::agent::understanding::{UnderstandingAgent, UnderstandingAgentError},
        llm::{FakeLlmClient, SharedLlmClient},
    },
    knowledge::model::{
        CapabilityDefaults, CapabilityGuards, CapabilityKnowledge, ClassificationPolicy,
        KnowledgeCatalog,
    },
};
use uuid::Uuid;

fn catalog() -> KnowledgeCatalog {
    KnowledgeCatalog {
        root_path: PathBuf::new(),
        query_path: PathBuf::new(),
        data_areas: vec![],
        domains: vec![],
        schemas: vec![],
        metrics: vec![],
        capabilities: vec![CapabilityKnowledge {
            id: "savings_deposit_top_n".into(),
            status: "approved_mvp".into(),
            domain: "savings".into(),
            query_id: "savings.deposit_top_n".into(),
            dataset_recipe: None,
            output_mode: "top_n".into(),
            request_shape: Default::default(),
            kind: Default::default(),
            member_capability_ids: vec![],
            display_name: None,
            description: None,
            data_areas: vec![],
            metrics: vec![],
            examples: vec![],
            continuation: false,
            required_parameters: vec![],
            optional_parameters: vec![],
            defaults: CapabilityDefaults::default(),
            guards: CapabilityGuards::default(),
            supported_intents: vec![],
            unsupported_intents: vec![],
            parameter_policies: vec![],
        }],
        queries: vec![],
        policies: vec![],
        responses: vec![],
        parameter_bindings: Default::default(),
        parameter_inputs: vec![],
        classification: ClassificationPolicy::default(),
        datasets: vec![],
    }
}

fn principal() -> PrincipalContext {
    PrincipalContext {
        user_id: Uuid::nil(),
        role: "admin".into(),
        office_ids: vec![1],
        capability_ids: vec!["savings_deposit_top_n".into()],
        can_view_pii: true,
        legacy_api_key_id: None,
    }
}

#[tokio::test]
async fn rejects_candidate_outside_the_visible_catalog_at_the_agent_boundary() {
    let fake = Arc::new(FakeLlmClient::default());
    fake.push_structured(serde_json::json!({
        "intent_kind": "report_request",
        "domain": "savings",
        "language": "en",
        "candidates": [{
            "capability_id": "invented_capability",
            "confidence": 1.0,
            "why": "invented"
        }]
    }));
    let agent = UnderstandingAgent::new(fake as SharedLlmClient, 2);

    let error = agent
        .extract("show deposits", None, &catalog(), &principal())
        .await
        .expect_err("a catalog-invisible capability must not reach downstream stages");

    assert!(matches!(error, UnderstandingAgentError::CatalogVocabulary));
}

#[tokio::test]
async fn stops_at_the_configured_turn_limit_when_the_model_never_produces_a_valid_extraction() {
    let fake = Arc::new(FakeLlmClient::default());
    fake.push_structured(serde_json::json!({"broken": true}));
    fake.push_structured(serde_json::json!({"broken": true}));
    fake.push_structured(serde_json::json!({
        "intent_kind": "report_request",
        "domain": "savings",
        "language": "en"
    }));
    let agent = UnderstandingAgent::new(fake as SharedLlmClient, 2);

    let error = agent
        .extract("show deposits", None, &catalog(), &principal())
        .await
        .expect_err("the third model response must not be requested");

    assert!(matches!(error, UnderstandingAgentError::MaxTurnsExceeded));
}
