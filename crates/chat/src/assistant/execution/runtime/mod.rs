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
pub use planning::build_retrieval_trace;
use semantic::complete_semantic_route;
use transition::*;

use serde_json::json;
use std::sync::Arc;

use app_core::auth::model::PrincipalContext;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::assistant::temporal::BusinessDateSource;
use crate::assistant::understanding::extraction::SensitiveIdentifier;
use crate::assistant::workflow::WorkflowStateRepository;

use crate::assistant::execution::plan::PolicyDecisionStatus;
use crate::assistant::{
    AssistantDomain, AssistantEntityType, AssistantIntent, AssistantIntentKind, AssistantLanguage,
    AssistantResponse, CanonicalStateRepository, ClarificationFacts, ClarificationOption,
    ClarificationOutcome, ClarificationPayload, ClarificationPlanResult, ClarificationPlanner,
    ClarificationResolver, ConstraintField, ContextReference, ContextWarningCode, ContextWindow,
    DeterministicExtraction, GraphState, GraphTransition, JobMemory, LimitMode,
    OTHER_CLARIFICATION_OPTION_ID, Quantity, ResponseBuilder, SemanticRouter, SourceIntentSnapshot,
    TerminalState, TypedFactValue,
    evidence::{Evidence, RetrievalPlan},
    extract_message_facts, extract_message_facts_at,
    llm::SharedLlmClient,
    presentation::builder::finish,
    reranker::{LlmReranker, RerankerDecision, RerankerVerdict},
    retrieval::RetrievalEngine,
};
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

#[derive(Clone, Default)]
pub struct RuntimeUserInput {
    pub message: String,
    pub source_message: String,
    pub(crate) sensitive_identifier: Option<SensitiveIdentifier>,
    pub selected_option_id: Option<String>,
    pub clarification_id: Option<Uuid>,
    pub clarification_revision: Option<u32>,
    pub constraint_patch: crate::assistant::ConstraintPatch,
}

#[derive(Clone)]
pub struct CanonicalRuntimeContext {
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
            sensitive_identifier: None,
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
            sensitive_identifier: None,
            selected_option_id: None,
            clarification_id: None,
            clarification_revision: None,
            constraint_patch: Default::default(),
        }
    }
}

pub async fn run_with_router(
    mut memory: JobMemory,
    context: ContextWindow,
    router: Option<&SemanticRouter>,
    llm: Option<&SharedLlmClient>,
    knowledge: Option<&KnowledgeRepository>,
    fineract_pool: Option<&PgPool>,
    workflow_state: Option<&WorkflowStateRepository>,
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
        memory.current_user_message_metadata["clarification_id"] = json!(input.clarification_id);
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
        record_source_extraction_metadata(&mut memory, payload, canonical, &input.source_message);
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
            workflow_state,
            canonical,
            Some(payload),
            Some(None),
            input.sensitive_identifier.as_ref(),
            &input.source_message,
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
        record_source_extraction_metadata(&mut memory, payload, canonical, &input.source_message);
        match outcome {
            ClarificationOutcome::SelectedOption { option_id, .. } => {
                let selected_capability = option_id.clone();
                memory.selected_capability = Some(selected_capability);
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
                    workflow_state,
                    canonical,
                    Some(payload),
                    Some(None),
                    input.sensitive_identifier.as_ref(),
                    &input.source_message,
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
    // Greeting/help/out-of-domain/unsafe are classified by the understanding
    // boundary (SemanticRouter) and terminated in `complete_semantic_route`,
    // not by a keyword shortcut (issue-012 inventory item #6).
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
    crate::job::progress::started(crate::job::progress::Stage::Routing);
    let routing_started_at = std::time::Instant::now();
    let route = router.route(message, &context).await;
    crate::job::progress::finished(
        crate::job::progress::Stage::Routing,
        routing_started_at.elapsed().as_millis() as u64,
    );
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
            error = ?error,
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
        workflow_state,
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
