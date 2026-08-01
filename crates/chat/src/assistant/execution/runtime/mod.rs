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
    AssistantConstraints, AssistantDomain, AssistantEntityType, AssistantGraphTopology,
    AssistantIntent, AssistantIntentKind, AssistantLanguage, AssistantResponse,
    CanonicalStateRepository, ClarificationFacts, ClarificationOption, ClarificationOutcome,
    ClarificationPayload, ClarificationPlanResult, ClarificationPlanner, ClarificationResolver,
    ConstraintField, ContextReference, ContextWarningCode, ContextWindow, DeterministicExtraction,
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

/// Minimal fact projection for the pre-execution required-input gate. Only
/// fields that back a defaultless required parameter matter today (person name
/// for `search`); dates/limits are handled by policy defaults, not asked.
pub(super) fn clarification_facts_from_intent(
    intent: Option<&AssistantIntent>,
) -> ClarificationFacts {
    let mut values = std::collections::BTreeMap::new();
    if let Some(intent) = intent
        && let Some(entity) = intent
            .entities
            .iter()
            .find(|e| e.entity_type == AssistantEntityType::PersonName)
        && !entity.value.trim().is_empty()
    {
        values.insert(
            ConstraintField::PersonName,
            TypedFactValue::PersonName(entity.value.trim().to_string()),
        );
    }
    ClarificationFacts { values }
}

/// Route a chat request through the Layer-1 gateway → Layer-2 resolver →
/// Layer-3 decider pipeline built by Bundle 12. This is the drop-in entry
/// point spec §7 Task 7.1 steps 3–4 will use once the runtime graph fully
/// switches over; the default `run_with_router` path stays on the legacy
/// classifier to keep every existing test green.
///
/// ponytail: the runtime graph mapping (DecisionOutcome →
/// `terminal_state` / `pending_clarification` / `execution_summary` /
/// `ResponseBuilder`) is deliberately deferred to a fresh session where a
/// full flow trace and per-test verification can land safely. Callers who
/// want early access can invoke this helper directly.
pub async fn route_via_gateway_pipeline(
    llm: crate::assistant::llm::SharedLlmClient,
    catalog: &KnowledgeCatalog,
    principal: &PrincipalContext,
    user_message: &str,
    history: Option<&str>,
    business_today: chrono::NaiveDate,
) -> Result<
    crate::assistant::understanding::pipeline::PipelineOutcome,
    crate::assistant::understanding::gateway::GatewayError,
> {
    let gateway = crate::assistant::understanding::gateway::GatewayClient::new(llm);
    crate::assistant::understanding::pipeline::run(
        &gateway,
        catalog,
        principal,
        user_message,
        history,
        business_today,
    )
    .await
}

/// Map a Bundle-12 `DecisionOutcome` onto `GraphRuntimeResult` so the pipeline
/// is a complete alternate entry point to `run_with_router`. Callers opt in by
/// calling this instead of `run_with_router`; nothing routes traffic here by
/// default, so no existing behaviour changes.
///
/// - `Execute` → `memory.selected_capability` + `TerminalState::Completed` +
///   `ResponseBuilder::selected` (final wiring to `execute_selected_capability`
///   with an actual DB call is deferred; the mapping records the decision).
/// - `Clarify` → `TerminalState::WaitingForUserInput` +
///   `ClarificationPayload::CollectFields` with the reported missing fields.
/// - `Reject` → `TerminalState::FailedOperational` + sanitized error response,
///   reason string carries the reject code.
pub async fn run_via_gateway_pipeline(
    mut memory: JobMemory,
    context: ContextWindow,
    llm: crate::assistant::llm::SharedLlmClient,
    catalog: &KnowledgeCatalog,
    principal: &PrincipalContext,
    business_today: chrono::NaiveDate,
    input: impl Into<RuntimeUserInput>,
) -> GraphRuntimeResult {
    let input = input.into();
    let history = context
        .recent_messages
        .last()
        .map(|message| message.content.as_str());
    let recent_message_count = context.recent_messages.len();
    let outcome = match route_via_gateway_pipeline(
        llm,
        catalog,
        principal,
        &input.source_message,
        history,
        business_today,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            tracing::warn!(
                target: "assistant::runtime",
                error = %error,
                "gateway pipeline failed; falling back to error terminal",
            );
            return graph_result(
                memory,
                TerminalState::FailedOperational,
                "gateway_extraction_failed",
                ResponseBuilder::error(),
                recent_message_count,
                None,
                simple_intent_transitions(
                    TerminalState::FailedOperational,
                    "gateway_extraction_failed",
                ),
            );
        }
    };
    memory.current_user_message_metadata["llm_extraction"] =
        serde_json::to_value(&outcome.extraction).unwrap_or_else(|_| json!({}));
    memory.intent = Some(
        crate::assistant::understanding::pipeline::assistant_intent_from_extraction(
            &outcome.extraction,
            &input.source_message,
        ),
    );
    use crate::assistant::understanding::decider::DecisionOutcome;
    match outcome.decision {
        DecisionOutcome::Execute {
            capability_id,
            parameters,
        } => {
            memory.selected_capability = Some(capability_id.clone());
            memory.execution_summary = json!({
                "plan": {
                    "capability_id": capability_id,
                    "resolved_parameters": parameters
                        .into_iter()
                        .map(|(name, p)| (name, format!("{:?}", p.value)))
                        .collect::<std::collections::BTreeMap<_, _>>(),
                }
            });
            graph_result(
                memory,
                TerminalState::Completed,
                "gateway_pipeline_execute",
                ResponseBuilder::selected(capability_id),
                recent_message_count,
                None,
                simple_intent_transitions(TerminalState::Completed, "gateway_pipeline_execute"),
            )
        }
        DecisionOutcome::Clarify { missing_fields } => {
            let payload = ClarificationPayload {
                version: crate::assistant::clarification::CLARIFICATION_VERSION_1,
                id: uuid::Uuid::new_v4(),
                revision: 0,
                kind: crate::assistant::clarification::ClarificationKind::CollectFields,
                question: "What details should I use for this report?".into(),
                options: Vec::new(),
                fields: missing_fields
                    .into_iter()
                    .map(|key| crate::assistant::ClarificationField {
                        key,
                        label: String::new(),
                        field_type: crate::assistant::ClarificationFieldType::Text,
                        required: true,
                        value: None,
                        default_value: None,
                        help_text: None,
                        validation: Default::default(),
                        errors: Vec::new(),
                    })
                    .collect(),
                attempt: 0,
                source_intent: None,
                allow_free_text: false,
                is_missing_execution_parameters: true,
            };
            graph_result(
                memory,
                TerminalState::WaitingForUserInput,
                "gateway_pipeline_clarify",
                ResponseBuilder::clarification(payload.clone()),
                recent_message_count,
                Some(Some(payload)),
                simple_intent_transitions(
                    TerminalState::WaitingForUserInput,
                    "gateway_pipeline_clarify",
                ),
            )
        }
        DecisionOutcome::Reject { code } => graph_result(
            memory,
            TerminalState::FailedOperational,
            code,
            ResponseBuilder::error(),
            recent_message_count,
            None,
            simple_intent_transitions(TerminalState::FailedOperational, code),
        ),
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
        // Bundle 12: opt in to the LLM gateway pipeline via env var. When on and
        // llm + catalog + client are available, route through Layers 1-3 instead
        // of the classifier; extraction + intent land on memory the same way the
        // legacy path does, so downstream execution/audit is unchanged.
        if std::env::var("AI_REPORT_GATEWAY_PIPELINE").as_deref() == Ok("on")
            && let (Some(llm), Some(catalog), Some(client), Some(canonical)) =
                (llm, catalog, client, canonical)
        {
            return run_via_gateway_pipeline(
                memory,
                context,
                llm.clone(),
                catalog.as_ref(),
                client,
                canonical.business_today,
                input,
            )
            .await;
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
