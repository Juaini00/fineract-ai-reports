use serde::Deserialize;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateChatSessionRequest {
    #[validate(length(max = 120, message = "Title must be at most 120 characters long"))]
    pub title: Option<String>,
}
