use std::collections::BTreeMap;

use app_core::auth::model::PrincipalContext;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::assistant::ConstraintPatch;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CreateChatJobInput {
    pub client: PrincipalContext,
    pub session_id: Option<Uuid>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct RespondToChatJobInput {
    pub client: PrincipalContext,
    pub job_id: Uuid,
    pub clarification_id: Option<Uuid>,
    pub clarification_revision: Option<u32>,
    pub selected_option_id: Option<String>,
    pub source_message: Option<String>,
    pub answers: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct ValidatedClarificationSubmission {
    pub clarification_id: Option<Uuid>,
    pub clarification_revision: Option<u32>,
    pub selected_option_id: Option<String>,
    pub source_message: String,
    pub display_message: String,
    pub answers: BTreeMap<String, Value>,
    pub constraint_patch: ConstraintPatch,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedChatJob {
    pub session_id: Uuid,
    pub job_id: Uuid,
    pub user_message_id: Uuid,
    pub status: String,
    pub current_step: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatJob {
    pub id: Uuid,
    pub session_id: Uuid,
    pub user_id: Option<Uuid>,
    pub api_key_id: Option<Uuid>,
    pub user_message_id: Option<Uuid>,
    pub status: String,
    pub current_step: String,
    pub resume_from_step: Option<String>,
    pub message: String,
    pub state_json: serde_json::Value,
    pub state_revision: i64,
    pub result_json: Option<serde_json::Value>,
    pub error_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatJobAuditTimeline {
    pub job_id: Uuid,
    pub events: Vec<ChatJobAuditEvent>,
}

/// A durable row from `chat_job_events` (the SSE event log), replayed to a
/// client that subscribes after the live pub/sub race is already over.
#[derive(Debug, Clone)]
pub struct ChatJobEvent {
    pub event_type: String,
    pub step: Option<String>,
    pub payload_json: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatJobAuditEvent {
    pub id: Uuid,
    pub job_id: Uuid,
    pub session_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub api_key_id: Option<Uuid>,
    pub event_type: String,
    pub stage: String,
    pub layer: String,
    pub blueprint_step: Option<String>,
    pub status: String,
    pub duration_ms: Option<i64>,
    pub input_summary_json: serde_json::Value,
    pub output_summary_json: serde_json::Value,
    pub decision_json: serde_json::Value,
    pub flags_json: serde_json::Value,
    pub error_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}
