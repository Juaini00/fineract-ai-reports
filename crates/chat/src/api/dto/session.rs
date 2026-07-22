use serde::Deserialize;
use validator::{Validate, ValidationError};

#[derive(Debug, Deserialize, Validate)]
pub struct CreateChatSessionRequest {
    #[validate(length(max = 120, message = "Title must be at most 120 characters long"))]
    pub title: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RenameChatSessionRequest {
    #[validate(custom(function = "validate_title"))]
    pub title: String,
}

fn validate_title(title: &str) -> Result<(), ValidationError> {
    let length = title.trim().chars().count();
    if (1..=120).contains(&length) {
        Ok(())
    } else {
        Err(ValidationError::new("title_length"))
    }
}
