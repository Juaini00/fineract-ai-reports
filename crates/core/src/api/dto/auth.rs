use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub(crate) struct CreateApiKeyRequest {
    #[validate(length(min = 1, message = "name is required"))]
    pub(crate) name: String,

    #[serde(default)]
    pub(crate) expires_at: Option<chrono::DateTime<chrono::Utc>>,

    #[serde(default)]
    pub(crate) allowed_office_ids: Vec<i64>,

    #[serde(default)]
    pub(crate) allowed_capabilities: Vec<String>,

    #[serde(default)]
    pub(crate) allow_all_offices: bool,

    #[serde(default)]
    pub(crate) allow_all_capabilities: bool,

    #[serde(default)]
    pub(crate) can_view_pii: bool,
}

#[derive(Debug, Deserialize, Validate)]
pub(crate) struct LoginRequest {
    #[validate(length(min = 1, message = "username is required"))]
    pub(crate) username: String,

    #[validate(length(min = 1, message = "password is required"))]
    pub(crate) password: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateApiKeyResponse {
    pub(crate) id: uuid::Uuid,
    pub(crate) api_key: String,
    pub(crate) message: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct LoginResponse {
    pub(crate) access_token: String,
    pub(crate) token_type: &'static str,
    pub(crate) expires_in: i64,
    pub(crate) user: crate::auth::model::UserProfile,
}

#[derive(Debug, Serialize)]
pub(crate) struct RefreshResponse {
    pub(crate) access_token: String,
    pub(crate) token_type: &'static str,
    pub(crate) expires_in: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct LogoutResponse {
    pub(crate) message: &'static str,
}
