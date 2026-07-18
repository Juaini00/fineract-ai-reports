use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::clarification::ClarificationPayload;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextWindowPolicy {
    pub soft_token_limit: usize,
    pub hard_token_limit: usize,
    pub max_recent_messages: usize,
    pub max_relevant_jobs: usize,
}

impl ContextWindowPolicy {
    pub const fn new(
        soft_token_limit: usize,
        hard_token_limit: usize,
        max_recent_messages: usize,
        max_relevant_jobs: usize,
    ) -> Self {
        Self {
            soft_token_limit,
            hard_token_limit,
            max_recent_messages,
            max_relevant_jobs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContextWindow {
    pub summary: Option<String>,
    pub active_domain: Option<String>,
    #[serde(default)]
    pub selected_entities: serde_json::Value,
    #[serde(default)]
    pub recent_messages: Vec<ContextMessage>,
    #[serde(default)]
    pub relevant_jobs: Vec<RelevantJobSummary>,
    pub pending_clarification: Option<ClarificationPayload>,
    #[serde(default)]
    pub source_intent: Option<serde_json::Value>,
    #[serde(default)]
    pub source_snippets: Vec<ContextSourceSnippet>,
    #[serde(default)]
    pub client_scope: serde_json::Value,
    #[serde(default)]
    pub warnings: Vec<ContextWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RelevantJobSummary {
    pub job_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    pub summary: String,
    #[serde(default)]
    pub retrieval_plan: serde_json::Value,
    #[serde(default)]
    pub evidence_decision: serde_json::Value,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextSourceSnippet {
    pub source_id: String,
    pub source_type: String,
    pub title: Option<String>,
    pub snippet: String,
    #[serde(default)]
    pub score: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContextWarning {
    pub code: ContextWarningCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextWarningCode {
    SessionContextNearLimit,
    SessionContextExceeded,
}
