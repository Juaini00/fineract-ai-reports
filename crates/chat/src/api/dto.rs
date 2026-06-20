use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateChatSessionRequest {
    #[validate(length(max = 120, message = "Title must be at most 120 characters long"))]
    pub title: Option<String>,
}

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
