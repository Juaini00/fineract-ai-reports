use serde::Deserialize;
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
    #[validate(length(
        min = 1,
        max = 1000,
        message = "Message must be between 1 and 1000 characters long"
    ))]
    pub message: String,
}
