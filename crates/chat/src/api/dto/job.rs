use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateChatJobRequest {
    pub session_id: Option<Uuid>,

    #[validate(length(
        min = 1,
        max = 1000,
        message = "Message must be between 1 and 1000 characters long"
    ))]
    pub message: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RespondToChatJobRequest {
    #[serde(default)]
    #[validate(length(max = 1000, message = "Message must be at most 1000 characters long"))]
    pub message: Option<String>,

    #[serde(default)]
    #[validate(length(max = 200, message = "Option id must be at most 200 characters long"))]
    pub option_id: Option<String>,

    #[serde(default)]
    pub clarification_id: Option<Uuid>,

    #[serde(default)]
    pub clarification_revision: Option<u32>,

    #[serde(default)]
    pub answers: BTreeMap<String, Value>,
}
