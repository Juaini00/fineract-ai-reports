use serde_json::json;
use std::sync::Arc;

use app_core::auth::model::ClientContext;
use sqlx::PgPool;

use crate::assistant::{
    AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage, AssistantResponse,
    AssistantResponseType, ClarificationOption, ClarificationOutcome, ClarificationPayload,
    ClarificationResolver, ContextReference, ContextWarningCode, ContextWindow, GraphState,
    GraphTransition, JobMemory, OTHER_CLARIFICATION_OPTION_ID, Quantity, ResponseBuilder,
    SemanticRouter, TerminalState,
    evidence::{Evidence, EvidenceDecision, EvidenceEvaluator, RetrievalPlan},
    llm::SharedLlmClient,
    response::{ResponseAction, ResponseActionType},
};
use crate::chat::{executor::execute_plan, planner::PolicyDecisionStatus};
use crate::knowledge::index::repository::KnowledgeRepository;
use crate::knowledge::model::KnowledgeCatalog;

#[derive(Debug, Clone)]
pub struct GraphRuntimeResult {
    pub memory: JobMemory,
    pub transitions: Vec<GraphTransition>,
    pub pending_clarification: Option<Option<ClarificationPayload>>,
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
        });
        GraphRuntimeResult {
            memory,
            transitions,
            pending_clarification: None,
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
        client: Option<&ClientContext>,
        message: &str,
    ) -> GraphRuntimeResult {
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
            && let Some(outcome) = ClarificationResolver::resolve_exact(message, payload)
        {
            memory.intent = Some(pending_clarification_intent(&context));
            match outcome {
                ClarificationOutcome::SelectedOption { option_id, .. } => {
                    memory.selected_capability = Some(option_id.clone());
                    memory.retrieval_evidence = json!({ "clarification_outcome": "selected_option", "option_id": option_id });
                    return execute_selected_capability(
                        memory,
                        context.recent_messages.len(),
                        option_id,
                        catalog,
                        client,
                        fineract_pool,
                        Some(None),
                    )
                    .await;
                }
                ClarificationOutcome::FreeFormOther { .. } => {
                    memory.retrieval_evidence =
                        json!({ "clarification_outcome": "free_form_other" });
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
                _ => {}
            }
        }
        let Some(router) = router else {
            let intent = fallback_intent(message);
            if intent.intent == AssistantIntentKind::UnsupportedInDomain {
                memory.intent = Some(intent);
                return graph_result(
                    memory,
                    TerminalState::Unsupported,
                    "unsupported_in_domain",
                    ResponseBuilder::unsupported(),
                    context.recent_messages.len(),
                    None,
                    simple_intent_transitions(TerminalState::Unsupported, "unsupported_in_domain"),
                );
            }
            if let Some(capability_id) = fallback_capability_hint(message, &context) {
                memory.intent = Some(intent);
                memory.selected_capability = Some(capability_id.clone());
                return execute_selected_capability(
                    memory,
                    context.recent_messages.len(),
                    capability_id,
                    catalog,
                    client,
                    fineract_pool,
                    Some(None),
                )
                .await;
            }
            let plan = RetrievalPlan::new(
                message,
                &intent,
                allow_all_capabilities(&context),
                allowed_capabilities(&context),
            );
            let evidence = Vec::new();
            let decision = EvidenceDecision::Clarify;
            let payload = clarification_payload(&plan, &evidence);
            memory.intent = Some(intent);
            memory.retrieval_evidence = json!({
                "plan": plan,
                "evidence": evidence,
                "decision": decision,
            });
            return graph_result(
                memory,
                TerminalState::WaitingForUserInput,
                "semantic_router_unavailable_fallback",
                ResponseBuilder::clarification(payload.clone()),
                context.recent_messages.len(),
                Some(Some(payload)),
                clarification_transitions(
                    TerminalState::WaitingForUserInput,
                    "semantic_router_unavailable_fallback",
                ),
            );
        };
        let route = router.route(message, &context).await;
        let mut pending_clarification = None;
        let (terminal, reason, response) = match route {
            Ok(intent) => {
                if intent.intent == AssistantIntentKind::ClarificationReply
                    && let (Some(payload), Some(llm)) = (&context.pending_clarification, llm)
                {
                    match ClarificationResolver::resolve(message, payload, &context, llm.as_ref())
                        .await
                    {
                        Ok(ClarificationOutcome::SelectedOption { option_id, .. })
                            if option_id == OTHER_CLARIFICATION_OPTION_ID =>
                        {
                            memory.intent = Some(intent);
                            memory.retrieval_evidence =
                                json!({ "clarification_outcome": "free_form_other" });
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
                            memory.intent = Some(intent);
                            memory.selected_capability = Some(option_id.clone());
                            memory.retrieval_evidence = json!({ "clarification_outcome": "selected_option", "option_id": option_id });
                            pending_clarification = Some(None);
                            return execute_selected_capability(
                                memory,
                                context.recent_messages.len(),
                                option_id,
                                catalog,
                                client,
                                fineract_pool,
                                pending_clarification,
                            )
                            .await;
                        }
                        Ok(ClarificationOutcome::FreeFormOther { .. }) => {
                            memory.intent = Some(intent);
                            memory.retrieval_evidence =
                                json!({ "clarification_outcome": "free_form_other" });
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
                if let Some(capability_id) = fallback_capability_hint(message, &context) {
                    memory.selected_capability = Some(capability_id.clone());
                    return execute_selected_capability(
                        memory,
                        context.recent_messages.len(),
                        capability_id,
                        catalog,
                        client,
                        fineract_pool,
                        Some(None),
                    )
                    .await;
                }
                let evidence = retrieve_evidence(&plan, llm, knowledge).await;
                let (evidence, warning) = match evidence {
                    Ok(evidence) => (evidence, None),
                    Err(error) => (Vec::new(), Some(error.to_string())),
                };
                let decision = EvidenceEvaluator::default().evaluate(&plan, &evidence);
                memory.retrieval_evidence = json!({
                    "plan": plan,
                    "evidence": evidence,
                    "decision": decision,
                });
                if let Some(message) = warning {
                    memory.warnings = json!([{ "message": message }]);
                }
                match decision {
                    EvidenceDecision::Select { capability_id } => {
                        memory.selected_capability = Some(capability_id.clone());
                        return execute_selected_capability(
                            memory,
                            context.recent_messages.len(),
                            capability_id,
                            catalog,
                            client,
                            fineract_pool,
                            None,
                        )
                        .await;
                    }
                    EvidenceDecision::Clarify => {
                        let payload = clarification_payload(&plan, &evidence);
                        pending_clarification = Some(Some(payload.clone()));
                        (
                            TerminalState::WaitingForUserInput,
                            "weak_retrieval_evidence",
                            ResponseBuilder::clarification(payload),
                        )
                    }
                    EvidenceDecision::UnsupportedInDomain => (
                        TerminalState::Unsupported,
                        "unsupported_in_domain",
                        ResponseBuilder::unsupported(),
                    ),
                    EvidenceDecision::OutOfDomain => (
                        TerminalState::OutOfDomain,
                        "out_of_domain",
                        ResponseBuilder::out_of_domain(),
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
        GraphRuntimeResult {
            memory,
            transitions: vec![
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
            ],
            pending_clarification,
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
        language: AssistantLanguage::En,
        entities: Vec::new(),
        constraints: crate::assistant::AssistantConstraints {
            quantity,
            ..Default::default()
        },
        context_reference: ContextReference::PendingClarification,
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

fn fallback_intent(message: &str) -> AssistantIntent {
    let lower = message.to_lowercase();
    let (domain, intent) = if lower.contains("loan") {
        (
            AssistantDomain::Loan,
            AssistantIntentKind::UnsupportedInDomain,
        )
    } else if lower.contains("charge") || lower.contains("fee") {
        (
            AssistantDomain::Savings,
            AssistantIntentKind::UnsupportedInDomain,
        )
    } else if lower.contains("tax") {
        (
            AssistantDomain::Tax,
            AssistantIntentKind::UnsupportedInDomain,
        )
    } else if lower.contains("accounting")
        || lower.contains("general ledger")
        || lower.contains("journal")
        || lower.contains("gl")
    {
        (
            AssistantDomain::Accounting,
            AssistantIntentKind::UnsupportedInDomain,
        )
    } else if lower.contains("client") || lower.contains("customer") || lower.contains("member") {
        (AssistantDomain::Client, AssistantIntentKind::ReportRequest)
    } else if lower.contains("office") || lower.contains("organization") || lower.contains("branch")
    {
        (
            AssistantDomain::Organization,
            AssistantIntentKind::ReportRequest,
        )
    } else {
        (AssistantDomain::Savings, AssistantIntentKind::ReportRequest)
    };
    AssistantIntent {
        intent,
        domain,
        language: AssistantLanguage::En,
        entities: Vec::new(),
        constraints: Default::default(),
        context_reference: ContextReference::None,
        confidence: 0.5,
        reason: "semantic_router_unavailable_fallback".into(),
    }
}

fn fallback_capability_hint(message: &str, context: &ContextWindow) -> Option<String> {
    let lower = message.to_lowercase();
    let capability_id = if lower.contains("top client")
        && lower.contains("savings account")
        && lower.contains("count")
    {
        "client_top_n_by_savings_account_count"
    } else {
        return None;
    };

    if allow_all_capabilities(context)
        || allowed_capabilities(context)
            .iter()
            .any(|allowed| allowed == capability_id)
    {
        Some(capability_id.into())
    } else {
        None
    }
}

async fn execute_selected_capability(
    mut memory: JobMemory,
    recent_message_count: usize,
    capability_id: String,
    catalog: Option<&Arc<KnowledgeCatalog>>,
    client: Option<&ClientContext>,
    fineract_pool: Option<&PgPool>,
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
    let Some(intent) = memory.intent.clone() else {
        return graph_result(
            memory,
            TerminalState::WaitingForUserInput,
            "missing_intent",
            ResponseBuilder::missing_parameter("Please include the client name to search for."),
            recent_message_count,
            pending_clarification.clone(),
            execution_transitions(TerminalState::WaitingForUserInput, "missing_intent"),
        );
    };
    let plan = match crate::assistant::plan_selected_capability(catalog, &capability_id, &intent) {
        Ok(plan) => plan,
        Err(error) => {
            return graph_result(
                memory,
                TerminalState::WaitingForUserInput,
                "missing_execution_parameters",
                ResponseBuilder::missing_parameter(&error.to_string()),
                recent_message_count,
                pending_clarification.clone(),
                execution_transitions(
                    TerminalState::WaitingForUserInput,
                    "missing_execution_parameters",
                ),
            );
        }
    };
    memory.selected_tool = Some(plan.query_id.clone());
    let policy = crate::assistant::guard_selected_capability(client, catalog, &plan);
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
            let response =
                ResponseBuilder::from_tool_result(&intent, &plan, &policy, &result, catalog);
            let mut result_state = graph_result(
                memory,
                TerminalState::Completed,
                "execution_completed",
                response,
                recent_message_count,
                pending_clarification.clone(),
                execution_transitions(TerminalState::Completed, "execution_completed"),
            );
            result_state.memory.execution_summary =
                json!({ "plan": plan, "policy": policy, "result": result });
            result_state
        }
        Err(error) => {
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
    memory.structured_response = Some(response);
    GraphRuntimeResult {
        memory,
        transitions,
        pending_clarification,
    }
}

async fn retrieve_evidence(
    plan: &RetrievalPlan,
    llm: Option<&SharedLlmClient>,
    knowledge: Option<&KnowledgeRepository>,
) -> anyhow::Result<Vec<Evidence>> {
    let (Some(llm), Some(knowledge)) = (llm, knowledge) else {
        return Ok(Vec::new());
    };
    let embedding = llm.embed(&plan.query_text).await?.vector;
    let capabilities = knowledge
        .search_capabilities(
            embedding.clone(),
            plan.allow_all_capabilities,
            &plan.allowed_capabilities,
            5,
        )
        .await?;
    let _context = knowledge.search_context(embedding, 5).await?;
    Ok(capabilities.into_iter().map(Into::into).collect())
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

fn clarification_payload(plan: &RetrievalPlan, evidence: &[Evidence]) -> ClarificationPayload {
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

    #[test]
    fn traverses_three_state_skeleton() {
        let memory = JobMemory {
            job_id: Uuid::nil(),
            graph_state: "receive_message".into(),
            terminal_state: None,
            intent: None,
            retrieval_evidence: json!({}),
            selected_capability: None,
            selected_tool: None,
            policy_decision: json!({}),
            execution_summary: json!({}),
            structured_response: None,
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
            client_scope: json!({}),
            warnings: Vec::new(),
        });

        assert_eq!(
            intent.constraints.quantity,
            Some(Quantity::TopN { value: 10 })
        );
    }

    struct FakeLlm;

    #[async_trait]
    impl LlmClient for FakeLlm {
        async fn structured_value(
            &self,
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

        async fn embed(&self, _text: &str) -> Result<EmbeddingResponse> {
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
    async fn route_retrieval_evidence_without_repository_clarifies() {
        let memory = JobMemory {
            job_id: Uuid::nil(),
            graph_state: "receive_message".into(),
            terminal_state: None,
            intent: None,
            retrieval_evidence: json!({}),
            selected_capability: None,
            selected_tool: None,
            policy_decision: json!({}),
            execution_summary: json!({}),
            structured_response: None,
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
            "show savings",
        )
        .await;

        assert_eq!(
            result.memory.terminal_state,
            Some(TerminalState::WaitingForUserInput)
        );
        assert_eq!(result.transitions.len(), 7);
        assert_eq!(result.memory.graph_state, "complete_or_wait");
    }

    #[tokio::test]
    async fn semantic_router_unavailable_uses_actionable_clarification() {
        let memory = JobMemory {
            job_id: Uuid::nil(),
            graph_state: "receive_message".into(),
            terminal_state: None,
            intent: None,
            retrieval_evidence: json!({}),
            selected_capability: None,
            selected_tool: None,
            policy_decision: json!({}),
            execution_summary: json!({}),
            structured_response: None,
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
            "show client savings",
        )
        .await;

        assert_eq!(
            result.memory.terminal_state,
            Some(TerminalState::WaitingForUserInput)
        );
        assert_eq!(
            result
                .pending_clarification
                .as_ref()
                .and_then(Option::as_ref)
                .map(|payload| payload
                    .options
                    .iter()
                    .any(|option| option.id == "client_top_n_by_savings_account_count")),
            Some(true)
        );
        assert_eq!(
            result.memory.structured_response.unwrap().response_type,
            AssistantResponseType::Clarification
        );
    }

    #[tokio::test]
    async fn exact_pending_option_id_resolves_before_router() {
        let memory = JobMemory {
            job_id: Uuid::nil(),
            graph_state: "receive_message".into(),
            terminal_state: None,
            intent: None,
            retrieval_evidence: json!({}),
            selected_capability: None,
            selected_tool: None,
            policy_decision: json!({}),
            execution_summary: json!({}),
            structured_response: None,
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
                    id: "client_top_n_by_savings_balance".into(),
                    label: "Top clients by savings balance".into(),
                    description: None,
                }],
                attempt: 1,
            }),
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
            "client_top_n_by_savings_balance",
        )
        .await;

        assert_eq!(
            result.memory.selected_capability.as_deref(),
            Some("client_top_n_by_savings_balance")
        );
        assert_eq!(result.pending_clarification, Some(None));
        assert_ne!(
            result.memory.structured_response.unwrap().response_type,
            AssistantResponseType::Clarification
        );
    }

    #[test]
    fn clarification_payload_always_includes_others_option() {
        let payload = clarification_payload(&test_plan(), &[]);
        assert!(payload.options.iter().any(|option| {
            option.id == OTHER_CLARIFICATION_OPTION_ID && option.label == "Others"
        }));
    }

    #[test]
    fn clarification_payload_empty_evidence_uses_real_capabilities() {
        let payload = clarification_payload(&test_plan(), &[]);

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

    #[tokio::test]
    async fn fallback_unsupported_domains_do_not_clarify_as_savings() {
        for (message, domain) in [
            ("show loan portfolio report", AssistantDomain::Loan),
            ("show savings charges and fees", AssistantDomain::Savings),
            ("show tax report", AssistantDomain::Tax),
        ] {
            let result = AssistantGraphRuntime::run_with_router(
                JobMemory {
                    job_id: Uuid::nil(),
                    graph_state: "receive_message".into(),
                    terminal_state: None,
                    intent: None,
                    retrieval_evidence: json!({}),
                    selected_capability: None,
                    selected_tool: None,
                    policy_decision: json!({}),
                    execution_summary: json!({}),
                    structured_response: None,
                    warnings: json!([]),
                    revision: 0,
                },
                ContextWindow {
                    summary: None,
                    active_domain: None,
                    selected_entities: json!({}),
                    recent_messages: Vec::new(),
                    relevant_jobs: Vec::new(),
                    pending_clarification: None,
                    client_scope: json!({}),
                    warnings: Vec::new(),
                },
                None,
                None,
                None,
                None,
                None,
                None,
                message,
            )
            .await;

            assert_eq!(
                result.memory.terminal_state,
                Some(TerminalState::Unsupported)
            );
            assert_eq!(result.memory.intent.as_ref().unwrap().domain, domain);
            assert_eq!(result.pending_clarification, None);
            assert_eq!(
                result.memory.structured_response.unwrap().response_type,
                AssistantResponseType::Unsupported
            );
        }
    }

    fn test_plan() -> RetrievalPlan {
        RetrievalPlan::new(
            "show savings",
            &AssistantIntent {
                intent: AssistantIntentKind::ReportRequest,
                domain: AssistantDomain::Savings,
                language: AssistantLanguage::En,
                entities: Vec::new(),
                constraints: Default::default(),
                context_reference: ContextReference::None,
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
            client_scope: json!({ "allow_all_capabilities": true, "capabilities": [] }),
            warnings: Vec::new(),
        };

        assert!(allow_all_capabilities(&context));
        assert!(allowed_capabilities(&context).is_empty());
    }
}
