use serde_json::json;
use std::sync::Arc;

use app_core::auth::model::PrincipalContext;
use app_core::config::CanonicalGatewayMode;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::assistant::tool::{normalize_effective_parameters, plan_from_snapshot};
use crate::assistant::{
    AssistantConstraints, AssistantDomain, AssistantGraphTopology, AssistantIntent,
    AssistantIntentKind, AssistantLanguage, AssistantResponse, AssistantResponseType,
    CanonicalStateRepository, ClarificationOption, ClarificationOutcome, ClarificationPayload,
    ClarificationResolver, ContextReference, ContextWarningCode, ContextWindow,
    DeterministicExtraction, ExtractionProvenance, FactSourceKind, GraphState, GraphTransition,
    JobMemory, OTHER_CLARIFICATION_OPTION_ID, OriginalIntent, PlannerInputSnapshot,
    PrincipalProjection, Quantity, ResponseBuilder, SemanticRouter, SourceIntentSnapshot,
    TerminalState, deterministic_observations,
    evidence::{Evidence, RetrievalPlan},
    executable_constraint_contracts, extract_message_facts, extract_message_facts_at,
    llm::SharedLlmClient,
    merge_observations, original_request_observations,
    reranker::{LlmReranker, RerankerDecision, RerankerVerdict},
    response::{ResponseAction, ResponseActionType},
    response_builder::finish,
    retrieval::RetrievalEngine,
    stable_uuid,
};
use crate::chat::{executor::execute_plan, planner::PolicyDecisionStatus};
use crate::knowledge::index::repository::KnowledgeRepository;
use crate::knowledge::model::KnowledgeCatalog;

#[derive(Debug, Clone)]
pub struct GraphRuntimeResult {
    pub memory: JobMemory,
    pub transitions: Vec<GraphTransition>,
    pub pending_clarification: Option<Option<ClarificationPayload>>,
    /// Per-request retrieval audit trace (issue 06), set only on the
    /// semantic retrieval path. `None` elsewhere (deterministic responses,
    /// clarification continuations, error paths). Best-effort — the caller
    /// persists it without failing the request if the write errors.
    pub retrieval_trace: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct RuntimeUserInput {
    pub message: String,
    pub source_message: String,
    pub selected_option_id: Option<String>,
}

#[derive(Clone)]
pub struct CanonicalRuntimeContext {
    pub mode: CanonicalGatewayMode,
    pub repository: CanonicalStateRepository,
    pub catalog_version: Option<Uuid>,
    pub message_id: Uuid,
    pub observed_at: DateTime<Utc>,
    pub reference_instant: DateTime<Utc>,
    pub timezone: String,
    pub revision: i64,
    pub initial: bool,
}

impl From<&str> for RuntimeUserInput {
    fn from(message: &str) -> Self {
        Self {
            message: message.into(),
            source_message: message.into(),
            selected_option_id: None,
        }
    }
}

impl From<String> for RuntimeUserInput {
    fn from(message: String) -> Self {
        Self {
            source_message: message.clone(),
            message,
            selected_option_id: None,
        }
    }
}

pub struct AssistantGraphRuntime;

impl AssistantGraphRuntime {
    pub fn run(mut memory: JobMemory, context: ContextWindow) -> GraphRuntimeResult {
        let transitions = vec![
            GraphTransition {
                from: GraphState::ReceiveMessage,
                to: Some(GraphState::BuildContextWindow),
                terminal: None,
                reason: "message_received".into(),
            },
            GraphTransition {
                from: GraphState::BuildContextWindow,
                to: Some(GraphState::CompleteOrWait),
                terminal: None,
                reason: "context_built".into(),
            },
            GraphTransition {
                from: GraphState::CompleteOrWait,
                to: None,
                terminal: Some(TerminalState::WaitingForUserInput),
                reason: "semantic_router_unavailable".into(),
            },
        ];
        memory.graph_state = "complete_or_wait".into();
        memory.terminal_state = Some(TerminalState::WaitingForUserInput);
        memory.execution_summary = json!({
            "runtime": "semantic_assistant_graph",
            "recent_message_count": context.recent_messages.len(),
        });
        memory.structured_response = Some(AssistantResponse {
            response_type: AssistantResponseType::Clarification,
            title: Some("Assistant runtime is ready".into()),
            message: "I saved your message and built the conversation context. Semantic routing is not wired yet, so please try again after routing is enabled.".into(),
            sections: Vec::new(),
            table: None,
            cards: Vec::new(),
            options: Vec::new(),
            warnings: Vec::new(),
            actions: vec![ResponseAction { action_type: ResponseActionType::AskFollowUp, label: "Ask a follow-up".into() }],
            evidence_refs: Vec::new(),
            rendered_markdown: None,
        });
        AssistantGraphTopology::new()
            .validate_sequence(&transitions)
            .expect("assistant runtime produced illegal graph transitions");
        GraphRuntimeResult {
            memory,
            transitions,
            pending_clarification: None,
            retrieval_trace: None,
        }
    }

    pub async fn run_with_router(
        mut memory: JobMemory,
        context: ContextWindow,
        router: Option<&SemanticRouter>,
        llm: Option<&SharedLlmClient>,
        knowledge: Option<&KnowledgeRepository>,
        fineract_pool: Option<&PgPool>,
        catalog: Option<&Arc<KnowledgeCatalog>>,
        client: Option<&PrincipalContext>,
        canonical: Option<&CanonicalRuntimeContext>,
        input: impl Into<RuntimeUserInput>,
    ) -> GraphRuntimeResult {
        let input = input.into();
        let message = input.message.as_str();
        if context
            .warnings
            .iter()
            .any(|warning| warning.code == ContextWarningCode::SessionContextExceeded)
        {
            return graph_result(
                memory,
                TerminalState::ContextWindowExceeded,
                "context_window_exceeded",
                ResponseBuilder::context_window_exceeded(),
                context.recent_messages.len(),
                None,
                vec![
                    GraphTransition {
                        from: GraphState::ReceiveMessage,
                        to: Some(GraphState::BuildContextWindow),
                        terminal: None,
                        reason: "message_received".into(),
                    },
                    GraphTransition {
                        from: GraphState::BuildContextWindow,
                        to: None,
                        terminal: Some(TerminalState::ContextWindowExceeded),
                        reason: "context_window_exceeded".into(),
                    },
                ],
            );
        }
        if let Some(payload) = &context.pending_clarification
            && let Some(outcome) = resolve_pending_clarification(&input, payload, &memory, &context)
        {
            memory.intent = Some(intent_from_source(payload, &context, canonical));
            record_source_extraction_metadata(
                &mut memory,
                payload,
                canonical,
                &input.source_message,
            );
            match outcome {
                ClarificationOutcome::SelectedOption { option_id, .. } => {
                    memory.selected_capability = Some(option_id.clone());
                    memory.source_intent = payload
                        .source_intent
                        .as_ref()
                        .map(serde_json::to_value)
                        .transpose()
                        .ok()
                        .flatten();
                    memory.retrieval_evidence = clarification_audit(
                        if input.selected_option_id.is_some() {
                            "explicit_option_id"
                        } else {
                            "exact_label"
                        },
                        &option_id,
                        &input,
                        payload,
                    );
                    return execute_selected_capability(
                        memory,
                        context.recent_messages.len(),
                        option_id,
                        catalog,
                        client,
                        fineract_pool,
                        canonical,
                        Some(None),
                    )
                    .await;
                }
                ClarificationOutcome::FreeFormOther { .. } => {
                    memory.retrieval_evidence = json!({ "clarification_outcome": "free_form_other", "source_message": input.source_message, "source_intent": payload.source_intent });
                    return graph_result(
                        memory,
                        TerminalState::WaitingForUserInput,
                        "clarification_other_selected",
                        ResponseBuilder::free_form_other_prompt(),
                        context.recent_messages.len(),
                        Some(None),
                        clarification_transitions(
                            TerminalState::WaitingForUserInput,
                            "clarification_other_selected",
                        ),
                    );
                }
                ClarificationOutcome::Unresolved { .. } => {
                    memory.retrieval_evidence = json!({ "clarification_outcome": "unresolved", "source_message": input.source_message, "source_intent": payload.source_intent });
                    return graph_result(
                        memory,
                        TerminalState::WaitingForUserInput,
                        "clarification_unresolved",
                        ResponseBuilder::clarification(payload.clone()),
                        context.recent_messages.len(),
                        None,
                        clarification_transitions(
                            TerminalState::WaitingForUserInput,
                            "clarification_unresolved",
                        ),
                    );
                }
                _ => {}
            }
        }
        if let Some(payload) = &context.pending_clarification
            && payload.is_missing_execution_parameters
            && input.selected_option_id.is_none()
            && is_parameter_reply(&input.source_message)
            && let Some(capability_id) = continuation_capability(payload)
        {
            let mut intent = intent_from_source(payload, &context, canonical);
            merge_deterministic_extraction_at(
                &mut memory,
                &mut intent,
                &input.source_message,
                canonical,
            );
            memory.intent = Some(intent);
            record_source_extraction_metadata(
                &mut memory,
                payload,
                canonical,
                &input.source_message,
            );
            memory.selected_capability = Some(capability_id.clone());
            memory.retrieval_evidence =
                clarification_audit("missing_parameters", &capability_id, &input, payload);
            return execute_selected_capability(
                memory,
                context.recent_messages.len(),
                capability_id,
                catalog,
                client,
                fineract_pool,
                canonical,
                Some(None),
            )
            .await;
        }
        if let Some((intent_kind, response)) = deterministic_simple_response(message) {
            memory.intent = Some(deterministic_intent(intent_kind.clone(), message));
            return graph_result(
                memory,
                TerminalState::Completed,
                match intent_kind {
                    AssistantIntentKind::Greeting => "greeting",
                    AssistantIntentKind::Help => "help",
                    _ => "simple_intent",
                },
                response,
                context.recent_messages.len(),
                None,
                simple_intent_transitions(TerminalState::Completed, "simple_intent"),
            );
        }
        let Some(router) = router else {
            return graph_result(
                memory,
                TerminalState::FailedOperational,
                "semantic_router_unavailable",
                ResponseBuilder::error(),
                context.recent_messages.len(),
                None,
                simple_intent_transitions(
                    TerminalState::FailedOperational,
                    "semantic_router_unavailable",
                ),
            );
        };
        let route = router.route(message, &context).await;
        match &route {
            Ok(intent) => tracing::info!(
                target: "assistant::mapping",
                message = %message,
                intent = ?intent.intent,
                domain = ?intent.domain,
                request_shape = ?intent.request_shape,
                entities = ?intent.entities,
                confidence = intent.confidence,
                "router intent"
            ),
            Err(error) => tracing::warn!(
                target: "assistant::mapping",
                message = %message,
                error = %error,
                "router failed"
            ),
        }
        let mut pending_clarification = None;
        let mut retrieval_trace: Option<serde_json::Value> = None;
        let (terminal, reason, response) = match route {
            Ok(mut intent) => {
                merge_deterministic_extraction_at(
                    &mut memory,
                    &mut intent,
                    &input.source_message,
                    canonical,
                );
                if intent.intent == AssistantIntentKind::ClarificationReply
                    && let (Some(payload), Some(llm)) = (&context.pending_clarification, llm)
                {
                    let resolve_text = input
                        .selected_option_id
                        .as_deref()
                        .unwrap_or(&input.source_message);
                    match ClarificationResolver::resolve(
                        resolve_text,
                        payload,
                        &context,
                        llm.as_ref(),
                    )
                    .await
                    {
                        Ok(ClarificationOutcome::SelectedOption { option_id, .. })
                            if option_id == OTHER_CLARIFICATION_OPTION_ID =>
                        {
                            memory.intent = Some(intent_from_source(payload, &context, canonical));
                            record_source_extraction_metadata(
                                &mut memory,
                                payload,
                                canonical,
                                &input.source_message,
                            );
                            memory.retrieval_evidence = json!({ "clarification_outcome": "free_form_other", "source_message": input.source_message, "source_intent": payload.source_intent });
                            pending_clarification = Some(None);
                            return graph_result(
                                memory,
                                TerminalState::WaitingForUserInput,
                                "clarification_other_selected",
                                ResponseBuilder::free_form_other_prompt(),
                                context.recent_messages.len(),
                                pending_clarification,
                                clarification_transitions(
                                    TerminalState::WaitingForUserInput,
                                    "clarification_other_selected",
                                ),
                            );
                        }
                        Ok(ClarificationOutcome::SelectedOption { option_id, .. }) => {
                            memory.intent = Some(intent_from_source(payload, &context, canonical));
                            record_source_extraction_metadata(
                                &mut memory,
                                payload,
                                canonical,
                                &input.source_message,
                            );
                            memory.selected_capability = Some(option_id.clone());
                            memory.source_intent = payload
                                .source_intent
                                .as_ref()
                                .map(serde_json::to_value)
                                .transpose()
                                .ok()
                                .flatten();
                            memory.retrieval_evidence =
                                clarification_audit("semantic", &option_id, &input, payload);
                            pending_clarification = Some(None);
                            return execute_selected_capability(
                                memory,
                                context.recent_messages.len(),
                                option_id,
                                catalog,
                                client,
                                fineract_pool,
                                canonical,
                                pending_clarification,
                            )
                            .await;
                        }
                        Ok(ClarificationOutcome::FreeFormOther { .. }) => {
                            memory.intent = Some(intent_from_source(payload, &context, canonical));
                            record_source_extraction_metadata(
                                &mut memory,
                                payload,
                                canonical,
                                &input.source_message,
                            );
                            memory.retrieval_evidence = json!({ "clarification_outcome": "free_form_other", "source_message": input.source_message, "source_intent": payload.source_intent });
                            pending_clarification = Some(None);
                            return graph_result(
                                memory,
                                TerminalState::WaitingForUserInput,
                                "clarification_other_selected",
                                ResponseBuilder::free_form_other_prompt(),
                                context.recent_messages.len(),
                                pending_clarification,
                                clarification_transitions(
                                    TerminalState::WaitingForUserInput,
                                    "clarification_other_selected",
                                ),
                            );
                        }
                        Ok(outcome) => {
                            memory.intent = Some(intent);
                            memory.retrieval_evidence = json!({ "clarification_outcome": outcome });
                            return graph_result(
                                memory,
                                TerminalState::WaitingForUserInput,
                                "clarification_unresolved",
                                ResponseBuilder::clarification(payload.clone()),
                                context.recent_messages.len(),
                                None,
                                clarification_transitions(
                                    TerminalState::WaitingForUserInput,
                                    "clarification_unresolved",
                                ),
                            );
                        }
                        Err(error) => {
                            memory.warnings = json!([{ "message": error.to_string() }]);
                        }
                    }
                }
                let plan = RetrievalPlan::new(
                    message,
                    &intent,
                    allow_all_capabilities(&context),
                    allowed_capabilities(&context),
                );
                memory.intent = Some(intent);
                match memory.intent.as_ref().map(|intent| &intent.intent) {
                    Some(AssistantIntentKind::Greeting) => {
                        return graph_result(
                            memory,
                            TerminalState::Completed,
                            "greeting",
                            ResponseBuilder::greeting(),
                            context.recent_messages.len(),
                            None,
                            simple_intent_transitions(TerminalState::Completed, "greeting"),
                        );
                    }
                    Some(AssistantIntentKind::Help) => {
                        return graph_result(
                            memory,
                            TerminalState::Completed,
                            "help",
                            ResponseBuilder::help(),
                            context.recent_messages.len(),
                            None,
                            simple_intent_transitions(TerminalState::Completed, "help"),
                        );
                    }
                    Some(AssistantIntentKind::UnsafeRequest) => {
                        return graph_result(
                            memory,
                            TerminalState::BlockedByPolicy,
                            "unsafe_request",
                            ResponseBuilder::policy_blocked("This request is blocked by policy."),
                            context.recent_messages.len(),
                            None,
                            simple_intent_transitions(
                                TerminalState::BlockedByPolicy,
                                "unsafe_request",
                            ),
                        );
                    }
                    Some(AssistantIntentKind::OutOfDomain) => {
                        return graph_result(
                            memory,
                            TerminalState::OutOfDomain,
                            "out_of_domain",
                            ResponseBuilder::out_of_domain(),
                            context.recent_messages.len(),
                            None,
                            simple_intent_transitions(TerminalState::OutOfDomain, "out_of_domain"),
                        );
                    }
                    Some(AssistantIntentKind::UnsupportedInDomain) => {
                        return graph_result(
                            memory,
                            TerminalState::Unsupported,
                            "unsupported_in_domain",
                            ResponseBuilder::unsupported(),
                            context.recent_messages.len(),
                            None,
                            simple_intent_transitions(
                                TerminalState::Unsupported,
                                "unsupported_in_domain",
                            ),
                        );
                    }
                    _ => {}
                }
                tracing::info!(
                    target: "assistant::mapping",
                    query = %plan.query_text,
                    domain = ?plan.domain,
                    request_shape = ?plan.request_shape,
                    allow_all_capabilities = plan.allow_all_capabilities,
                    allowed_capabilities = ?plan.allowed_capabilities,
                    compatible_ids = ?catalog.map(|c| crate::assistant::retrieval::compatible_ids(&plan, c)),
                    "retrieval plan"
                );
                let evidence = RetrievalEngine::retrieve(&plan, llm, knowledge, catalog).await;
                let (evidence, warning) = match evidence {
                    Ok(evidence) => (evidence, None),
                    Err(error) => (Vec::new(), Some(error.to_string())),
                };
                tracing::info!(
                    target: "assistant::mapping",
                    evidence_count = evidence.len(),
                    evidence = ?evidence.iter().map(|e| (&e.capability_id, e.score)).collect::<Vec<_>>(),
                    warning = ?warning,
                    "retrieval evidence"
                );
                let decision = LlmReranker::new(llm)
                    .rerank(&plan.query_text, &evidence)
                    .await;
                tracing::info!(
                    target: "assistant::mapping",
                    decision = ?decision,
                    "reranker decision"
                );
                memory.retrieval_plan = json!(plan);
                memory.retrieval_evidence = json!(evidence);
                memory.evidence_decision = json!(decision);
                if let Some(message) = warning {
                    memory.warnings = json!([{ "message": message }]);
                }
                if let Some(routed_intent) = memory.intent.as_ref() {
                    retrieval_trace = Some(build_retrieval_trace(
                        routed_intent,
                        &plan,
                        &evidence,
                        &decision,
                    ));
                }
                match decision.decision {
                    RerankerVerdict::Select => {
                        // capability_id is required when Select; treat a missing/
                        // unknown id as ambiguity and Clarify with alternatives.
                        let capability_id = decision.capability_id.clone().and_then(|id| {
                            evidence.iter().any(|e| e.capability_id == id).then_some(id)
                        });
                        match capability_id {
                            Some(capability_id) => {
                                memory.selected_capability = Some(capability_id.clone());
                                let mut result = execute_selected_capability(
                                    memory,
                                    context.recent_messages.len(),
                                    capability_id,
                                    catalog,
                                    client,
                                    fineract_pool,
                                    canonical,
                                    None,
                                )
                                .await;
                                result.retrieval_trace = retrieval_trace.clone();
                                return result;
                            }
                            None => {
                                let payload = clarification_payload_for(
                                    &plan,
                                    &evidence,
                                    &decision.alternatives,
                                    memory
                                        .intent
                                        .as_ref()
                                        .map(|intent| source_intent_snapshot(intent, message)),
                                );
                                pending_clarification = Some(Some(payload.clone()));
                                (
                                    TerminalState::WaitingForUserInput,
                                    "weak_retrieval_evidence",
                                    ResponseBuilder::clarification(payload),
                                )
                            }
                        }
                    }
                    RerankerVerdict::Clarify => {
                        let payload = clarification_payload_for(
                            &plan,
                            &evidence,
                            &decision.alternatives,
                            memory
                                .intent
                                .as_ref()
                                .map(|intent| source_intent_snapshot(intent, message)),
                        );
                        pending_clarification = Some(Some(payload.clone()));
                        (
                            TerminalState::WaitingForUserInput,
                            "weak_retrieval_evidence",
                            ResponseBuilder::clarification(payload),
                        )
                    }
                    RerankerVerdict::Unsupported => (
                        TerminalState::Unsupported,
                        "unsupported_in_domain",
                        ResponseBuilder::unsupported(),
                    ),
                }
            }
            Err(error) => {
                memory.warnings = json!([{ "message": error.to_string() }]);
                (
                    TerminalState::FailedOperational,
                    "intent_route_failed",
                    ResponseBuilder::error(),
                )
            }
        };
        memory.graph_state = "complete_or_wait".into();
        memory.terminal_state = Some(terminal);
        memory.execution_summary = json!({
            "runtime": "semantic_assistant_graph",
            "recent_message_count": context.recent_messages.len(),
        });
        memory.structured_response = Some(response);
        let transitions = vec![
            GraphTransition {
                from: GraphState::ReceiveMessage,
                to: Some(GraphState::BuildContextWindow),
                terminal: None,
                reason: "message_received".into(),
            },
            GraphTransition {
                from: GraphState::BuildContextWindow,
                to: Some(GraphState::RouteIntent),
                terminal: None,
                reason: "context_built".into(),
            },
            GraphTransition {
                from: GraphState::RouteIntent,
                to: Some(GraphState::PlanRetrieval),
                terminal: None,
                reason: "intent_routed".into(),
            },
            GraphTransition {
                from: GraphState::PlanRetrieval,
                to: Some(GraphState::RetrieveKnowledge),
                terminal: None,
                reason: "retrieval_planned".into(),
            },
            GraphTransition {
                from: GraphState::RetrieveKnowledge,
                to: Some(GraphState::EvaluateEvidence),
                terminal: None,
                reason: "knowledge_retrieved".into(),
            },
            GraphTransition {
                from: GraphState::EvaluateEvidence,
                to: Some(GraphState::CompleteOrWait),
                terminal: None,
                reason: "evidence_evaluated".into(),
            },
            GraphTransition {
                from: GraphState::CompleteOrWait,
                to: None,
                terminal: Some(terminal),
                reason: reason.into(),
            },
        ];
        AssistantGraphTopology::new()
            .validate_sequence(&transitions)
            .expect("assistant runtime produced illegal graph transitions");
        GraphRuntimeResult {
            memory,
            transitions,
            pending_clarification,
            retrieval_trace,
        }
    }
}

fn pending_clarification_intent(context: &ContextWindow) -> AssistantIntent {
    let quantity = pending_clarification_quantity(context);
    AssistantIntent {
        intent: AssistantIntentKind::ClarificationReply,
        domain: match context.active_domain.as_deref() {
            Some("savings") => AssistantDomain::Savings,
            Some("client") => AssistantDomain::Client,
            Some("organization") => AssistantDomain::Organization,
            _ => AssistantDomain::Unknown,
        },
        request_shape: Default::default(),
        language: AssistantLanguage::En,
        entities: Vec::new(),
        constraints: crate::assistant::AssistantConstraints {
            quantity,
            ..Default::default()
        },
        context_reference: ContextReference::PendingClarification,
        source: None,
        confidence: 1.0,
        reason: "exact pending clarification option".into(),
    }
}

fn pending_clarification_quantity(context: &ContextWindow) -> Option<Quantity> {
    context
        .recent_messages
        .iter()
        .rev()
        .filter(|message| message.role == "user")
        .find_map(|message| first_standalone_limit(&message.content))
        .map(|value| Quantity::TopN { value })
}

fn first_standalone_limit(content: &str) -> Option<i64> {
    content
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .find_map(|part| {
            part.parse::<i64>()
                .ok()
                .filter(|value| (1..=100).contains(value))
        })
}

fn is_parameter_reply(message: &str) -> bool {
    message.split_whitespace().count() <= 6
}

#[cfg(test)]
fn merge_deterministic_extraction(
    memory: &mut JobMemory,
    intent: &mut AssistantIntent,
    message: &str,
) {
    let extraction = extract_message_facts(message);
    let conflicts = extraction.conflicts_with(intent);
    if !conflicts.is_empty() {
        memory.current_user_message_metadata["deterministic_extraction_conflicts"] =
            serde_json::to_value(conflicts).unwrap_or_else(|_| json!([]));
    }
    extraction.merge_into(intent);
    record_extraction_metadata(memory, &extraction);
}

fn merge_deterministic_extraction_at(
    memory: &mut JobMemory,
    intent: &mut AssistantIntent,
    message: &str,
    canonical: Option<&CanonicalRuntimeContext>,
) {
    let extraction = extract_for_context(message, canonical);
    let conflicts = extraction.conflicts_with(intent);
    if !conflicts.is_empty() {
        memory.current_user_message_metadata["deterministic_extraction_conflicts"] =
            serde_json::to_value(conflicts).unwrap_or_else(|_| json!([]));
    }
    extraction.merge_into(intent);
    record_extraction_metadata(memory, &extraction);
}

/// Records deterministic extraction metadata for a clarification-reply turn.
///
/// Bug 08-B: previously this only re-extracted facts from the *original*
/// Turn-1 prompt (`payload.source_intent.prompt`), which clobbers whatever
/// the user actually said in their Turn-2 reply. `verify_capability_metric`
/// (tool.rs) then gates the newly *selected* capability against a metric
/// extracted from Turn-1's wording — even though the user just explicitly
/// picked a different capability in Turn 2. Refresh from the current turn's
/// message and let it take priority; fall back to the Turn-1 extraction only
/// for fields the current turn's message didn't mention (e.g. a bare "3"
/// limit stated only in Turn 1).
fn record_source_extraction_metadata(
    memory: &mut JobMemory,
    payload: &ClarificationPayload,
    canonical: Option<&CanonicalRuntimeContext>,
    current_message: &str,
) {
    let source_extraction = payload
        .source_intent
        .as_ref()
        .map(|source| extract_for_context(&source.prompt, canonical))
        .unwrap_or_default();
    let current_extraction = extract_for_context(current_message, canonical);
    let refreshed = prefer_current_turn_extraction(source_extraction, current_extraction);
    record_extraction_metadata(memory, &refreshed);
}

/// Merges two extraction passes, letting the current turn's signals win over
/// the original (Turn-1) source prompt's — see `record_source_extraction_metadata`.
fn prefer_current_turn_extraction(
    source: DeterministicExtraction,
    current: DeterministicExtraction,
) -> DeterministicExtraction {
    DeterministicExtraction {
        constraints: crate::assistant::AssistantConstraints {
            quantity: current.constraints.quantity.or(source.constraints.quantity),
            from_date: current
                .constraints
                .from_date
                .or(source.constraints.from_date),
            to_date: current.constraints.to_date.or(source.constraints.to_date),
            currency_code: current
                .constraints
                .currency_code
                .or(source.constraints.currency_code),
            product_ids: current
                .constraints
                .product_ids
                .or(source.constraints.product_ids),
            office_ids: current
                .constraints
                .office_ids
                .or(source.constraints.office_ids),
            metric: current.constraints.metric.or(source.constraints.metric),
        },
        domain: current.domain.or(source.domain),
        entities: if current.entities.is_empty() {
            source.entities
        } else {
            current.entities
        },
        candidates: if current.candidates.is_empty() {
            source.candidates
        } else {
            current.candidates
        },
        temporal_provenance: current.temporal_provenance.or(source.temporal_provenance),
        temporal_error: current.temporal_error.or(source.temporal_error),
    }
}

fn extract_for_context(
    message: &str,
    canonical: Option<&CanonicalRuntimeContext>,
) -> DeterministicExtraction {
    canonical
        .map(|context| extract_message_facts_at(message, context.reference_instant, 366))
        .unwrap_or_else(|| extract_message_facts(message))
}

fn record_extraction_metadata(
    memory: &mut JobMemory,
    extraction: &crate::assistant::DeterministicExtraction,
) {
    if !extraction.is_empty() {
        memory.current_user_message_metadata["deterministic_extraction"] =
            serde_json::to_value(extraction).unwrap_or_else(|_| json!({}));
    }
}

async fn execute_selected_capability(
    mut memory: JobMemory,
    recent_message_count: usize,
    capability_id: String,
    catalog: Option<&Arc<KnowledgeCatalog>>,
    client: Option<&PrincipalContext>,
    fineract_pool: Option<&PgPool>,
    canonical: Option<&CanonicalRuntimeContext>,
    pending_clarification: Option<Option<ClarificationPayload>>,
) -> GraphRuntimeResult {
    let (Some(catalog), Some(client)) = (catalog, client) else {
        return graph_result(
            memory,
            TerminalState::Completed,
            "capability_selected",
            ResponseBuilder::selected(capability_id),
            recent_message_count,
            pending_clarification.clone(),
            execution_transitions(TerminalState::Completed, "capability_selected"),
        );
    };
    let intent = memory.intent.clone();
    if intent.is_none()
        && canonical.is_none_or(|context| context.mode != CanonicalGatewayMode::Authoritative)
    {
        return graph_result(
            memory,
            TerminalState::WaitingForUserInput,
            "missing_intent",
            ResponseBuilder::missing_parameter("Please include the client name to search for."),
            recent_message_count,
            pending_clarification.clone(),
            execution_transitions(TerminalState::WaitingForUserInput, "missing_intent"),
        );
    }
    if let Some(error) = memory
        .current_user_message_metadata
        .get("deterministic_extraction")
        .cloned()
        .and_then(|value| serde_json::from_value::<DeterministicExtraction>(value).ok())
        .and_then(|extraction| extraction.temporal_error)
    {
        tracing::warn!(
            target: "assistant::execute_selected_capability",
            capability_id = %capability_id,
            error_code = %error.code,
            error_message = %error.message,
            "clarification-reply execution blocked: invalid temporal input"
        );
        let payload = ClarificationPayload {
            question: error.message.clone(),
            options: vec![ClarificationOption {
                id: capability_id.clone(),
                label: capability_id.clone(),
                description: Some("Retry this report after providing a valid date range.".into()),
            }],
            attempt: 1,
            source_intent: intent
                .as_ref()
                .map(|intent| source_intent_snapshot(intent, &intent.reason)),
            allow_free_text: true,
            is_missing_execution_parameters: true,
        };
        return graph_result(
            memory,
            TerminalState::WaitingForUserInput,
            &error.code,
            ResponseBuilder::clarification(payload.clone()),
            recent_message_count,
            Some(Some(payload)),
            execution_transitions(TerminalState::WaitingForUserInput, "invalid_temporal_input"),
        );
    }
    let authoritative =
        canonical.filter(|context| context.mode == CanonicalGatewayMode::Authoritative);
    let authoritative_plan = match authoritative {
        Some(context) => {
            authoritative_plan(context, &mut memory, catalog, client, &capability_id).await
        }
        None => Ok(None),
    };
    let (plan, execution_client) = match authoritative_plan {
        Ok(Some((plan, principal))) => (plan, principal),
        Ok(None) => {
            let intent = intent.as_ref().expect("legacy path checked intent");
            let deterministic_extraction = memory
                .current_user_message_metadata
                .get("deterministic_extraction")
                .cloned()
                .and_then(|value| serde_json::from_value::<DeterministicExtraction>(value).ok());
            match crate::assistant::plan_selected_capability_verified(
                catalog,
                &capability_id,
                intent,
                deterministic_extraction.as_ref(),
            ) {
                Ok(plan) => (plan, client.clone()),
                Err(error) => {
                    tracing::warn!(
                        target: "assistant::execute_selected_capability",
                        capability_id = %capability_id,
                        error = %error,
                        "clarification-reply plan_selected_capability_verified failed; \
                         re-clarifying instead of executing"
                    );
                    let payload = ClarificationPayload {
                        question: error.to_string(),
                        options: vec![
                            ClarificationOption {
                                id: capability_id.clone(),
                                label: capability_id.clone(),
                                description: None,
                            },
                            ClarificationOption {
                                id: OTHER_CLARIFICATION_OPTION_ID.into(),
                                label: "Others".into(),
                                description: Some(
                                    "Let me describe what I need in my own words.".into(),
                                ),
                            },
                        ],
                        attempt: 1,
                        source_intent: Some(source_intent_snapshot(intent, &intent.reason)),
                        allow_free_text: true,
                        is_missing_execution_parameters: true,
                    };
                    return graph_result(
                        memory,
                        TerminalState::WaitingForUserInput,
                        "missing_execution_parameters",
                        ResponseBuilder::clarification(payload.clone()),
                        recent_message_count,
                        Some(Some(payload)),
                        execution_transitions(
                            TerminalState::WaitingForUserInput,
                            "missing_execution_parameters",
                        ),
                    );
                }
            }
        }
        Err(error) => {
            tracing::warn!(
                target: "assistant::execute_selected_capability",
                capability_id = %capability_id,
                error = %error,
                "clarification-reply authoritative_plan failed; returning routing error"
            );
            memory.warnings = json!([{ "message": error.to_string() }]);
            return graph_result(
                memory,
                TerminalState::FailedOperational,
                "canonical_snapshot_invalid",
                ResponseBuilder::error(),
                recent_message_count,
                pending_clarification,
                execution_transitions(
                    TerminalState::FailedOperational,
                    "canonical_snapshot_invalid",
                ),
            );
        }
    };
    let evidence_refs = evidence_refs(&memory.retrieval_evidence);
    let tool_request = crate::assistant::tool_request_from_plan(&plan, evidence_refs);
    memory.selected_tool = Some(tool_request.tool_name.clone());
    memory.tool_params = json!(tool_request);
    let policy = crate::assistant::guard_selected_capability(&execution_client, catalog, &plan);
    memory.policy_decision = json!(policy);
    if policy.status != PolicyDecisionStatus::Allowed {
        return graph_result(
            memory,
            TerminalState::BlockedByPolicy,
            "blocked_by_policy",
            ResponseBuilder::policy_blocked(policy.reason.as_deref().unwrap_or("policy blocked")),
            recent_message_count,
            pending_clarification.clone(),
            execution_transitions(TerminalState::BlockedByPolicy, "blocked_by_policy"),
        );
    }
    let Some(pool) = fineract_pool else {
        return graph_result(
            memory,
            TerminalState::Completed,
            "execution_not_configured",
            ResponseBuilder::selected(capability_id),
            recent_message_count,
            pending_clarification.clone(),
            execution_transitions(TerminalState::Completed, "execution_not_configured"),
        );
    };
    match execute_plan(pool, catalog, &plan, &policy).await {
        Ok(result) => {
            let tool_result =
                crate::assistant::tool_result_from_execution(&tool_request, result.clone());
            let response = ResponseBuilder::from_tool_result(
                intent.as_ref().expect("successful execution has intent"),
                &plan,
                &policy,
                &tool_result,
                catalog,
            );
            let mut result_state = graph_result(
                memory,
                TerminalState::Completed,
                "execution_completed",
                response,
                recent_message_count,
                pending_clarification.clone(),
                execution_transitions(TerminalState::Completed, "execution_completed"),
            );
            result_state.memory.execution_summary = json!({ "plan": plan, "policy": policy, "tool_request": tool_request, "tool_result": tool_result, "result": result });
            result_state
        }
        Err(error) => {
            tracing::warn!(
                target: "assistant::execute_selected_capability",
                capability_id = %capability_id,
                query_id = %plan.query_id,
                error = %error,
                "clarification-reply execute_plan failed; returning routing error"
            );
            memory.warnings = json!([{ "message": error.to_string() }]);
            graph_result(
                memory,
                TerminalState::FailedOperational,
                "execution_failed",
                ResponseBuilder::error(),
                recent_message_count,
                pending_clarification,
                execution_transitions(TerminalState::FailedOperational, "execution_failed"),
            )
        }
    }
}

async fn authoritative_plan(
    context: &CanonicalRuntimeContext,
    memory: &mut JobMemory,
    catalog: &KnowledgeCatalog,
    current_client: &PrincipalContext,
    capability_id: &str,
) -> anyhow::Result<Option<(crate::chat::planner::ExecutionPlan, PrincipalContext)>> {
    let catalog_version = context
        .catalog_version
        .ok_or_else(|| anyhow::anyhow!("missing canonical catalog version"))?;
    if let Some(snapshot_id) = memory.planner_snapshot_id {
        let loaded = context
            .repository
            .get_planner_snapshot(snapshot_id, memory.job_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("missing planner snapshot"))?;
        anyhow::ensure!(
            loaded.capability_catalog_version == catalog_version,
            "mismatched planner snapshot catalog"
        );
        if loaded.revision == context.revision {
            let plan = plan_from_snapshot(catalog, &loaded)?;
            let principal = principal_from_snapshot(loaded.principal_projection);
            return Ok(Some((plan, principal)));
        }
        anyhow::ensure!(
            loaded.revision < context.revision,
            "mismatched planner snapshot revision"
        );
    }
    let extraction = memory
        .current_user_message_metadata
        .get("deterministic_extraction")
        .cloned()
        .and_then(|value| serde_json::from_value::<DeterministicExtraction>(value).ok())
        .unwrap_or_default();
    let source_id = context.message_id.to_string();
    let effective = if context.initial {
        let intent = memory
            .intent
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing accepted initial parse"))?;
        let mut original = OriginalIntent {
            id: stable_uuid(memory.job_id, 1),
            job_id: memory.job_id,
            schema_version: 1,
            raw_message_id: context.message_id,
            locale: intent.language.clone(),
            action: intent.intent.clone(),
            entities: intent.entities.clone(),
            metrics: intent.constraints.metric.clone().into_iter().collect(),
            groupings: vec![format!("{:?}", intent.request_shape.grouping).to_lowercase()],
            output: Some(format!("{:?}", intent.request_shape.output).to_lowercase()),
            parameters: Default::default(),
            pii_request: false,
            extraction_provenance: vec![ExtractionProvenance {
                extractor: "semantic_router".into(),
                version: "canonical_v1".into(),
                source_identifiers: vec![source_id.clone()],
                source_spans: Vec::new(),
                rule: None,
                reference_instant: None,
                timezone: None,
            }],
            created_at: context.reference_instant,
        };
        if let Some(provenance) = &extraction.temporal_provenance {
            original.extraction_provenance.push(ExtractionProvenance {
                extractor: "deterministic_temporal_resolver".into(),
                version: "v1".into(),
                source_identifiers: vec![source_id.clone()],
                source_spans: vec![provenance.phrase_span],
                rule: Some(provenance.rule.clone()),
                reference_instant: Some(provenance.reference_instant),
                timezone: Some(provenance.timezone.clone()),
            });
        }
        let observations = original_request_observations(
            memory.job_id,
            &source_id,
            intent,
            &extraction,
            context.observed_at,
        );
        let mut effective = merge_observations(
            memory.job_id,
            context.revision,
            &observations,
            &executable_constraint_contracts(),
        )?;
        effective.id = stable_uuid(memory.job_id, context.revision as u128 + 2);
        effective.created_at = context.observed_at;
        context
            .repository
            .insert_initial_state(&original, &observations, &effective)
            .await?
            .2
    } else {
        let first_sequence = context
            .repository
            .list_observations(memory.job_id)
            .await?
            .len() as i64
            + 1;
        let observations = deterministic_observations(
            memory.job_id,
            &source_id,
            first_sequence,
            FactSourceKind::Clarification,
            &extraction,
            context.observed_at,
        );
        context
            .repository
            .append_observations(memory.job_id, &observations)
            .await?;
        context
            .repository
            .derive_and_insert_effective(
                memory.job_id,
                context.revision,
                &executable_constraint_contracts(),
            )
            .await?
    };
    let original = context
        .repository
        .get_original_intent(memory.job_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing canonical original intent"))?;
    let snapshot = PlannerInputSnapshot {
        id: stable_uuid(memory.job_id, context.revision as u128 + 100),
        job_id: memory.job_id,
        revision: context.revision,
        original_intent_id: original.id,
        effective_constraints_id: effective.id,
        capability_catalog_version: catalog_version,
        principal_projection: PrincipalProjection {
            user_id: current_client.user_id,
            role: current_client.role.clone(),
            capability_ids: current_client.capability_ids.clone(),
            office_ids: current_client.office_ids.clone(),
            can_view_pii: current_client.can_view_pii,
            legacy_api_key_id: current_client.legacy_api_key_id,
        },
        reference_instant: context.reference_instant,
        timezone: context.timezone.clone(),
        selected_capability_id: capability_id.to_owned(),
        normalized_parameters: normalize_effective_parameters(catalog, capability_id, &effective)?,
        created_at: context.observed_at,
    };
    let snapshot_id = context
        .repository
        .insert_planner_snapshot(&snapshot)
        .await?
        .id;
    memory.planner_snapshot_id = Some(snapshot_id);
    let loaded = context
        .repository
        .get_planner_snapshot(snapshot_id, memory.job_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing planner snapshot"))?;
    anyhow::ensure!(
        loaded.job_id == memory.job_id
            && loaded.original_intent_id == original.id
            && loaded.effective_constraints_id == effective.id
            && loaded.capability_catalog_version == catalog_version,
        "mismatched planner snapshot"
    );
    let plan = plan_from_snapshot(catalog, &loaded)?;
    let principal = principal_from_snapshot(loaded.principal_projection);
    Ok(Some((plan, principal)))
}

fn principal_from_snapshot(projection: PrincipalProjection) -> PrincipalContext {
    PrincipalContext {
        user_id: projection.user_id,
        role: projection.role,
        capability_ids: projection.capability_ids,
        office_ids: projection.office_ids,
        can_view_pii: projection.can_view_pii,
        legacy_api_key_id: projection.legacy_api_key_id,
    }
}

fn graph_result(
    mut memory: JobMemory,
    terminal: TerminalState,
    reason: &str,
    response: AssistantResponse,
    recent_message_count: usize,
    pending_clarification: Option<Option<ClarificationPayload>>,
    transitions: Vec<GraphTransition>,
) -> GraphRuntimeResult {
    memory.graph_state = "complete_or_wait".into();
    memory.terminal_state = Some(terminal);
    memory.execution_summary = json!({
        "runtime": "semantic_assistant_graph",
        "recent_message_count": recent_message_count,
        "reason": reason,
    });
    memory.structured_response = Some(finish(response));
    AssistantGraphTopology::new()
        .validate_sequence(&transitions)
        .expect("assistant runtime produced illegal graph transitions");
    GraphRuntimeResult {
        memory,
        transitions,
        pending_clarification,
        retrieval_trace: None,
    }
}

fn deterministic_simple_response(
    message: &str,
) -> Option<(AssistantIntentKind, AssistantResponse)> {
    let normalized = message
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation())
        .to_ascii_lowercase();
    match normalized.as_str() {
        "hi" | "hello" | "hey" => {
            Some((AssistantIntentKind::Greeting, ResponseBuilder::greeting()))
        }
        "help" | "bisa apa" => Some((AssistantIntentKind::Help, ResponseBuilder::help())),
        _ => None,
    }
}

fn deterministic_intent(intent: AssistantIntentKind, message: &str) -> AssistantIntent {
    AssistantIntent {
        intent,
        domain: AssistantDomain::Unknown,
        request_shape: Default::default(),
        language: AssistantLanguage::En,
        entities: Vec::new(),
        constraints: AssistantConstraints::default(),
        context_reference: ContextReference::None,
        source: None,
        confidence: 1.0,
        reason: format!("deterministic simple intent: {message}"),
    }
}

fn evidence_refs(evidence: &serde_json::Value) -> Vec<String> {
    evidence
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.get("capability_id")
                .or_else(|| item.get("id"))
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect()
}

fn resolve_pending_clarification(
    input: &RuntimeUserInput,
    payload: &ClarificationPayload,
    memory: &JobMemory,
    context: &ContextWindow,
) -> Option<ClarificationOutcome> {
    input
        .selected_option_id
        .as_deref()
        .map(|id| {
            if id.eq_ignore_ascii_case(OTHER_CLARIFICATION_OPTION_ID) {
                ClarificationOutcome::FreeFormOther {
                    text: String::new(),
                    confidence: 1.0,
                }
            } else if clarification_candidate_allowed(id, payload, memory, context) {
                ClarificationOutcome::SelectedOption {
                    option_id: id.to_string(),
                    confidence: 1.0,
                }
            } else {
                ClarificationOutcome::Unresolved {
                    reason: "selected option is not available".into(),
                }
            }
        })
        .or_else(|| ClarificationResolver::resolve_exact(&input.source_message, payload))
}

fn clarification_candidate_allowed(
    id: &str,
    payload: &ClarificationPayload,
    _memory: &JobMemory,
    context: &ContextWindow,
) -> bool {
    let is_candidate = payload.options.iter().any(|option| option.id == id);
    if !is_candidate {
        return false;
    }
    let has_scope = context.client_scope.get("allow_all_capabilities").is_some()
        || context.client_scope.get("capabilities").is_some();
    if !has_scope {
        return true;
    }
    allow_all_capabilities(context)
        || allowed_capabilities(context)
            .iter()
            .any(|capability| capability == id)
}

fn continuation_capability(payload: &ClarificationPayload) -> Option<String> {
    if !payload.is_missing_execution_parameters {
        return None;
    }
    let mut options = payload
        .options
        .iter()
        .filter(|option| option.id != OTHER_CLARIFICATION_OPTION_ID);
    let option = options.next()?;
    options.next().is_none().then(|| option.id.clone())
}

fn source_intent_snapshot(intent: &AssistantIntent, prompt: &str) -> SourceIntentSnapshot {
    SourceIntentSnapshot {
        prompt: prompt.into(),
        normalized_prompt: Some(prompt.trim().to_lowercase()),
        intent: intent.intent.clone(),
        domain: intent.domain.clone(),
        request_shape: intent.request_shape.clone(),
        entities: intent.entities.clone(),
        constraints: intent.constraints.clone(),
        context_reference: intent.context_reference.clone(),
        confidence: intent.confidence,
        reason: intent.reason.clone(),
    }
}

fn intent_from_source(
    payload: &ClarificationPayload,
    context: &ContextWindow,
    canonical: Option<&CanonicalRuntimeContext>,
) -> AssistantIntent {
    if let Some(source) = &payload.source_intent {
        let mut intent = AssistantIntent {
            intent: source.intent.clone(),
            domain: source.domain.clone(),
            request_shape: source.request_shape.clone(),
            language: AssistantLanguage::En,
            entities: source.entities.clone(),
            constraints: source.constraints.clone(),
            context_reference: ContextReference::PendingClarification,
            source: Some(source.clone()),
            confidence: source.confidence,
            reason: format!(
                "clarification resolved from source intent: {}",
                source.reason
            ),
        };
        if matches!(intent.constraints.quantity, None | Some(Quantity::Default)) {
            intent.constraints.quantity = pending_clarification_quantity(context);
        }
        let extraction = extract_for_context(&source.prompt, canonical);
        extraction.merge_into(&mut intent);
        return intent;
    }
    pending_clarification_intent(context)
}

fn clarification_audit(
    source: &str,
    option_id: &str,
    input: &RuntimeUserInput,
    payload: &ClarificationPayload,
) -> serde_json::Value {
    json!({
        "clarification_outcome": "selected_option",
        "option_id": option_id,
        "source_message": input.source_message,
        "source": source,
        "source_intent": payload.source_intent,
    })
}

fn allowed_capabilities(context: &ContextWindow) -> Vec<String> {
    context
        .client_scope
        .get("capabilities")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect()
}

fn allow_all_capabilities(context: &ContextWindow) -> bool {
    context
        .client_scope
        .get("allow_all_capabilities")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn simple_intent_transitions(terminal: TerminalState, reason: &str) -> Vec<GraphTransition> {
    vec![
        GraphTransition {
            from: GraphState::ReceiveMessage,
            to: Some(GraphState::BuildContextWindow),
            terminal: None,
            reason: "message_received".into(),
        },
        GraphTransition {
            from: GraphState::BuildContextWindow,
            to: Some(GraphState::RouteIntent),
            terminal: None,
            reason: "context_built".into(),
        },
        GraphTransition {
            from: GraphState::RouteIntent,
            to: Some(GraphState::CompleteOrWait),
            terminal: None,
            reason: "intent_routed".into(),
        },
        GraphTransition {
            from: GraphState::CompleteOrWait,
            to: None,
            terminal: Some(terminal),
            reason: reason.into(),
        },
    ]
}

fn execution_transitions(terminal: TerminalState, reason: &str) -> Vec<GraphTransition> {
    vec![
        GraphTransition {
            from: GraphState::ReceiveMessage,
            to: Some(GraphState::BuildContextWindow),
            terminal: None,
            reason: "message_received".into(),
        },
        GraphTransition {
            from: GraphState::BuildContextWindow,
            to: Some(GraphState::RouteIntent),
            terminal: None,
            reason: "context_built".into(),
        },
        GraphTransition {
            from: GraphState::RouteIntent,
            to: Some(GraphState::PlanRetrieval),
            terminal: None,
            reason: "intent_routed".into(),
        },
        GraphTransition {
            from: GraphState::PlanRetrieval,
            to: Some(GraphState::RetrieveKnowledge),
            terminal: None,
            reason: "retrieval_planned".into(),
        },
        GraphTransition {
            from: GraphState::RetrieveKnowledge,
            to: Some(GraphState::EvaluateEvidence),
            terminal: None,
            reason: "knowledge_retrieved".into(),
        },
        GraphTransition {
            from: GraphState::EvaluateEvidence,
            to: Some(GraphState::PlanToolOrCapability),
            terminal: None,
            reason: "evidence_evaluated".into(),
        },
        GraphTransition {
            from: GraphState::PlanToolOrCapability,
            to: Some(GraphState::GuardExecution),
            terminal: None,
            reason: "tool_planned".into(),
        },
        GraphTransition {
            from: GraphState::GuardExecution,
            to: Some(GraphState::ExecuteToolOrSql),
            terminal: None,
            reason: "policy_checked".into(),
        },
        GraphTransition {
            from: GraphState::ExecuteToolOrSql,
            to: Some(GraphState::BuildStructuredResponse),
            terminal: None,
            reason: "execution_finished".into(),
        },
        GraphTransition {
            from: GraphState::BuildStructuredResponse,
            to: Some(GraphState::CompleteOrWait),
            terminal: None,
            reason: "response_built".into(),
        },
        GraphTransition {
            from: GraphState::CompleteOrWait,
            to: None,
            terminal: Some(terminal),
            reason: reason.into(),
        },
    ]
}

/// Variant of `clarification_payload` that prefers the reranker's `alternatives`
/// (capability ids) as the option pool, filtering evidence to those ids. Falls
/// back to the top-3 evidence when `alternatives` is empty (parity with the
/// pre-reranker payload builder).
fn clarification_payload_for(
    plan: &RetrievalPlan,
    evidence: &[Evidence],
    alternatives: &[String],
    source_intent: Option<SourceIntentSnapshot>,
) -> ClarificationPayload {
    if alternatives.is_empty() {
        return clarification_payload(plan, evidence, source_intent);
    }
    let by_id: std::collections::HashMap<&str, &Evidence> = evidence
        .iter()
        .map(|e| (e.capability_id.as_str(), e))
        .collect();
    let mut options: Vec<ClarificationOption> = alternatives
        .iter()
        .filter_map(|id| {
            by_id.get(id.as_str()).map(|e| ClarificationOption {
                id: e.capability_id.clone(),
                label: e.title.clone(),
                description: e
                    .metadata
                    .get("description")
                    .or_else(|| e.metadata.get("summary"))
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned),
            })
        })
        .collect();
    if options.is_empty() {
        options = fallback_clarification_options(&plan.domain);
    }
    options.push(ClarificationOption {
        id: OTHER_CLARIFICATION_OPTION_ID.into(),
        label: "Others".into(),
        description: Some("Let me describe what I need in my own words.".into()),
    });
    ClarificationPayload {
        question: "Which report should I use?".into(),
        options,
        attempt: 1,
        source_intent,
        allow_free_text: true,
        is_missing_execution_parameters: false,
    }
}

fn clarification_payload(
    plan: &RetrievalPlan,
    evidence: &[Evidence],
    source_intent: Option<SourceIntentSnapshot>,
) -> ClarificationPayload {
    let mut options: Vec<ClarificationOption> = evidence
        .iter()
        .take(3)
        .map(|item| ClarificationOption {
            id: item.capability_id.clone(),
            label: item.title.clone(),
            description: item
                .metadata
                .get("description")
                .or_else(|| item.metadata.get("summary"))
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
        })
        .collect();
    if options.is_empty() {
        options = fallback_clarification_options(&plan.domain);
    }
    options.push(ClarificationOption {
        id: OTHER_CLARIFICATION_OPTION_ID.into(),
        label: "Others".into(),
        description: Some("Let me describe what I need in my own words.".into()),
    });
    ClarificationPayload {
        question: "Which report should I use?".into(),
        options,
        attempt: 1,
        source_intent,
        allow_free_text: true,
        is_missing_execution_parameters: false,
    }
}

fn fallback_clarification_options(domain: &AssistantDomain) -> Vec<ClarificationOption> {
    let choices = match domain {
        AssistantDomain::Savings | AssistantDomain::Client => vec![
            (
                "client_top_n_by_savings_account_count",
                "Top clients by number of savings accounts",
                "Rank clients by savings account count.",
            ),
            (
                "client_top_n_by_savings_balance",
                "Top clients by savings balance",
                "Rank clients by total savings balance.",
            ),
            (
                "client_top_n_by_deposit_volume",
                "Top clients by deposit volume",
                "Rank clients by deposit transaction volume.",
            ),
        ],
        AssistantDomain::Organization => vec![
            (
                "organization_office_summary",
                "Office summary",
                "Summarize offices in the organization.",
            ),
            (
                "organization_office_savings_summary",
                "Office savings summary",
                "Summarize savings by office.",
            ),
            (
                "organization_office_activity_ranking",
                "Office activity ranking",
                "Rank offices by activity.",
            ),
        ],
        _ => vec![
            (
                "savings_deposit_top_n",
                "Top savings deposits",
                "Rank savings accounts by deposits.",
            ),
            (
                "savings_balance_summary",
                "Savings balance summary",
                "Summarize savings balances.",
            ),
            (
                "organization_office_summary",
                "Office summary",
                "Summarize offices in the organization.",
            ),
        ],
    };

    choices
        .into_iter()
        .map(|(id, label, description)| ClarificationOption {
            id: id.into(),
            label: label.into(),
            description: Some(description.into()),
        })
        .collect()
}

fn clarification_transitions(terminal: TerminalState, reason: &str) -> Vec<GraphTransition> {
    vec![
        GraphTransition {
            from: GraphState::ReceiveMessage,
            to: Some(GraphState::BuildContextWindow),
            terminal: None,
            reason: "message_received".into(),
        },
        GraphTransition {
            from: GraphState::BuildContextWindow,
            to: Some(GraphState::RouteIntent),
            terminal: None,
            reason: "context_built".into(),
        },
        GraphTransition {
            from: GraphState::RouteIntent,
            to: Some(GraphState::ResolveClarification),
            terminal: None,
            reason: "clarification_reply".into(),
        },
        GraphTransition {
            from: GraphState::ResolveClarification,
            to: Some(GraphState::CompleteOrWait),
            terminal: None,
            reason: "clarification_resolved".into(),
        },
        GraphTransition {
            from: GraphState::CompleteOrWait,
            to: None,
            terminal: Some(terminal),
            reason: reason.into(),
        },
    ]
}

/// Builds a JSON audit trace of one retrieval pass for `state_json.retrieval_trace`.
/// Best-effort/debug-only shape — not part of the graph contract, so it is built
/// inline at the call site rather than added as a `JobMemory` field.
pub fn build_retrieval_trace(
    intent: &AssistantIntent,
    plan: &crate::assistant::evidence::RetrievalPlan,
    evidence: &[crate::assistant::evidence::Evidence],
    decision: &RerankerDecision,
) -> serde_json::Value {
    let candidates: Vec<_> = evidence
        .iter()
        .take(10)
        .map(|e| {
            json!({
                "capability_id": e.capability_id,
                "title": e.title,
                "score": e.score,
                "source_type": e.source_type,
            })
        })
        .collect();

    let kind = match decision.decision {
        RerankerVerdict::Select => "select",
        RerankerVerdict::Clarify => "clarify",
        RerankerVerdict::Unsupported => "unsupported",
    };
    let decision_json = json!({
        "kind": kind,
        "capability_id": decision.capability_id,
        "confidence": decision.confidence,
        "alternatives": decision.alternatives,
        "reason": decision.reason,
    });

    json!({
        "router_intent": {
            "intent": intent.intent,
            "domain": intent.domain,
            "request_shape": intent.request_shape,
            "confidence": intent.confidence,
        },
        "plan": {
            "query_text": plan.query_text,
            "allowed_capability_count": plan.allowed_capabilities.len(),
            "allow_all_capabilities": plan.allow_all_capabilities,
        },
        "candidates": candidates,
        "decision": decision_json,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;
    use async_trait::async_trait;
    use uuid::Uuid;

    use super::*;
    use crate::{
        assistant::{
            AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage,
            ContextReference,
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
        assert!(payload.options.iter().any(|option| {
            option.id == OTHER_CLARIFICATION_OPTION_ID && option.label == "Others"
        }));
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
        field: crate::assistant::extraction::PayloadField,
    ) -> crate::assistant::extraction::PayloadCandidate {
        crate::assistant::extraction::PayloadCandidate {
            field,
            value: json!("sample"),
            source: crate::assistant::extraction::PayloadSource::UserText,
            trust: crate::assistant::extraction::PayloadTrust::Trusted,
        }
    }

    #[test]
    fn prefer_current_turn_extraction_falls_back_to_source_candidates_when_current_empty() {
        let source = DeterministicExtraction {
            candidates: vec![sample_candidate(
                crate::assistant::extraction::PayloadField::Metric,
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
                crate::assistant::extraction::PayloadField::Metric,
            )],
            ..Default::default()
        };
        let current = DeterministicExtraction {
            candidates: vec![sample_candidate(
                crate::assistant::extraction::PayloadField::Limit,
            )],
            ..Default::default()
        };

        let merged = prefer_current_turn_extraction(source, current.clone());

        assert_eq!(merged.candidates, current.candidates);
    }
}
