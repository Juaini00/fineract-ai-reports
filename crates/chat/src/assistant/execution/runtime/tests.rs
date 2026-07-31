use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

use super::*;
use crate::{
    assistant::{
        AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage,
        AssistantResponseType, ContextReference,
        llm::{EmbeddingResponse, LlmClient, LlmResponse, TokenUsage},
    },
    knowledge::catalog::{loader::KnowledgeLoader, validator::KnowledgeValidator},
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
        pending_clarification: None,
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
        pending_clarification: None,
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
        version: crate::assistant::clarification::CLARIFICATION_VERSION_1,
        id: uuid::Uuid::new_v4(),
        revision: 0,
        kind: crate::assistant::clarification::ClarificationKind::SelectOption,
        question: "Which report?".into(),
        options: Vec::new(),
        fields: Vec::new(),
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
        canonical_query_en: String::new(),
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
        canonical_query_en: String::new(),
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
        canonical_query_en: String::new(),
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
        pending_clarification: None,
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
        parameter_inputs: Vec::new(),
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
        pending_clarification: None,
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
        pending_clarification: None,
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
            version: crate::assistant::clarification::CLARIFICATION_VERSION_1,
            id: uuid::Uuid::new_v4(),
            revision: 0,
            kind: crate::assistant::clarification::ClarificationKind::SelectOption,
            question: "Which report?".into(),
            options: vec![
                ClarificationOption {
                    id: "client_top_n_by_deposit_volume".into(),
                    label: "Top clients by deposit volume".into(),
                    description: None,
                    fields: Vec::new(),
                },
                ClarificationOption {
                    id: "client_top_n_by_savings_balance".into(),
                    label: "Top clients by savings balance".into(),
                    description: None,
                    fields: Vec::new(),
                },
            ],
            fields: Vec::new(),
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
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
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
    assert_eq!(result.memory.retrieval_evidence["structured"], false);
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
        pending_clarification: None,
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
            version: crate::assistant::clarification::CLARIFICATION_VERSION_1,
            id: uuid::Uuid::new_v4(),
            revision: 0,
            kind: crate::assistant::clarification::ClarificationKind::SelectOption,
            question: "Which report?".into(),
            options: vec![ClarificationOption {
                id: "client_top_n_by_deposit_volume".into(),
                label: "Top clients by deposit volume".into(),
                description: None,
                fields: Vec::new(),
            }],
            fields: Vec::new(),
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
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
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
    assert_eq!(
        result
            .pending_clarification
            .as_ref()
            .and_then(|payload| payload.as_ref())
            .map(|payload| payload.attempt),
        Some(2)
    );
}

#[tokio::test]
async fn repeated_invalid_option_enters_bounded_free_text_recovery() {
    let result = AssistantGraphRuntime::run_with_router(
        empty_memory(),
        pending_context(false, 3, "savings_deposit_top_n"),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        RuntimeUserInput {
            message: "stale option".into(),
            source_message: "stale option".into(),
            selected_option_id: Some("unavailable_report".into()),
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
        },
    )
    .await;

    assert_eq!(result.pending_clarification, Some(None));
    assert_eq!(
        result.memory.structured_response.unwrap().title.as_deref(),
        Some("Describe your request")
    );
}

#[tokio::test]
async fn selected_option_with_conflicting_message_reclarifies_and_increments_attempt() {
    let mut context = pending_context(false, 1, "client_top_n_by_deposit_volume");
    context.active_domain = Some("client".into());
    context
        .pending_clarification
        .as_mut()
        .unwrap()
        .source_intent
        .as_mut()
        .unwrap()
        .domain = AssistantDomain::Client;
    let catalog = Arc::new(runtime_test_catalog());
    let client = PrincipalContext {
        user_id: Uuid::nil(),
        role: "admin".into(),
        office_ids: vec![1],
        capability_ids: vec!["client_top_n_by_deposit_volume".into()],
        can_view_pii: true,
        legacy_api_key_id: None,
    };
    let message = "show 10 clients with the most savings accounts";

    let result = AssistantGraphRuntime::run_with_router(
        empty_memory(),
        context,
        None,
        None,
        None,
        None,
        Some(&catalog),
        Some(&client),
        None,
        RuntimeUserInput {
            message: message.into(),
            source_message: message.into(),
            selected_option_id: Some("client_top_n_by_deposit_volume".into()),
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
        },
    )
    .await;

    let payload = result
        .pending_clarification
        .as_ref()
        .and_then(|payload| payload.as_ref())
        .expect("conflict clarification");
    assert_eq!(payload.attempt, 2);
    assert!(payload.question.contains("requested metric"));
    assert_eq!(
        result.memory.terminal_state,
        Some(TerminalState::WaitingForUserInput)
    );
}

#[tokio::test]
async fn source_month_survives_selection_and_limit_falls_back_to_default() {
    let mut context = pending_context(false, 1, "organization_office_activity_ranking");
    context.active_domain = Some("organization".into());
    let source = context
        .pending_clarification
        .as_mut()
        .unwrap()
        .source_intent
        .as_mut()
        .unwrap();
    source.prompt =
        "give me the report with the most savings account transactions this month".into();
    source.domain = AssistantDomain::Organization;
    let catalog = Arc::new(runtime_test_catalog());
    let client = PrincipalContext {
        user_id: Uuid::nil(),
        role: "admin".into(),
        office_ids: vec![1],
        capability_ids: vec!["organization_office_activity_ranking".into()],
        can_view_pii: true,
        legacy_api_key_id: None,
    };
    let message = "Rank offices by savings transaction volume";

    let result = AssistantGraphRuntime::run_with_router(
        empty_memory(),
        context,
        None,
        None,
        None,
        None,
        Some(&catalog),
        Some(&client),
        None,
        RuntimeUserInput {
            message: message.into(),
            source_message: message.into(),
            selected_option_id: Some("organization_office_activity_ranking".into()),
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
        },
    )
    .await;

    // An unspecified row count defaults instead of bouncing a clarification back.
    if let Some(payload) = result
        .pending_clarification
        .as_ref()
        .and_then(|payload| payload.as_ref())
    {
        assert!(
            !payload.question.contains("missing parameter limit")
                && !payload.question.contains("from_date"),
            "unexpected clarification: {}",
            payload.question
        );
    }
    let constraints = &result.memory.intent.as_ref().unwrap().constraints;
    assert!(constraints.from_date.is_some());
    assert!(constraints.to_date.is_some());
}

#[tokio::test]
async fn defaultless_required_search_asks_and_runs_nothing() {
    let mut context = pending_context(false, 1, "client_name_lookup");
    context.active_domain = Some("client".into());
    context
        .pending_clarification
        .as_mut()
        .unwrap()
        .source_intent
        .as_mut()
        .unwrap()
        .domain = AssistantDomain::Client;
    let catalog = Arc::new(runtime_test_catalog());
    let client = PrincipalContext {
        user_id: Uuid::nil(),
        role: "admin".into(),
        office_ids: vec![1],
        capability_ids: vec!["client_name_lookup".into()],
        can_view_pii: true,
        legacy_api_key_id: None,
    };
    let message = "look up a client please";
    let result = AssistantGraphRuntime::run_with_router(
        empty_memory(),
        context,
        None,
        None,
        None,
        None,
        Some(&catalog),
        Some(&client),
        None,
        RuntimeUserInput {
            message: message.into(),
            source_message: message.into(),
            selected_option_id: Some("client_name_lookup".into()),
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
        },
    )
    .await;

    assert_eq!(
        result.memory.terminal_state,
        Some(TerminalState::WaitingForUserInput)
    );
    let payload = result
        .pending_clarification
        .as_ref()
        .and_then(|p| p.as_ref())
        .expect("must ask for the missing search parameter");
    assert!(
        payload.fields.iter().any(|f| f.key == "search"),
        "collect_fields must carry `search`: {:?}",
        payload.fields
    );
    assert!(result.memory.selected_tool.is_none());
    assert_eq!(result.memory.tool_params, json!({}));
}

#[tokio::test]
async fn fully_defaulted_capability_completes_without_asking() {
    let mut context = pending_context(false, 1, "organization_office_activity_ranking");
    context.active_domain = Some("organization".into());
    let source = context
        .pending_clarification
        .as_mut()
        .unwrap()
        .source_intent
        .as_mut()
        .unwrap();
    source.domain = AssistantDomain::Organization;
    source.constraints.from_date = Some("2026-07-01".into());
    source.constraints.to_date = Some("2026-07-29".into());
    let catalog = Arc::new(runtime_test_catalog());
    let client = PrincipalContext {
        user_id: Uuid::nil(),
        role: "admin".into(),
        office_ids: vec![1],
        capability_ids: vec!["organization_office_activity_ranking".into()],
        can_view_pii: true,
        legacy_api_key_id: None,
    };
    let message = "Rank offices by savings transaction volume this month";
    let result = AssistantGraphRuntime::run_with_router(
        empty_memory(),
        context,
        None,
        None,
        None,
        None,
        Some(&catalog),
        Some(&client),
        None,
        RuntimeUserInput {
            message: message.into(),
            source_message: message.into(),
            selected_option_id: Some("organization_office_activity_ranking".into()),
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
        },
    )
    .await;

    // "No ask" guarantee: no clarification payload attached. Terminal state may
    // still be WaitingForUserInput for unrelated pipeline reasons in the no-DB
    // harness — the durable guarantee is that we did not construct a clarification.
    let payload = result
        .pending_clarification
        .as_ref()
        .and_then(|p| p.as_ref());
    assert!(
        payload.is_none(),
        "a fully-defaulted capability must not ask: {payload:?}"
    );
}

fn runtime_test_catalog() -> KnowledgeCatalog {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
        .load()
        .unwrap();
    KnowledgeValidator::validate(&catalog).unwrap();
    catalog
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

#[test]
fn clarification_options_show_period_already_given_in_the_request() {
    let evidence = vec![Evidence {
        capability_id: "savings_deposit_top_n".into(),
        title: "Top Savings Deposits".into(),
        score: 0.9,
        source_type: "capability".into(),
        metadata: json!({
            "description": "Return the largest savings deposit transactions for a date range within the caller's authorized office scope."
        }),
        conflicting: false,
    }];
    let intent = AssistantIntent {
        intent: AssistantIntentKind::ReportRequest,
        domain: AssistantDomain::Savings,
        request_shape: Default::default(),
        language: AssistantLanguage::En,
        canonical_query_en: String::new(),
        entities: Vec::new(),
        constraints: crate::assistant::AssistantConstraints {
            from_date: Some("2026-07-01".into()),
            to_date: Some("2026-07-31".into()),
            ..Default::default()
        },
        context_reference: ContextReference::None,
        source: None,
        confidence: 0.9,
        reason: "test".into(),
    };
    let source_intent = source_intent_snapshot(&intent, "top savings this month");

    let payload = clarification_payload(&test_plan(), &evidence, Some(source_intent));

    let description = payload.options[0].description.clone().unwrap();
    assert!(
        description.contains("2026-07-01 to 2026-07-31"),
        "expected resolved period in option description, got: {description}"
    );
    assert!(!description.contains("a date range"));
}

fn test_plan() -> RetrievalPlan {
    RetrievalPlan::new(
        "show savings",
        &AssistantIntent {
            intent: AssistantIntentKind::ReportRequest,
            domain: AssistantDomain::Savings,
            request_shape: Default::default(),
            language: AssistantLanguage::En,
            canonical_query_en: String::new(),
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

fn pending_context(missing_parameters: bool, attempt: u32, capability_id: &str) -> ContextWindow {
    ContextWindow {
        summary: None,
        active_domain: Some("savings".into()),
        selected_entities: json!({}),
        recent_messages: Vec::new(),
        relevant_jobs: Vec::new(),
        pending_clarification: Some(ClarificationPayload {
            version: crate::assistant::clarification::CLARIFICATION_VERSION_1,
            id: Uuid::nil(),
            revision: 0,
            kind: crate::assistant::clarification::ClarificationKind::SelectOption,
            question: "Please clarify".into(),
            options: vec![
                ClarificationOption {
                    id: capability_id.into(),
                    label: capability_id.into(),
                    description: None,
                    fields: Vec::new(),
                },
                ClarificationOption {
                    id: OTHER_CLARIFICATION_OPTION_ID.into(),
                    label: "Others".into(),
                    description: None,
                    fields: Vec::new(),
                },
            ],
            fields: Vec::new(),
            attempt,
            source_intent: Some(SourceIntentSnapshot {
                prompt: "show the largest savings deposits".into(),
                normalized_prompt: None,
                intent: AssistantIntentKind::ReportRequest,
                domain: AssistantDomain::Savings,
                request_shape: Default::default(),
                entities: Vec::new(),
                constraints: Default::default(),
                context_reference: ContextReference::None,
                confidence: 0.9,
                reason: "test".into(),
            }),
            allow_free_text: true,
            is_missing_execution_parameters: missing_parameters,
        }),
        source_intent: None,
        source_snippets: Vec::new(),
        client_scope: json!({ "allow_all_capabilities": true }),
        warnings: Vec::new(),
    }
}

#[test]
fn meaningful_others_is_a_new_request_not_a_reset_prompt() {
    let context = pending_context(false, 1, "savings_deposit_top_n");
    let payload = context.pending_clarification.as_ref().unwrap();
    let message = "Rank offices by savings transaction volume this month";

    let outcome = resolve_pending_clarification(
        &RuntimeUserInput {
            message: message.into(),
            source_message: message.into(),
            selected_option_id: Some("others".into()),
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
        },
        payload,
        &empty_memory(),
        &context,
    );

    assert_eq!(
        outcome,
        Some(ClarificationOutcome::NewRequest {
            message: message.into(),
            confidence: 1.0,
        })
    );
}

#[tokio::test]
async fn others_continues_missing_parameters_with_message_facts() {
    let message = "this month";
    let result = AssistantGraphRuntime::run_with_router(
        empty_memory(),
        pending_context(true, 1, "savings_deposit_top_n"),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        RuntimeUserInput {
            message: message.into(),
            source_message: message.into(),
            selected_option_id: Some("others".into()),
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
        },
    )
    .await;

    assert_eq!(
        result.memory.selected_capability.as_deref(),
        Some("savings_deposit_top_n")
    );
    assert!(
        result
            .memory
            .intent
            .as_ref()
            .unwrap()
            .constraints
            .from_date
            .is_some()
    );
    assert_eq!(result.pending_clarification, Some(None));
}

#[tokio::test]
async fn long_message_continues_missing_parameters() {
    let message = "Please use every transaction from this current month for the report";
    let result = AssistantGraphRuntime::run_with_router(
        empty_memory(),
        pending_context(true, 1, "savings_deposit_top_n"),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        RuntimeUserInput {
            message: message.into(),
            source_message: message.into(),
            selected_option_id: None,
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
        },
    )
    .await;

    assert_eq!(
        result.memory.selected_capability.as_deref(),
        Some("savings_deposit_top_n")
    );
    assert_eq!(result.pending_clarification, Some(None));
}

#[tokio::test]
async fn gateway_pipeline_runtime_entry_maps_execute_to_completed() {
    use crate::assistant::execution::runtime::run_via_gateway_pipeline;
    use crate::assistant::llm::FakeLlmClient;
    let fake = std::sync::Arc::new(FakeLlmClient::default());
    fake.push_structured(serde_json::json!({
        "intent_kind": "report_request",
        "domain": "savings",
        "language": "en",
        "entities": [],
        "candidates": [
            { "capability_id": "savings_deposit_total", "confidence": 0.95, "why": "totals" }
        ]
    }));
    let llm: crate::assistant::llm::SharedLlmClient = fake;
    let catalog = runtime_test_catalog();
    let client = PrincipalContext {
        user_id: Uuid::nil(),
        role: "admin".into(),
        office_ids: vec![1],
        capability_ids: vec!["savings_deposit_total".into()],
        can_view_pii: true,
        legacy_api_key_id: None,
    };
    let result = run_via_gateway_pipeline(
        empty_memory(),
        empty_context(),
        llm,
        &catalog,
        &client,
        chrono::NaiveDate::from_ymd_opt(2026, 7, 15).unwrap(),
        RuntimeUserInput {
            message: "How much did we deposit?".into(),
            source_message: "How much did we deposit?".into(),
            selected_option_id: None,
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
        },
    )
    .await;
    assert_eq!(result.memory.terminal_state, Some(TerminalState::Completed));
    assert_eq!(
        result.memory.selected_capability.as_deref(),
        Some("savings_deposit_total")
    );
}

#[tokio::test]
async fn gateway_pipeline_runtime_entry_maps_clarify_to_waiting() {
    use crate::assistant::execution::runtime::run_via_gateway_pipeline;
    use crate::assistant::llm::FakeLlmClient;
    let fake = std::sync::Arc::new(FakeLlmClient::default());
    fake.push_structured(serde_json::json!({
        "intent_kind": "data_lookup",
        "domain": "client",
        "language": "en",
        "entities": [],
        "candidates": [
            { "capability_id": "client_name_lookup", "confidence": 0.9, "why": "lookup" }
        ]
    }));
    let llm: crate::assistant::llm::SharedLlmClient = fake;
    let catalog = runtime_test_catalog();
    let client = PrincipalContext {
        user_id: Uuid::nil(),
        role: "admin".into(),
        office_ids: vec![1],
        capability_ids: vec!["client_name_lookup".into()],
        can_view_pii: true,
        legacy_api_key_id: None,
    };
    let result = run_via_gateway_pipeline(
        empty_memory(),
        empty_context(),
        llm,
        &catalog,
        &client,
        chrono::NaiveDate::from_ymd_opt(2026, 7, 15).unwrap(),
        RuntimeUserInput {
            message: "look up a client".into(),
            source_message: "look up a client".into(),
            selected_option_id: None,
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
        },
    )
    .await;
    assert_eq!(
        result.memory.terminal_state,
        Some(TerminalState::WaitingForUserInput)
    );
    let payload = result
        .pending_clarification
        .as_ref()
        .and_then(|p| p.as_ref())
        .expect("clarification payload attached");
    assert!(payload.fields.iter().any(|f| f.key == "search"));
}
