use super::*;

pub(super) fn graph_result(
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

pub(super) fn deterministic_simple_response(
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

pub(super) fn deterministic_intent(intent: AssistantIntentKind, message: &str) -> AssistantIntent {
    AssistantIntent {
        intent,
        domain: AssistantDomain::Unknown,
        request_shape: Default::default(),
        language: AssistantLanguage::En,
        canonical_query_en: message.to_string(),
        entities: Vec::new(),
        constraints: AssistantConstraints::default(),
        context_reference: ContextReference::None,
        source: None,
        confidence: 1.0,
        reason: format!("deterministic simple intent: {message}"),
    }
}
pub(super) fn simple_intent_transitions(
    terminal: TerminalState,
    reason: &str,
) -> Vec<GraphTransition> {
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

pub(super) fn execution_transitions(terminal: TerminalState, reason: &str) -> Vec<GraphTransition> {
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
