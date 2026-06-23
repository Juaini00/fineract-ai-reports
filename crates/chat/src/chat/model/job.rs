use app_core::auth::model::ClientContext;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CreateChatJobInput {
    pub client: ClientContext,
    pub session_id: Option<Uuid>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct RespondToChatJobInput {
    pub client: ClientContext,
    pub job_id: Uuid,
    pub message: String,
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
    pub api_key_id: Uuid,
    pub user_message_id: Option<Uuid>,
    pub status: String,
    pub current_step: String,
    pub resume_from_step: Option<String>,
    pub message: String,
    pub state_json: serde_json::Value,
    pub result_json: Option<serde_json::Value>,
    pub error_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
}
