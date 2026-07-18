use schemars::schema_for;

use crate::assistant::{
    AssistantIntent, AssistantResponse, ClarificationOutcome, ClarificationPayload, ContextWindow,
    Evidence, GraphState, GraphTransition, JobMemory, MemoryDelta, PendingClarification,
    RerankerDecision, RetrievalPlan, SessionMemory, SourceIntentSnapshot, TerminalState,
    ToolRequest, ToolResult, ToolValidationError,
};

pub fn assistant_contract_names() -> &'static [&'static str] {
    &[
        "AssistantIntent",
        "SourceIntentSnapshot",
        "ClarificationPayload",
        "ClarificationOutcome",
        "PendingClarification",
        "GraphState",
        "TerminalState",
        "GraphTransition",
        "RetrievalPlan",
        "Evidence",
        "RerankerDecision",
        "ContextWindow",
        "JobMemory",
        "SessionMemory",
        "MemoryDelta",
        "ToolRequest",
        "ToolResult",
        "ToolValidationError",
        "AssistantResponse",
    ]
}

pub fn assistant_contract_schemas() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        (
            "AssistantIntent",
            serde_json::to_value(schema_for!(AssistantIntent)).unwrap(),
        ),
        (
            "SourceIntentSnapshot",
            serde_json::to_value(schema_for!(SourceIntentSnapshot)).unwrap(),
        ),
        (
            "ClarificationPayload",
            serde_json::to_value(schema_for!(ClarificationPayload)).unwrap(),
        ),
        (
            "ClarificationOutcome",
            serde_json::to_value(schema_for!(ClarificationOutcome)).unwrap(),
        ),
        (
            "PendingClarification",
            serde_json::to_value(schema_for!(PendingClarification)).unwrap(),
        ),
        (
            "GraphState",
            serde_json::to_value(schema_for!(GraphState)).unwrap(),
        ),
        (
            "TerminalState",
            serde_json::to_value(schema_for!(TerminalState)).unwrap(),
        ),
        (
            "GraphTransition",
            serde_json::to_value(schema_for!(GraphTransition)).unwrap(),
        ),
        (
            "RetrievalPlan",
            serde_json::to_value(schema_for!(RetrievalPlan)).unwrap(),
        ),
        (
            "Evidence",
            serde_json::to_value(schema_for!(Evidence)).unwrap(),
        ),
        (
            "RerankerDecision",
            serde_json::to_value(schema_for!(RerankerDecision)).unwrap(),
        ),
        (
            "ContextWindow",
            serde_json::to_value(schema_for!(ContextWindow)).unwrap(),
        ),
        (
            "JobMemory",
            serde_json::to_value(schema_for!(JobMemory)).unwrap(),
        ),
        (
            "SessionMemory",
            serde_json::to_value(schema_for!(SessionMemory)).unwrap(),
        ),
        (
            "MemoryDelta",
            serde_json::to_value(schema_for!(MemoryDelta)).unwrap(),
        ),
        (
            "ToolRequest",
            serde_json::to_value(schema_for!(ToolRequest)).unwrap(),
        ),
        (
            "ToolResult",
            serde_json::to_value(schema_for!(ToolResult)).unwrap(),
        ),
        (
            "ToolValidationError",
            serde_json::to_value(schema_for!(ToolValidationError)).unwrap(),
        ),
        (
            "AssistantResponse",
            serde_json::to_value(schema_for!(AssistantResponse)).unwrap(),
        ),
    ]
}
