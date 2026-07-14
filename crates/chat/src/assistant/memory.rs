use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    clarification::{ClarificationPayload, PendingClarification},
    graph::TerminalState,
    intent::AssistantIntent,
    response::AssistantResponse,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JobMemory {
    #[schemars(with = "String")]
    pub job_id: Uuid,
    pub graph_state: String,
    pub terminal_state: Option<TerminalState>,
    #[serde(default)]
    pub current_user_message_metadata: serde_json::Value,
    pub intent: Option<AssistantIntent>,
    pub source_intent: Option<serde_json::Value>,
    #[serde(default)]
    pub retrieval_plan: serde_json::Value,
    #[serde(default)]
    pub retrieval_evidence: serde_json::Value,
    #[serde(default)]
    pub evidence_decision: serde_json::Value,
    pub selected_capability: Option<String>,
    pub selected_tool: Option<String>,
    #[serde(default)]
    pub tool_params: serde_json::Value,
    #[serde(default)]
    pub policy_decision: serde_json::Value,
    #[serde(default)]
    pub execution_summary: serde_json::Value,
    pub structured_response: Option<AssistantResponse>,
    #[serde(default)]
    pub warnings: serde_json::Value,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionMemory {
    #[schemars(with = "String")]
    pub session_id: Uuid,
    pub summary: Option<String>,
    pub active_domain: Option<String>,
    pub pending_clarification: Option<ClarificationPayload>,
    pub pending_clarification_source_intent: Option<serde_json::Value>,
    #[serde(default)]
    pub pending: Option<PendingClarification>,
    #[serde(default)]
    pub entities: serde_json::Value,
    #[serde(default)]
    pub relevant_jobs: serde_json::Value,
    #[serde(default)]
    pub context_warnings: serde_json::Value,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum MemoryDelta {
    Job {
        #[schemars(with = "String")]
        job_id: Uuid,
        field: String,
        value: serde_json::Value,
    },
    Session {
        #[schemars(with = "String")]
        session_id: Uuid,
        field: String,
        value: serde_json::Value,
    },
}
