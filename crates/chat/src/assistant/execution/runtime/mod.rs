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
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::tool::{normalize_effective_parameters, plan_from_snapshot};
use crate::assistant::execution::plan::PolicyDecisionStatus;
use crate::assistant::{
    AssistantConstraints, AssistantDomain, AssistantGraphTopology, AssistantIntent,
    AssistantIntentKind, AssistantLanguage, AssistantResponse, CanonicalStateRepository,
    ClarificationOption, ClarificationOutcome, ClarificationPayload, ClarificationResolver,
    ContextReference, ContextWarningCode, ContextWindow, DeterministicExtraction,
    ExtractionProvenance, FactSourceKind, GraphState, GraphTransition, JobMemory,
    OTHER_CLARIFICATION_OPTION_ID, OriginalIntent, PlannerInputSnapshot, PrincipalProjection,
    Quantity, ResponseBuilder, SemanticRouter, SourceIntentSnapshot, TerminalState,
    deterministic_observations,
    evidence::{Evidence, RetrievalPlan},
    executable_constraint_contracts, extract_message_facts, extract_message_facts_at,
    llm::SharedLlmClient,
    merge_observations, original_request_observations,
    presentation::builder::finish,
    reranker::{LlmReranker, RerankerDecision, RerankerVerdict},
    retrieval::RetrievalEngine,
    stable_uuid,
};
use crate::chat::executor::execute_plan;
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
        complete_semantic_route(
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
        .await
    }
}
