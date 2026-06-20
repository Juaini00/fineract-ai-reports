use axum::{
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Value, json};
use tracing::warn;
use validator::{ValidationErrors, ValidationErrorsKind};

use crate::api::response;
use crate::auth::authorization::AuthorizationError;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    details: Option<Value>,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request",
            message: message.into(),
            details: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
            details: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: message.into(),
            details: None,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "forbidden",
            message: message.into(),
            details: None,
        }
    }

    pub fn internal(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: error.to_string(),
            details: None,
        }
    }

    pub fn invalid_json(rejection: JsonRejection) -> Self {
        let body_text = rejection.body_text();
        warn!(error = %body_text, "invalid JSON request body");

        let details = invalid_json_details(&body_text);

        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request_body",
            message: "Request body is invalid. Check JSON syntax and field types.".to_string(),
            details: Some(details),
        }
    }

    pub fn validation(errors: ValidationErrors) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "validation_error",
            message: "Request validation failed.".to_string(),
            details: Some(json!({ "fields": validation_fields(errors) })),
        }
    }
}

fn invalid_json_details(body_text: &str) -> Value {
    if let Some(field) = extract_missing_field(body_text) {
        return json!({
            "reason": "missing_field",
            "hint": "Provide the required field with the correct type.",
            "fields": [{
                "field": field,
                "code": "required",
                "message": "field is required"
            }]
        });
    }

    if let Some(field) = extract_invalid_type_field(body_text) {
        return json!({
            "reason": "invalid_field_type",
            "hint": "Check the field type and send the expected JSON type.",
            "fields": [{
                "field": field,
                "code": "invalid_type",
                "message": "field has an invalid type"
            }]
        });
    }

    json!({
        "reason": "invalid_json",
        "hint": "Ensure the request body is valid JSON and matches the endpoint contract."
    })
}

fn extract_missing_field(body_text: &str) -> Option<String> {
    let marker = "missing field `";
    let start = body_text.find(marker)? + marker.len();
    let rest = &body_text[start..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}

fn extract_invalid_type_field(body_text: &str) -> Option<String> {
    let marker = "target type: ";
    let start = body_text.find(marker)? + marker.len();
    let rest = &body_text[start..];
    let end = rest.find(": invalid type")?;
    let field = rest[..end].trim();

    if field.is_empty() {
        None
    } else {
        Some(field.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        response::error(self.status, self.code, self.message, self.details).into_response()
    }
}

#[derive(Debug, Serialize)]
struct FieldError {
    field: String,
    code: String,
    message: String,
}

fn validation_fields(errors: ValidationErrors) -> Vec<FieldError> {
    let mut fields = Vec::new();

    for (field, kind) in errors.into_errors() {
        collect_validation_kind(field.to_string(), kind, &mut fields);
    }

    fields
}

fn collect_validation_kind(
    field: String,
    kind: ValidationErrorsKind,
    fields: &mut Vec<FieldError>,
) {
    match kind {
        ValidationErrorsKind::Field(errors) => {
            for error in errors {
                fields.push(FieldError {
                    field: field.clone(),
                    code: error.code.to_string(),
                    message: error
                        .message
                        .map(|message| message.to_string())
                        .unwrap_or_else(|| "field is invalid".to_string()),
                });
            }
        }
        ValidationErrorsKind::Struct(errors) => {
            for (nested_field, nested_kind) in errors.into_errors() {
                collect_validation_kind(format!("{field}.{nested_field}"), nested_kind, fields);
            }
        }
        ValidationErrorsKind::List(errors) => {
            for (index, nested_errors) in errors {
                for (nested_field, nested_kind) in nested_errors.into_errors() {
                    collect_validation_kind(
                        format!("{field}[{index}].{nested_field}"),
                        nested_kind,
                        fields,
                    );
                }
            }
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::internal(error)
    }
}

impl From<AuthorizationError> for ApiError {
    fn from(error: AuthorizationError) -> Self {
        Self::forbidden(error.to_string())
    }
}
