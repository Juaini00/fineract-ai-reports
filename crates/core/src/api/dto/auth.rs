use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub(crate) struct CreateApiKeyRequest {
    #[validate(length(min = 1, message = "name is required"))]
    pub(crate) name: String,

    #[validate(length(min = 1, message = "owner is required"))]
    pub(crate) owner: String,

    #[serde(default)]
    pub(crate) expires_at: Option<chrono::DateTime<chrono::Utc>>,

    #[serde(default)]
    pub(crate) allowed_office_ids: Vec<i64>,

    #[validate(length(min = 1, message = "at least one capability is required"))]
    pub(crate) allowed_capabilities: Vec<String>,

    #[serde(default)]
    pub(crate) can_view_pii: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateApiKeyResponse {
    pub(crate) id: uuid::Uuid,
    pub(crate) api_key: String,
    pub(crate) message: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct AuthMeResponse {
    pub(crate) auth_type: &'static str,
    pub(crate) client: crate::auth::model::ClientContext,
}
