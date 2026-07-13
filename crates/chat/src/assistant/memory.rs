use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    clarification::ClarificationPayload, graph::TerminalState, intent::AssistantIntent,
    response::AssistantResponse,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobMemory {
    pub job_id: Uuid,
    pub graph_state: String,
    pub terminal_state: Option<TerminalState>,
    pub intent: Option<AssistantIntent>,
    #[serde(default)]
    pub retrieval_evidence: serde_json::Value,
    pub selected_capability: Option<String>,
    pub selected_tool: Option<String>,
    #[serde(default)]
    pub policy_decision: serde_json::Value,
    #[serde(default)]
    pub execution_summary: serde_json::Value,
    pub structured_response: Option<AssistantResponse>,
    #[serde(default)]
    pub warnings: serde_json::Value,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMemory {
    pub session_id: Uuid,
    pub summary: Option<String>,
    pub active_domain: Option<String>,
    pub pending_clarification: Option<ClarificationPayload>,
    #[serde(default)]
    pub entities: serde_json::Value,
    pub revision: i64,
}
