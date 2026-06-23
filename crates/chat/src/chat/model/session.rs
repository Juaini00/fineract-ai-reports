use app_core::auth::model::ClientContext;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CreateChatSessionInput {
    pub client: ClientContext,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatSession {
    pub id: Uuid,
    pub api_key_id: Uuid,
    pub title: Option<String>,
    pub status: String,
    pub context_json: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
}
