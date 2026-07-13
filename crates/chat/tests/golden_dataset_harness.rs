use serde::Deserialize;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chat::assistant::{
    AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage, ClarificationOption,
    ClarificationOutcome, ClarificationPayload, ClarificationResolver, ContextReference,
    ContextWindow, ResponseBuilder, SemanticRouter,
    llm::{EmbeddingResponse, LlmClient, LlmResponse, TokenUsage},
};
use chat::knowledge::catalog::loader::KnowledgeLoader;
use serde_json::json;

#[derive(Debug, Deserialize)]
struct GoldenCase {
    prompt: String,
    expected_intent: String,
    expected_domain: String,
    #[serde(default)]
    expected_entities: Vec<serde_json::Value>,
    expected_response_type: String,
}

struct GoldenFakeLlm;

#[async_trait]
impl LlmClient for GoldenFakeLlm {
    async fn structured_value(
        &self,
        _system: &str,
        user: &str,
        _schema: serde_json::Value,
    ) -> Result<LlmResponse<serde_json::Value>> {
        let message = serde_json::from_str::<serde_json::Value>(user)?["message"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        let (intent, domain) = classify_for_golden(&message);
        Ok(LlmResponse {
            value: json!({
                "intent": intent,
                "domain": domain,
                "language": AssistantLanguage::En,
                "entities": [],
                "constraints": {},
                "context_reference": ContextReference::None,
                "confidence": 0.9,
                "reason": "offline golden fake"
            }),
            usage: TokenUsage::default(),
            cost_usd: None,
            provider: "fake".into(),
            model: "fake".into(),
            latency_ms: 0,
        })
    }

    async fn embed(&self, text: &str) -> Result<EmbeddingResponse> {
        Ok(EmbeddingResponse {
            vector: fake_embedding(text),
            usage: TokenUsage::default(),
            cost_usd: None,
            provider: "fake".into(),
            model: "fake".into(),
            latency_ms: 0,
        })
    }
}

fn fake_embedding(text: &str) -> Vec<f32> {
    let text = text.to_lowercase();
    vec![
        text.matches("client").count() as f32,
        text.matches("saving").count() as f32,
        text.matches("balance").count() as f32,
        text.matches("monthly").count() as f32,
        text.matches("loan").count() as f32,
    ]
}

fn classify_for_golden(message: &str) -> (AssistantIntentKind, AssistantDomain) {
    if message == "hi" {
        (AssistantIntentKind::Greeting, AssistantDomain::Unknown)
    } else if message.contains("bisa apa") {
        (AssistantIntentKind::Help, AssistantDomain::Unknown)
    } else if message.contains("laptop") {
        (AssistantIntentKind::OutOfDomain, AssistantDomain::Unknown)
    } else if message.contains("raw account") {
        (AssistantIntentKind::UnsafeRequest, AssistantDomain::Unknown)
    } else if message.contains("yang balance") {
        (
            AssistantIntentKind::ClarificationReply,
            AssistantDomain::Savings,
        )
    } else if message.contains("sekarang") {
        (AssistantIntentKind::FollowUp, AssistantDomain::Client)
    } else if message.contains("client") {
        (AssistantIntentKind::DataLookup, AssistantDomain::Client)
    } else {
        (AssistantIntentKind::ReportRequest, AssistantDomain::Savings)
    }
}

fn empty_context() -> ContextWindow {
    ContextWindow {
        summary: None,
        active_domain: None,
        selected_entities: json!({}),
        recent_messages: Vec::new(),
        relevant_jobs: Vec::new(),
        pending_clarification: None,
        client_scope: json!({}),
        warnings: Vec::new(),
    }
}

fn fake_response_type(intent: &AssistantIntent) -> String {
    let response_type = match intent.intent {
        AssistantIntentKind::Help => chat::assistant::AssistantResponseType::Help,
        AssistantIntentKind::DataLookup | AssistantIntentKind::FollowUp => {
            chat::assistant::AssistantResponseType::Table
        }
        AssistantIntentKind::ClarificationReply => {
            ResponseBuilder::clarification(ClarificationPayload {
                question: "Which report?".into(),
                options: Vec::new(),
                attempt: 1,
            })
            .response_type
        }
        AssistantIntentKind::OutOfDomain => ResponseBuilder::out_of_domain().response_type,
        AssistantIntentKind::UnsafeRequest => {
            ResponseBuilder::policy_blocked("blocked").response_type
        }
        _ => chat::assistant::AssistantResponseType::Summary,
    };
    serde_json::to_string(&response_type)
        .unwrap()
        .trim_matches('"')
        .to_string()
}

fn load_seed() -> Vec<GoldenCase> {
    include_str!("../../../tests/golden/seed.jsonl")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<GoldenCase>)
        .collect::<Result<Vec<_>, _>>()
        .expect("seed golden JSONL parses")
}

#[test]
fn golden_seed_dataset_parses() {
    let rows = load_seed();
    assert!(rows.len() >= 8);
    assert!(rows.iter().any(|row| row.expected_intent == "data_lookup"));
    assert!(
        rows.iter()
            .any(|row| row.expected_intent == "clarification_reply")
    );
    assert!(rows.iter().all(|row| !row.prompt.trim().is_empty()));
    assert!(
        rows.iter()
            .all(|row| !row.expected_domain.trim().is_empty())
    );
    assert!(
        rows.iter()
            .all(|row| !row.expected_response_type.trim().is_empty())
    );
    let entity_count: usize = rows.iter().map(|row| row.expected_entities.len()).sum();
    assert!(entity_count > 0);
}

#[tokio::test]
async fn offline_fake_resolver_selects_semantic_balance_option() {
    let payload = ClarificationPayload {
        question: "Which savings report?".into(),
        options: vec![
            ClarificationOption {
                id: "savings_balance_summary".into(),
                label: "balance".into(),
                description: None,
            },
            ClarificationOption {
                id: "savings_deposit_total".into(),
                label: "deposit total".into(),
                description: None,
            },
        ],
        attempt: 1,
    };
    let outcome = ClarificationResolver::resolve(
        "yang balance aja",
        &payload,
        &empty_context(),
        &GoldenFakeLlm,
    )
    .await
    .unwrap();
    assert_eq!(
        outcome,
        ClarificationOutcome::SelectedOption {
            option_id: "savings_balance_summary".into(),
            confidence: 1.0
        }
    );
}

#[tokio::test]
async fn offline_fake_router_meets_seed_accuracy_floor() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
        .load()
        .expect("load catalog");
    let router = SemanticRouter::new(Arc::new(GoldenFakeLlm), &catalog);
    let rows = load_seed();
    let mut matches = 0usize;
    for row in &rows {
        let routed = router.route(&row.prompt, &empty_context()).await.unwrap();
        let intent = serde_json::to_string(&routed.intent)
            .unwrap()
            .trim_matches('"')
            .to_string();
        let domain = serde_json::to_string(&routed.domain)
            .unwrap()
            .trim_matches('"')
            .to_string();
        if intent == row.expected_intent && domain == row.expected_domain {
            matches += 1;
        }
        assert_eq!(fake_response_type(&routed), row.expected_response_type);
    }
    let accuracy = matches as f32 / rows.len() as f32;
    assert!(accuracy >= 0.8, "accuracy {accuracy:.2} below floor");
}
