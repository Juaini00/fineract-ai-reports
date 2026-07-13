use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphState {
    ReceiveMessage,
    BuildContextWindow,
    RouteIntent,
    ResolveClarification,
    PlanRetrieval,
    RetrieveKnowledge,
    EvaluateEvidence,
    PlanToolOrCapability,
    GuardExecution,
    ExecuteToolOrSql,
    BuildStructuredResponse,
    RenderResponse,
    CompleteOrWait,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
    Completed,
    WaitingForUserInput,
    Unsupported,
    OutOfDomain,
    BlockedByPolicy,
    ContextWindowExceeded,
    FailedOperational,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GraphTransition {
    pub from: GraphState,
    pub to: Option<GraphState>,
    pub terminal: Option<TerminalState>,
    pub reason: String,
}
