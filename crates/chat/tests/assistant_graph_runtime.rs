use chat::assistant::{AssistantGraphTopology, GraphState, GraphTransition, TerminalState};

#[test]
fn legal_execution_transitions_are_accepted() {
    AssistantGraphTopology::new()
        .validate_sequence(&[
            transition(GraphState::ReceiveMessage, GraphState::BuildContextWindow),
            transition(GraphState::BuildContextWindow, GraphState::RouteIntent),
            transition(GraphState::RouteIntent, GraphState::PlanRetrieval),
            transition(GraphState::PlanRetrieval, GraphState::RetrieveKnowledge),
            transition(GraphState::RetrieveKnowledge, GraphState::EvaluateEvidence),
            transition(
                GraphState::EvaluateEvidence,
                GraphState::PlanToolOrCapability,
            ),
            transition(GraphState::PlanToolOrCapability, GraphState::GuardExecution),
            transition(GraphState::GuardExecution, GraphState::ExecuteToolOrSql),
            transition(
                GraphState::ExecuteToolOrSql,
                GraphState::BuildStructuredResponse,
            ),
            transition(
                GraphState::BuildStructuredResponse,
                GraphState::CompleteOrWait,
            ),
            terminal(GraphState::CompleteOrWait, TerminalState::Completed),
        ])
        .unwrap();
}

#[test]
fn legal_clarification_transitions_are_accepted() {
    AssistantGraphTopology::new()
        .validate_sequence(&[
            transition(GraphState::ReceiveMessage, GraphState::BuildContextWindow),
            transition(GraphState::BuildContextWindow, GraphState::RouteIntent),
            transition(GraphState::RouteIntent, GraphState::ResolveClarification),
            transition(GraphState::ResolveClarification, GraphState::CompleteOrWait),
            terminal(
                GraphState::CompleteOrWait,
                TerminalState::WaitingForUserInput,
            ),
        ])
        .unwrap();
}

#[test]
fn illegal_disconnected_chain_is_rejected() {
    let err = AssistantGraphTopology::new()
        .validate_sequence(&[
            transition(GraphState::ReceiveMessage, GraphState::BuildContextWindow),
            transition(GraphState::RouteIntent, GraphState::PlanRetrieval),
        ])
        .unwrap_err();
    assert!(err.to_string().contains("disconnected"));
}

#[test]
fn terminal_before_final_is_rejected() {
    let err = AssistantGraphTopology::new()
        .validate_sequence(&[
            terminal(GraphState::CompleteOrWait, TerminalState::Completed),
            transition(GraphState::ReceiveMessage, GraphState::BuildContextWindow),
        ])
        .unwrap_err();
    assert!(err.to_string().contains("terminal"));
}

#[test]
fn illegal_transition_is_rejected() {
    let err = AssistantGraphTopology::new()
        .validate_transition(&transition(
            GraphState::ReceiveMessage,
            GraphState::ExecuteToolOrSql,
        ))
        .unwrap_err();
    assert!(err.to_string().contains("illegal graph transition"));
}

#[test]
fn debug_edges_exposes_snapshot() {
    let snapshot = AssistantGraphTopology::new().debug_edges();
    assert!(snapshot.contains("ReceiveMessage -> BuildContextWindow"));
    assert!(snapshot.contains("CompleteOrWait -> terminal::Completed"));
}

fn transition(from: GraphState, to: GraphState) -> GraphTransition {
    GraphTransition {
        from,
        to: Some(to),
        terminal: None,
        reason: "test".into(),
    }
}

fn terminal(from: GraphState, terminal: TerminalState) -> GraphTransition {
    GraphTransition {
        from,
        to: None,
        terminal: Some(terminal),
        reason: "test".into(),
    }
}
