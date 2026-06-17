use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T>
where
    T: Serialize,
{
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiErrorBody>,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

pub fn success<T>(status: StatusCode, data: T) -> impl IntoResponse
where
    T: Serialize,
{
    (
        status,
        Json(ApiResponse {
            success: true,
            data: Some(data),
            error: None,
        }),
    )
}

pub fn error(
    status: StatusCode,
    code: &'static str,
    message: impl Into<String>,
    details: Option<Value>,
) -> impl IntoResponse {
    (
        status,
        Json(ApiResponse::<Value> {
            success: false,
            data: None,
            error: Some(ApiErrorBody {
                code,
                message: message.into(),
                details,
            }),
        }),
    )
}
