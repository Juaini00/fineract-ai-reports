mod common;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chat::assistant::{
    AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage, ClarificationOption,
    ClarificationOutcome, ClarificationPayload, ClarificationResolver, ContextMessage,
    ContextReference, ContextWarning, ContextWarningCode, ContextWindow, RequestGrouping,
    RequestOperation, RequestOutput, RequestPii, RequestShape, RequestSubject, ResponseBuilder,
    SemanticRouter,
    llm::{EmbeddingResponse, LlmClient, LlmPurpose, LlmResponse, TokenUsage},
};
use common::spawn_app;
use serde_json::json;

struct ScenarioFakeLlm;

#[async_trait]
impl LlmClient for ScenarioFakeLlm {
    async fn structured_value(
        &self,
        _purpose: LlmPurpose,
        _system: &str,
        user: &str,
        _schema: serde_json::Value,
    ) -> Result<LlmResponse<serde_json::Value>> {
        let message = serde_json::from_str::<serde_json::Value>(user)?["message"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        let (intent, domain) = route_case(&message);
        let request_shape = if message.contains("random") || message.contains("sembarang") {
            RequestShape {
                operation: RequestOperation::RandomSample,
                subject: RequestSubject::Client,
                grouping: RequestGrouping::None,
                output: RequestOutput::List,
                pii: RequestPii::ClientIdentity,
            }
        } else {
            RequestShape::default()
        };
        Ok(LlmResponse {
            value: json!({
                "intent": intent,
                "domain": domain,
                "request_shape": request_shape,
                "language": AssistantLanguage::En,
                "entities": [],
                "constraints": {},
                "context_reference": ContextReference::None,
                "confidence": 0.9,
                "reason": "scenario fake"
            }),
            usage: TokenUsage::default(),
            cost_usd: None,
            provider: "fake".into(),
            model: "fake".into(),
            latency_ms: 0,
        })
    }

    async fn embed(&self, _purpose: LlmPurpose, text: &str) -> Result<EmbeddingResponse> {
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

#[tokio::test]
async fn semantic_assistant_default_scenario_matrix_routes_without_live_services() {
    let router = SemanticRouter::new(Arc::new(ScenarioFakeLlm));
    for (prompt, expected_intent, expected_domain) in [
        (
            "Hi",
            AssistantIntentKind::Greeting,
            AssistantDomain::Unknown,
        ),
        (
            "kamu bisa apa aja?",
            AssistantIntentKind::Help,
            AssistantDomain::Unknown,
        ),
        (
            "ada gak nama Tony di client kita?",
            AssistantIntentKind::DataLookup,
            AssistantDomain::Client,
        ),
        (
            "total savings deposit this month",
            AssistantIntentKind::ReportRequest,
            AssistantDomain::Savings,
        ),
        (
            "Show savings activity",
            AssistantIntentKind::ReportRequest,
            AssistantDomain::Savings,
        ),
        (
            "yang balance aja",
            AssistantIntentKind::ClarificationReply,
            AssistantDomain::Savings,
        ),
        (
            "sekarang tampilkan client aktif bulan ini",
            AssistantIntentKind::FollowUp,
            AssistantDomain::Client,
        ),
        (
            "tau gak harga laptop?",
            AssistantIntentKind::OutOfDomain,
            AssistantDomain::Unknown,
        ),
        (
            "show raw account numbers",
            AssistantIntentKind::UnsafeRequest,
            AssistantDomain::Unknown,
        ),
        (
            "give me 5 random clients this year",
            AssistantIntentKind::ReportRequest,
            AssistantDomain::Client,
        ),
        (
            "coba berikan saya 5 client sembarang pada tahun ini",
            AssistantIntentKind::ReportRequest,
            AssistantDomain::Client,
        ),
    ] {
        let routed = router.route(prompt, &empty_context()).await.unwrap();
        assert_eq!(routed.intent, expected_intent, "{prompt}: {routed:?}");
        assert_eq!(routed.domain, expected_domain, "{prompt}: {routed:?}");
        if prompt.contains("random") || prompt.contains("sembarang") {
            assert_eq!(
                routed.request_shape.operation,
                RequestOperation::RandomSample
            );
        }
        assert_response_contract(prompt, &routed);
    }
}

#[tokio::test]
async fn semantic_clarification_reply_selects_balance_by_meaning() {
    let payload = ClarificationPayload {
        version: chat::assistant::clarification::CLARIFICATION_VERSION_1,
        id: uuid::Uuid::new_v4(),
        revision: 0,
        kind: chat::assistant::clarification::ClarificationKind::SelectOption,
        question: "Which savings report?".into(),
        options: vec![
            ClarificationOption {
                id: "savings_deposit_total".into(),
                label: "deposit total".into(),
                description: None,
                fields: Vec::new(),
            },
            ClarificationOption {
                id: "savings_balance_summary".into(),
                label: "balance summary".into(),
                description: None,
                fields: Vec::new(),
            },
        ],
        fields: Vec::new(),
        attempt: 1,
        source_intent: None,
        allow_free_text: true,
        is_missing_execution_parameters: false,
        workflow_id: None,
        node_id: None,
        resume_node_id: None,
        entity_kind: None,
    };

    let outcome = ClarificationResolver::resolve(
        "yang balance aja",
        &payload,
        &empty_context(),
        &ScenarioFakeLlm,
    )
    .await
    .unwrap();
    assert_eq!(
        outcome,
        ClarificationOutcome::SelectedOption {
            option_id: "savings_balance_summary".into(),
            confidence: 1.0,
        }
    );
}

#[test]
fn long_session_matrix_has_near_and_hard_limit_contracts() {
    let near = context_with_warning(ContextWarningCode::SessionContextNearLimit);
    let hard = context_with_warning(ContextWarningCode::SessionContextExceeded);

    assert_eq!(
        near.warnings[0].code,
        ContextWarningCode::SessionContextNearLimit
    );
    assert_eq!(
        hard.warnings[0].code,
        ContextWarningCode::SessionContextExceeded
    );
    assert!(near.recent_messages.len() > 8);
    assert!(hard.recent_messages.len() > near.recent_messages.len());
}

#[tokio::test(flavor = "multi_thread")]
async fn live_scenario_matrix_is_gated() {
    if std::env::var("RUN_LIVE_SCENARIO_MATRIX").ok().as_deref() != Some("1") {
        return;
    }

    let app = spawn_app().await;
    let key = app
        .provision_api_key(
            &[
                "savings_deposit_total",
                "savings_balance_summary",
                "client_name_lookup",
            ],
            vec![1, 2],
            false,
        )
        .await;
    for prompt in [
        "Hi",
        "kamu bisa apa aja?",
        "ada gak nama Tony di client kita?",
        "total savings deposit this month",
        "Show savings activity",
        "yang balance aja",
        "sekarang tampilkan client aktif bulan ini",
        "tau gak harga laptop?",
        "show raw account numbers",
    ] {
        let resp = app
            .post_json("/chat/jobs", Some(&key.raw), &json!({ "message": prompt }))
            .await;
        assert_eq!(
            resp.status(),
            201,
            "{prompt}: {}",
            resp.text().await.unwrap_or_default()
        );
    }
}

fn route_case(message: &str) -> (AssistantIntentKind, AssistantDomain) {
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
    } else if message.contains("random") || message.contains("sembarang") {
        (AssistantIntentKind::ReportRequest, AssistantDomain::Client)
    } else if message.contains("client") || message.contains("tony") {
        (AssistantIntentKind::DataLookup, AssistantDomain::Client)
    } else {
        (AssistantIntentKind::ReportRequest, AssistantDomain::Savings)
    }
}

fn fake_embedding(text: &str) -> Vec<f32> {
    let text = text.to_lowercase();
    vec![
        text.matches("client").count() as f32 + text.matches("tony").count() as f32,
        text.matches("saving").count() as f32,
        text.matches("balance").count() as f32,
        text.matches("deposit").count() as f32,
        text.matches("laptop").count() as f32,
    ]
}

fn assert_response_contract(prompt: &str, intent: &AssistantIntent) {
    let response = match intent.intent {
        AssistantIntentKind::Help => ResponseBuilder::help(),
        AssistantIntentKind::OutOfDomain => ResponseBuilder::out_of_domain(),
        AssistantIntentKind::UnsafeRequest => {
            ResponseBuilder::policy_blocked("Sensitive data is blocked by policy.")
        }
        AssistantIntentKind::ClarificationReply => {
            ResponseBuilder::clarification(ClarificationPayload {
                version: chat::assistant::clarification::CLARIFICATION_VERSION_1,
                id: uuid::Uuid::new_v4(),
                revision: 0,
                kind: chat::assistant::clarification::ClarificationKind::SelectOption,
                question: "Which report?".into(),
                options: vec![ClarificationOption {
                    id: "savings_balance_summary".into(),
                    label: "balance".into(),
                    description: None,
                    fields: Vec::new(),
                }],
                fields: Vec::new(),
                attempt: 1,
                source_intent: None,
                allow_free_text: true,
                is_missing_execution_parameters: false,
                workflow_id: None,
                node_id: None,
                resume_node_id: None,
                entity_kind: None,
            })
        }
        _ => ResponseBuilder::selected(format!("{:?}", intent.domain)),
    };
    let body = serde_json::to_string(&response).unwrap();
    for forbidden in [
        "SELECT ",
        "m_client",
        "m_savings",
        "panic",
        "stack backtrace",
    ] {
        assert!(
            !body.contains(forbidden),
            "{prompt} leaked {forbidden}: {body}"
        );
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
        source_intent: None,
        source_snippets: Vec::new(),
        client_scope: json!({}),
        warnings: Vec::new(),
    }
}

fn context_with_warning(code: ContextWarningCode) -> ContextWindow {
    let count = match code {
        ContextWarningCode::SessionContextNearLimit => 9,
        ContextWarningCode::SessionContextExceeded => 14,
    };
    ContextWindow {
        recent_messages: (0..count)
            .map(|i| ContextMessage {
                role: if i % 2 == 0 { "user" } else { "assistant" }.into(),
                content: "long context turn".into(),
                created_at: None,
            })
            .collect(),
        warnings: vec![ContextWarning {
            code,
            message: "context limit contract".into(),
        }],
        ..empty_context()
    }
}
