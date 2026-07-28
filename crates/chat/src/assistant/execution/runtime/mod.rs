mod clarification;
mod execution;
mod extraction;
mod planning;
mod semantic;
#[cfg(test)]
mod tests;
mod transition;

use clarification::*;
use execution::*;
use extraction::*;
use planning::authoritative_plan;
pub use planning::build_retrieval_trace;
use semantic::complete_semantic_route;
use transition::*;

use serde_json::json;
use std::sync::Arc;

use app_core::auth::model::PrincipalContext;
use app_core::config::CanonicalGatewayMode;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::assistant::temporal::BusinessDateSource;

use super::tool::{normalize_effective_parameters, plan_from_snapshot};
use crate::assistant::execution::plan::PolicyDecisionStatus;
use crate::assistant::{
    AssistantConstraints, AssistantDomain, AssistantGraphTopology, AssistantIntent,
    AssistantIntentKind, AssistantLanguage, AssistantResponse, CanonicalStateRepository,
    ClarificationFacts, ClarificationOption, ClarificationOutcome, ClarificationPayload,
    ClarificationPlanResult, ClarificationPlanner, ClarificationResolver, ConstraintField,
    ContextReference, ContextWarningCode, ContextWindow, DeterministicExtraction,
    ExtractionProvenance, FactSourceKind, GraphState, GraphTransition, JobMemory, LimitMode,
    OTHER_CLARIFICATION_OPTION_ID, OriginalIntent, PlannerInputSnapshot, PrincipalProjection,
    Quantity, ResponseBuilder, SemanticRouter, SourceIntentSnapshot, TerminalState, TypedFactValue,
    evidence::{Evidence, RetrievalPlan},
    executable_constraint_contracts, extract_message_facts, extract_message_facts_at,
    llm::SharedLlmClient,
    merge_observations, original_request_observations,
    presentation::builder::finish,
    reranker::{LlmReranker, RerankerDecision, RerankerVerdict},
    retrieval::RetrievalEngine,
    stable_uuid,
};
use crate::execution::repository::execute_plan;
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
    pub clarification_id: Option<Uuid>,
    pub clarification_revision: Option<u32>,
    pub constraint_patch: crate::assistant::ConstraintPatch,
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
    pub business_today: NaiveDate,
    pub business_date_source: BusinessDateSource,
    pub execution_limits: crate::execution::repository::ExecutionLimits,
}

impl From<&str> for RuntimeUserInput {
    fn from(message: &str) -> Self {
        Self {
            message: message.into(),
            source_message: message.into(),
            selected_option_id: None,
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
        }
    }
}

impl From<String> for RuntimeUserInput {
    fn from(message: String) -> Self {
        Self {
            source_message: message.clone(),
            message,
            selected_option_id: None,
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
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
        memory.structured_response = Some(ResponseBuilder::error());
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
        // This input is constructed from the validated service submission; canonical
        // planning consumes the patch directly rather than re-parsing user text.
        if input.clarification_id.is_some() {
            memory.current_user_message_metadata["validated_constraint_patch"] =
                serde_json::to_value(&input.constraint_patch).unwrap_or_else(|_| json!({}));
            memory.current_user_message_metadata["clarification_id"] =
                json!(input.clarification_id);
            memory.current_user_message_metadata["clarification_revision"] =
                json!(input.clarification_revision);
            memory.current_user_message_metadata["structured_deterministic_extraction"] =
                serde_json::to_value(extract_for_context(&input.source_message, canonical))
                    .unwrap_or_else(|_| json!({}));
        }
        let message = input.message.as_str();
        let mut clear_pending_after_reroute = false;
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
        // A missing-parameter clarification already identifies one capability. Treat
        // any free-text reply (including an explicit "Others") as facts for that
        // capability rather than routing it as a fresh request.
        if let Some(payload) = &context.pending_clarification
            && payload.is_missing_execution_parameters
            && input
                .selected_option_id
                .as_deref()
                .is_none_or(|id| id.eq_ignore_ascii_case(OTHER_CLARIFICATION_OPTION_ID))
            && let Some(capability_id) = continuation_capability(payload)
        {
            let mut intent = intent_from_source(payload, &context, canonical);
            merge_deterministic_extraction_at(
                &mut memory,
                &mut intent,
                &input.source_message,
                canonical,
            );
            apply_constraint_patch(&mut intent, &input.constraint_patch);
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
                Some(payload),
                Some(None),
            )
            .await;
        }
        if let Some(payload) = &context.pending_clarification
            && let Some(outcome) = resolve_pending_clarification(&input, payload, &memory, &context)
        {
            let mut continuation_intent = intent_from_source(payload, &context, canonical);
            merge_deterministic_extraction_at(
                &mut memory,
                &mut continuation_intent,
                &input.source_message,
                canonical,
            );
            apply_constraint_patch(&mut continuation_intent, &input.constraint_patch);
            memory.intent = Some(continuation_intent);
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
                        Some(payload),
                        Some(None),
                    )
                    .await;
                }
                ClarificationOutcome::FreeFormOther { .. } => {
                    memory.retrieval_evidence = json!({ "clarification_outcome": "free_form_other", "clarification_id": payload.id, "clarification_revision": payload.revision, "clarification_kind": payload.kind });
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
                ClarificationOutcome::NewRequest { .. } => {
                    memory.retrieval_evidence = json!({ "clarification_outcome": "new_request", "clarification_id": payload.id, "clarification_revision": payload.revision, "clarification_kind": payload.kind });
                    clear_pending_after_reroute = true;
                }
                ClarificationOutcome::Unresolved { .. } => {
                    memory.retrieval_evidence = json!({ "clarification_outcome": "unresolved", "clarification_id": payload.id, "clarification_revision": payload.revision, "clarification_kind": payload.kind });
                    if payload.attempt >= MAX_CLARIFICATION_ATTEMPTS {
                        return graph_result(
                            memory,
                            TerminalState::WaitingForUserInput,
                            "clarification_recovery",
                            {
                                let mut response = ResponseBuilder::free_form_other_prompt();
                                response.title = Some("Describe your request".into());
                                response
                            },
                            context.recent_messages.len(),
                            Some(None),
                            clarification_transitions(
                                TerminalState::WaitingForUserInput,
                                "clarification_recovery",
                            ),
                        );
                    }
                    let next_payload = incremented_clarification(payload);
                    return graph_result(
                        memory,
                        TerminalState::WaitingForUserInput,
                        "clarification_unresolved",
                        ResponseBuilder::clarification(next_payload.clone()),
                        context.recent_messages.len(),
                        Some(Some(next_payload)),
                        clarification_transitions(
                            TerminalState::WaitingForUserInput,
                            "clarification_unresolved",
                        ),
                    );
                }
                _ => {}
            }
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
        let mut result = complete_semantic_route(
            memory,
            context,
            route,
            llm,
            knowledge,
            fineract_pool,
            catalog,
            client,
            canonical,
            input,
        )
        .await;
        if clear_pending_after_reroute && result.pending_clarification.is_none() {
            result.pending_clarification = Some(None);
        }
        result
    }
}
