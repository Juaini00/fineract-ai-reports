use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, header, request::Parts},
};

use crate::{api::AppState, api::error::ApiError, auth::model::ClientContext};

const X_API_KEY_HEADER: &str = "x-api-key";

pub struct AuthenticatedClient(pub ClientContext);

impl FromRequestParts<AppState> for AuthenticatedClient {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let raw_key = extract_api_key(&parts.headers)?
            .ok_or_else(|| ApiError::unauthorized("missing API key"))?;

        let client = state
            .auth_service
            .authenticate_api_key(&raw_key)
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::unauthorized("invalid API key"))?;

        Ok(Self(client))
    }
}

fn extract_api_key(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    let has_authorization = headers.contains_key(header::AUTHORIZATION);
    let has_x_api_key = headers.contains_key(X_API_KEY_HEADER);

    if has_authorization && has_x_api_key {
        return Err(ApiError::bad_request(
            "send API key using either Authorization or X-API-Key, not both",
        ));
    }

    let bearer_token = extract_bearer_token(headers);
    let x_api_key = extract_x_api_key(headers);

    Ok(bearer_token.or(x_api_key))
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ")?;
    non_empty_token(token)
}

fn extract_x_api_key(headers: &HeaderMap) -> Option<String> {
    let token = headers.get(X_API_KEY_HEADER)?.to_str().ok()?;
    non_empty_token(token)
}

fn non_empty_token(token: &str) -> Option<String> {
    let token = token.trim();

    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}
