use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

use super::*;
use crate::{
    assistant::{
        AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage, ContextReference,
        llm::{EmbeddingResponse, LlmClient, LlmResponse, TokenUsage},
    },
    knowledge::model::KnowledgeCatalog,
};

fn empty_memory() -> JobMemory {
    JobMemory {
        job_id: Uuid::nil(),
        graph_state: "receive_message".into(),
        terminal_state: None,
        current_user_message_metadata: json!({}),
        intent: None,
        source_intent: None,
        retrieval_plan: json!({}),
        retrieval_evidence: json!({}),
        evidence_decision: json!({}),
        selected_capability: None,
        selected_tool: None,
        tool_params: json!({}),
        policy_decision: json!({}),
        execution_summary: json!({}),
        structured_response: None,
        planner_snapshot_id: None,
        warnings: json!([]),
        revision: 0,
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

#[test]
fn traverses_three_state_skeleton() {
    let memory = JobMemory {
        job_id: Uuid::nil(),
        graph_state: "receive_message".into(),
        terminal_state: None,
        current_user_message_metadata: json!({}),
        intent: None,
        source_intent: None,
        retrieval_plan: json!({}),
        retrieval_evidence: json!({}),
        evidence_decision: json!({}),
        selected_capability: None,
        selected_tool: None,
        tool_params: json!({}),
        policy_decision: json!({}),
        execution_summary: json!({}),
        structured_response: None,
        planner_snapshot_id: None,
        warnings: json!([]),
        revision: 0,
    };
    let result = AssistantGraphRuntime::run(
        memory,
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
        },
    );
    assert_eq!(result.transitions.len(), 3);
    assert_eq!(
        result.memory.terminal_state,
        Some(TerminalState::WaitingForUserInput)
    );
    assert!(result.memory.structured_response.is_some());
}

#[test]
fn preserves_limit_from_pending_clarification_context() {
    let intent = pending_clarification_intent(&ContextWindow {
        summary: None,
        active_domain: Some("client".into()),
        selected_entities: json!({}),
        recent_messages: vec![crate::assistant::ContextMessage {
            role: "user".into(),
            content: "show 10 clients with the most savings accounts".into(),
            created_at: None,
        }],
        relevant_jobs: Vec::new(),
        pending_clarification: None,
        source_intent: None,
        source_snippets: Vec::new(),
        client_scope: json!({}),
        warnings: Vec::new(),
    });

    assert_eq!(
        intent.constraints.quantity,
        Some(Quantity::TopN { value: 10 })
    );
}

#[test]
fn preserves_limit_when_source_intent_quantity_defaults() {
    let context = ContextWindow {
        summary: None,
        active_domain: Some("client".into()),
        selected_entities: json!({}),
        recent_messages: vec![crate::assistant::ContextMessage {
            role: "user".into(),
            content: "show 10 clients with the most savings accounts".into(),
            created_at: None,
        }],
        relevant_jobs: Vec::new(),
        pending_clarification: None,
        source_intent: None,
        source_snippets: Vec::new(),
        client_scope: json!({}),
        warnings: Vec::new(),
    };
    let payload = ClarificationPayload {
        question: "Which report?".into(),
        options: Vec::new(),
        attempt: 1,
        source_intent: Some(SourceIntentSnapshot {
            prompt: "show 10 clients with the most savings accounts".into(),
            normalized_prompt: None,
            intent: AssistantIntentKind::ReportRequest,
            domain: AssistantDomain::Client,
            request_shape: Default::default(),
            entities: Vec::new(),
            constraints: crate::assistant::AssistantConstraints {
                quantity: Some(Quantity::Default),
                ..Default::default()
            },
            context_reference: ContextReference::None,
            confidence: 1.0,
            reason: "test".into(),
        }),
        allow_free_text: false,
        is_missing_execution_parameters: false,
    };

    let intent = intent_from_source(&payload, &context, None);

    assert_eq!(
        intent.constraints.quantity,
        Some(Quantity::TopN { value: 10 })
    );
}

#[test]
fn preserves_limit_from_direct_report_intent() {
    let mut memory = empty_memory();
    let mut intent = AssistantIntent {
        intent: AssistantIntentKind::ReportRequest,
        domain: AssistantDomain::Client,
        request_shape: Default::default(),
        language: AssistantLanguage::En,
        entities: Vec::new(),
        constraints: Default::default(),
        context_reference: ContextReference::None,
        source: None,
        confidence: 1.0,
        reason: "test".into(),
    };

    merge_deterministic_extraction(
        &mut memory,
        &mut intent,
        "show 10 clients with the most savings accounts",
    );
    let plan = RetrievalPlan::new(
        "show 10 clients with the most savings accounts",
        &intent,
        true,
        Vec::new(),
    );

    assert_eq!(
        intent.constraints.quantity,
        Some(Quantity::TopN { value: 10 })
    );
    assert_eq!(
        plan.constraints["quantity"],
        json!({ "mode": "top_n", "value": 10 })
    );
    assert_eq!(
        intent.constraints.metric.as_deref(),
        Some("savings_account_count")
    );
    assert!(
        intent
            .entities
            .iter()
            .any(|entity| entity.entity_type == crate::assistant::AssistantEntityType::Metric)
    );
    assert!(memory.current_user_message_metadata["deterministic_extraction"].is_object());
}

#[test]
fn preserves_explicit_quantity_from_direct_report_intent() {
    let mut memory = empty_memory();
    let mut intent = AssistantIntent {
        intent: AssistantIntentKind::ReportRequest,
        domain: AssistantDomain::Client,
        request_shape: Default::default(),
        language: AssistantLanguage::En,
        entities: Vec::new(),
        constraints: crate::assistant::AssistantConstraints {
            quantity: Some(Quantity::Limit { value: 5 }),
            ..Default::default()
        },
        context_reference: ContextReference::None,
        source: None,
        confidence: 1.0,
        reason: "test".into(),
    };

    merge_deterministic_extraction(
        &mut memory,
        &mut intent,
        "show 10 clients with the most savings accounts",
    );

    assert_eq!(
        intent.constraints.quantity,
        Some(Quantity::TopN { value: 10 })
    );
}

#[test]
fn records_deterministic_conflicts_before_merge() {
    let mut memory = empty_memory();
    let mut intent = AssistantIntent {
        intent: AssistantIntentKind::ReportRequest,
        domain: AssistantDomain::Client,
        request_shape: Default::default(),
        language: AssistantLanguage::En,
        entities: Vec::new(),
        constraints: crate::assistant::AssistantConstraints {
            quantity: Some(Quantity::Limit { value: 20 }),
            currency_code: Some("USD".into()),
            ..Default::default()
        },
        context_reference: ContextReference::None,
        source: None,
        confidence: 1.0,
        reason: "test".into(),
    };

    merge_deterministic_extraction(&mut memory, &mut intent, "show 10 clients in IDR");

    assert_eq!(
        intent.constraints.quantity,
        Some(Quantity::Limit { value: 10 })
    );
    assert_eq!(intent.constraints.currency_code.as_deref(), Some("IDR"));
    assert_eq!(
        memory.current_user_message_metadata["deterministic_extraction_conflicts"],
        json!([
            {
                "field": "limit",
                "llm_value": { "mode": "limit", "value": 20 },
                "trusted_value": { "mode": "limit", "value": 10 },
                "reason": "deterministic_extraction_preferred"
            },
            {
                "field": "currency_code",
                "llm_value": "USD",
                "trusted_value": "IDR",
                "reason": "deterministic_extraction_preferred"
            }
        ])
    );
}

struct FakeLlm;

#[async_trait]
impl LlmClient for FakeLlm {
    async fn structured_value(
        &self,
        _purpose: crate::assistant::llm::LlmPurpose,
        _system: &str,
        _user: &str,
        _schema: serde_json::Value,
    ) -> Result<LlmResponse<serde_json::Value>> {
        Ok(LlmResponse {
            value: json!({
                "intent": AssistantIntentKind::ReportRequest,
                "domain": AssistantDomain::Savings,
                "language": AssistantLanguage::En,
                "entities": [],
                "constraints": {},
                "context_reference": ContextReference::None,
                "confidence": 0.9,
                "reason": "fake"
            }),
            usage: TokenUsage::default(),
            cost_usd: None,
            provider: "fake".into(),
            model: "fake".into(),
            latency_ms: 0,
        })
    }

    async fn embed(
        &self,
        _purpose: crate::assistant::llm::LlmPurpose,
        _text: &str,
    ) -> Result<EmbeddingResponse> {
        Ok(EmbeddingResponse {
            vector: vec![1.0, 0.0],
            usage: TokenUsage::default(),
            cost_usd: None,
            provider: "fake".into(),
            model: "fake".into(),
            latency_ms: 0,
        })
    }
}

#[tokio::test]
async fn route_retrieval_evidence_without_repository_is_unsupported_without_catalog_evidence() {
    let memory = JobMemory {
        job_id: Uuid::nil(),
        graph_state: "receive_message".into(),
        terminal_state: None,
        current_user_message_metadata: json!({}),
        intent: None,
        source_intent: None,
        retrieval_plan: json!({}),
        retrieval_evidence: json!([{ "capability_id": "client_top_n_by_savings_balance" }]),
        evidence_decision: json!({}),
        selected_capability: None,
        selected_tool: None,
        tool_params: json!({}),
        policy_decision: json!({}),
        execution_summary: json!({}),
        structured_response: None,
        planner_snapshot_id: None,
        warnings: json!([]),
        revision: 0,
    };
    let context = ContextWindow {
        summary: None,
        active_domain: None,
        selected_entities: json!({}),
        recent_messages: Vec::new(),
        relevant_jobs: Vec::new(),
        pending_clarification: None,
        source_intent: None,
        source_snippets: Vec::new(),
        client_scope: json!({ "capabilities": ["savings_deposit_total"] }),
        warnings: Vec::new(),
    };
    let catalog = KnowledgeCatalog {
        root_path: Default::default(),
        query_path: Default::default(),
        data_areas: Vec::new(),
        domains: Vec::new(),
        schemas: Vec::new(),
        metrics: Vec::new(),
        capabilities: Vec::new(),
        queries: Vec::new(),
        policies: Vec::new(),
        responses: Vec::new(),
        classification: Default::default(),
    };
    let llm = Arc::new(FakeLlm) as SharedLlmClient;
    let router = SemanticRouter::new(llm.clone(), &catalog);

    let result = AssistantGraphRuntime::run_with_router(
        memory,
        context,
        Some(&router),
        Some(&llm),
        None,
        None,
        None,
        None,
        None,
        "show savings",
    )
    .await;

    assert_eq!(
        result.memory.terminal_state,
        Some(TerminalState::Unsupported)
    );
    assert_eq!(result.transitions.len(), 7);
    assert_eq!(result.memory.graph_state, "complete_or_wait");
}

#[tokio::test]
async fn semantic_router_unavailable_fails_closed() {
    let memory = JobMemory {
        job_id: Uuid::nil(),
        graph_state: "receive_message".into(),
        terminal_state: None,
        current_user_message_metadata: json!({}),
        intent: None,
        source_intent: None,
        retrieval_plan: json!({}),
        retrieval_evidence: json!({}),
        evidence_decision: json!({}),
        selected_capability: None,
        selected_tool: None,
        tool_params: json!({}),
        policy_decision: json!({}),
        execution_summary: json!({}),
        structured_response: None,
        planner_snapshot_id: None,
        warnings: json!([]),
        revision: 0,
    };
    let context = ContextWindow {
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
    };

    let result = AssistantGraphRuntime::run_with_router(
        memory,
        context,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        "show client savings",
    )
    .await;

    assert_eq!(
        result.memory.terminal_state,
        Some(TerminalState::FailedOperational)
    );
    assert_eq!(result.pending_clarification, None);
    assert_eq!(
        result.memory.structured_response.unwrap().response_type,
        AssistantResponseType::Error
    );
}

#[tokio::test]
async fn greeting_completes_without_router() {
    let result = AssistantGraphRuntime::run_with_router(
        empty_memory(),
        empty_context(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        "Hi",
    )
    .await;

    assert_eq!(result.memory.terminal_state, Some(TerminalState::Completed));
    assert_eq!(
        result.memory.structured_response.unwrap().title.as_deref(),
        Some("Hello")
    );
}

#[tokio::test]
async fn exact_pending_option_id_resolves_before_router() {
    let memory = JobMemory {
        job_id: Uuid::nil(),
        graph_state: "receive_message".into(),
        terminal_state: None,
        current_user_message_metadata: json!({}),
        intent: None,
        source_intent: None,
        retrieval_plan: json!({}),
        retrieval_evidence: json!([{ "capability_id": "client_top_n_by_savings_balance" }]),
        evidence_decision: json!({}),
        selected_capability: None,
        selected_tool: None,
        tool_params: json!({}),
        policy_decision: json!({}),
        execution_summary: json!({}),
        structured_response: None,
        planner_snapshot_id: None,
        warnings: json!([]),
        revision: 0,
    };
    let context = ContextWindow {
        summary: None,
        active_domain: Some("client".into()),
        selected_entities: json!({}),
        recent_messages: Vec::new(),
        relevant_jobs: Vec::new(),
        pending_clarification: Some(ClarificationPayload {
            question: "Which report?".into(),
            options: vec![
                ClarificationOption {
                    id: "client_top_n_by_deposit_volume".into(),
                    label: "Top clients by deposit volume".into(),
                    description: None,
                },
                ClarificationOption {
                    id: "client_top_n_by_savings_balance".into(),
                    label: "Top clients by savings balance".into(),
                    description: None,
                },
            ],
            attempt: 1,
            source_intent: Some(SourceIntentSnapshot {
                prompt: "show 10 clients in USD".into(),
                normalized_prompt: None,
                intent: AssistantIntentKind::ReportRequest,
                domain: AssistantDomain::Client,
                request_shape: Default::default(),
                entities: vec![crate::assistant::AssistantEntity {
                    entity_type: crate::assistant::AssistantEntityType::Currency,
                    value: "USD".into(),
                    canonical: Some("USD".into()),
                    confidence: Some(1.0),
                }],
                constraints: crate::assistant::AssistantConstraints {
                    quantity: Some(Quantity::TopN { value: 10 }),
                    currency_code: Some("USD".into()),
                    ..Default::default()
                },
                context_reference: ContextReference::None,
                confidence: 0.9,
                reason: "test".into(),
            }),
            allow_free_text: true,
            is_missing_execution_parameters: false,
        }),
        source_intent: None,
        source_snippets: Vec::new(),
        client_scope: json!({}),
        warnings: Vec::new(),
    };

    let result = AssistantGraphRuntime::run_with_router(
        memory,
        context,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        RuntimeUserInput {
            message: "client_top_n_by_savings_balance".into(),
            source_message: "please use the balance option".into(),
            selected_option_id: Some("client_top_n_by_savings_balance".into()),
        },
    )
    .await;

    assert_eq!(
        result.memory.selected_capability.as_deref(),
        Some("client_top_n_by_savings_balance")
    );
    assert_eq!(result.pending_clarification, Some(None));
    assert_eq!(
        result.memory.intent.as_ref().unwrap().constraints.quantity,
        Some(Quantity::TopN { value: 10 })
    );
    assert_eq!(
        result
            .memory
            .intent
            .as_ref()
            .unwrap()
            .constraints
            .currency_code
            .as_deref(),
        Some("USD")
    );
    assert_eq!(
        result.memory.retrieval_evidence["source_message"],
        "please use the balance option"
    );
    assert_eq!(
        result.memory.retrieval_evidence["source"],
        "explicit_option_id"
    );
    assert_ne!(
        result.memory.structured_response.unwrap().response_type,
        AssistantResponseType::Clarification
    );
}

#[tokio::test]
async fn invalid_pending_option_id_is_rejected_before_router() {
    let memory = JobMemory {
        job_id: Uuid::nil(),
        graph_state: "receive_message".into(),
        terminal_state: None,
        current_user_message_metadata: json!({}),
        intent: None,
        source_intent: None,
        retrieval_plan: json!({}),
        retrieval_evidence: json!([{ "capability_id": "client_top_n_by_savings_balance" }]),
        evidence_decision: json!({}),
        selected_capability: None,
        selected_tool: None,
        tool_params: json!({}),
        policy_decision: json!({}),
        execution_summary: json!({}),
        structured_response: None,
        planner_snapshot_id: None,
        warnings: json!([]),
        revision: 0,
    };
    let context = ContextWindow {
        summary: None,
        active_domain: Some("client".into()),
        selected_entities: json!({}),
        recent_messages: Vec::new(),
        relevant_jobs: Vec::new(),
        pending_clarification: Some(ClarificationPayload {
            question: "Which report?".into(),
            options: vec![ClarificationOption {
                id: "client_top_n_by_deposit_volume".into(),
                label: "Top clients by deposit volume".into(),
                description: None,
            }],
            attempt: 1,
            source_intent: None,
            allow_free_text: true,
            is_missing_execution_parameters: false,
        }),
        source_intent: None,
        source_snippets: Vec::new(),
        client_scope: json!({ "allow_all_capabilities": true }),
        warnings: Vec::new(),
    };

    let result = AssistantGraphRuntime::run_with_router(
        memory,
        context,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        RuntimeUserInput {
            message: "client_summary".into(),
            source_message: "client summary".into(),
            selected_option_id: Some("client_summary".into()),
        },
    )
    .await;

    assert_eq!(result.memory.selected_capability, None);
    assert_eq!(
        result.memory.terminal_state,
        Some(TerminalState::WaitingForUserInput)
    );
    assert_eq!(result.memory.graph_state, "complete_or_wait");
    assert_eq!(
        result.memory.retrieval_evidence["clarification_outcome"],
        "unresolved"
    );
}

#[test]
fn clarification_payload_always_includes_others_option() {
    let payload = clarification_payload(&test_plan(), &[], None);
    assert!(
        payload.options.iter().any(|option| {
            option.id == OTHER_CLARIFICATION_OPTION_ID && option.label == "Others"
        })
    );
}

#[test]
fn clarification_payload_empty_evidence_uses_real_capabilities() {
    let payload = clarification_payload(&test_plan(), &[], None);

    assert!(
        payload
            .options
            .iter()
            .any(|option| option.id == "client_top_n_by_savings_account_count")
    );
    assert!(
        !payload
            .options
            .iter()
            .any(|option| option.id == "refine_metric")
    );
}

fn test_plan() -> RetrievalPlan {
    RetrievalPlan::new(
        "show savings",
        &AssistantIntent {
            intent: AssistantIntentKind::ReportRequest,
            domain: AssistantDomain::Savings,
            request_shape: Default::default(),
            language: AssistantLanguage::En,
            entities: Vec::new(),
            constraints: Default::default(),
            context_reference: ContextReference::None,
            source: None,
            confidence: 0.9,
            reason: "test".into(),
        },
        true,
        Vec::new(),
    )
}

#[test]
fn allow_all_scope_is_preserved_with_empty_capabilities() {
    let context = ContextWindow {
        summary: None,
        active_domain: None,
        selected_entities: json!({}),
        recent_messages: Vec::new(),
        relevant_jobs: Vec::new(),
        pending_clarification: None,
        source_intent: None,
        source_snippets: Vec::new(),
        client_scope: json!({ "allow_all_capabilities": true, "capabilities": [] }),
        warnings: Vec::new(),
    };

    assert!(allow_all_capabilities(&context));
    assert!(allowed_capabilities(&context).is_empty());
}

fn sample_candidate(
    field: crate::assistant::understanding::extraction::PayloadField,
) -> crate::assistant::understanding::extraction::PayloadCandidate {
    crate::assistant::understanding::extraction::PayloadCandidate {
        field,
        value: json!("sample"),
        source: crate::assistant::understanding::extraction::PayloadSource::UserText,
        trust: crate::assistant::understanding::extraction::PayloadTrust::Trusted,
    }
}

#[test]
fn prefer_current_turn_extraction_falls_back_to_source_candidates_when_current_empty() {
    let source = DeterministicExtraction {
        candidates: vec![sample_candidate(
            crate::assistant::understanding::extraction::PayloadField::Metric,
        )],
        ..Default::default()
    };
    let current = DeterministicExtraction::default();

    let merged = prefer_current_turn_extraction(source.clone(), current);

    assert_eq!(merged.candidates, source.candidates);
}

#[test]
fn prefer_current_turn_extraction_keeps_current_candidates_when_present() {
    let source = DeterministicExtraction {
        candidates: vec![sample_candidate(
            crate::assistant::understanding::extraction::PayloadField::Metric,
        )],
        ..Default::default()
    };
    let current = DeterministicExtraction {
        candidates: vec![sample_candidate(
            crate::assistant::understanding::extraction::PayloadField::Limit,
        )],
        ..Default::default()
    };

    let merged = prefer_current_turn_extraction(source, current.clone());

    assert_eq!(merged.candidates, current.candidates);
}
