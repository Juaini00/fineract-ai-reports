use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub session_id: Uuid,
    pub job_id: Option<Uuid>,
    pub role: String,
    pub metadata_json: serde_json::Value,
    pub content: String,
    pub created_at: DateTime<Utc>,
}
