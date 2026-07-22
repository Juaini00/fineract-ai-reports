use app_core::auth::model::PrincipalContext;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CreateChatSessionInput {
    pub client: PrincipalContext,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RenameChatSessionInput {
    pub client: PrincipalContext,
    pub session_id: Uuid,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeleteChatSessionResponse {
    pub session_id: Uuid,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatSession {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub api_key_id: Option<Uuid>,
    pub title: Option<String>,
    pub status: String,
    pub context_json: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
}
