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
    GraphRuntimeResult {
        memory,
        transitions,
        pending_clarification,
        retrieval_trace: None,
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
