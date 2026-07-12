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
    pub allow_all_offices: bool,
    pub allow_all_capabilities: bool,
    pub can_view_pii: bool,
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct NewApiKeyRecord {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub name: String,
    pub owner: String,
    pub key_prefix: String,
    pub key_hash: String,
    pub allowed_office_ids: Vec<i64>,
    pub allowed_capabilities: Vec<String>,
    pub allow_all_offices: bool,
    pub allow_all_capabilities: bool,
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
    pub user_id: Option<Uuid>,
    pub name: String,
    pub owner: String,
    pub key_prefix: String,
    pub allowed_office_ids: Vec<i64>,
    pub allowed_capabilities: Vec<String>,
    pub allow_all_offices: bool,
    pub allow_all_capabilities: bool,
    pub can_view_pii: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientContext {
    pub api_key_id: Uuid,
    pub user_id: Option<Uuid>,
    pub name: String,
    pub owner: String,
    pub key_prefix: String,
    pub allowed_office_ids: Vec<i64>,
    pub allowed_capabilities: Vec<String>,
    pub allow_all_offices: bool,
    pub allow_all_capabilities: bool,
    pub can_view_pii: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

impl From<ActiveApiKeyRecord> for ClientContext {
    fn from(record: ActiveApiKeyRecord) -> Self {
        Self {
            api_key_id: record.id,
            user_id: record.user_id,
            name: record.name,
            owner: record.owner,
            key_prefix: record.key_prefix,
            allowed_office_ids: record.allowed_office_ids,
            allowed_capabilities: record.allowed_capabilities,
            allow_all_offices: record.allow_all_offices,
            allow_all_capabilities: record.allow_all_capabilities,
            can_view_pii: record.can_view_pii,
            expires_at: record.expires_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub full_name: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct UserRecord {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub full_name: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewUserRecord {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub full_name: Option<String>,
    pub role: String,
}

#[derive(Debug, Clone)]
pub struct LoginInput {
    pub username: String,
    pub password: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoginResult {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    pub user: UserProfile,
}

#[derive(Debug, Clone)]
pub struct RefreshResult {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
}

#[derive(Debug, Clone)]
pub struct IssuedRefreshToken {
    pub id: Uuid,
    pub raw_token: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewSessionRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewRefreshTokenRecord {
    pub id: Uuid,
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
}
