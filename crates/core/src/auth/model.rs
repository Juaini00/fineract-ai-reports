use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CreateApiKeyInput {
    pub name: String,
    pub owner: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub allowed_office_ids: Vec<i64>,
    pub allowed_capabilities: Vec<String>,
    pub can_view_pii: bool,
}

#[derive(Debug, Clone)]
pub struct NewApiKeyRecord {
    pub id: Uuid,
    pub name: String,
    pub owner: String,
    pub key_prefix: String,
    pub key_hash: String,
    pub allowed_office_ids: Vec<i64>,
    pub allowed_capabilities: Vec<String>,
    pub can_view_pii: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct CreatedApiKey {
    pub id: Uuid,
    pub raw_key: String,
}

#[derive(Debug, Clone)]
pub struct ActiveApiKeyRecord {
    pub id: Uuid,
    pub name: String,
    pub owner: String,
    pub key_prefix: String,
    pub allowed_office_ids: Vec<i64>,
    pub allowed_capabilities: Vec<String>,
    pub can_view_pii: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientContext {
    pub api_key_id: Uuid,
    pub name: String,
    pub owner: String,
    pub key_prefix: String,
    pub allowed_office_ids: Vec<i64>,
    pub allowed_capabilities: Vec<String>,
    pub can_view_pii: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

impl From<ActiveApiKeyRecord> for ClientContext {
    fn from(record: ActiveApiKeyRecord) -> Self {
        Self {
            api_key_id: record.id,
            name: record.name,
            owner: record.owner,
            key_prefix: record.key_prefix,
            allowed_office_ids: record.allowed_office_ids,
            allowed_capabilities: record.allowed_capabilities,
            can_view_pii: record.can_view_pii,
            expires_at: record.expires_at,
        }
    }
}
