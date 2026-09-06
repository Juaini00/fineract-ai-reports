use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// `GraphState`/`GraphTransition` are the durable job-lifecycle audit vocabulary
// (persisted in checkpoints/events), retained per the issue-012 Phase 7 Task 7.3
// amendment. The legacy petgraph transition *validator* (issue inventory item
// #15: "topology validation, not orchestration") was deleted with the atomic
// runtime — the workflow engine (`workflow::graph`) is now the sole petgraph
// control plane.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
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
