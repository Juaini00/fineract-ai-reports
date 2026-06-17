use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, header, request::Parts},
};

use crate::{api::AppState, api::error::ApiError, auth::model::ClientContext};

pub struct AuthenticatedClient(pub ClientContext);

impl FromRequestParts<AppState> for AuthenticatedClient {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let raw_key = extract_api_key(&parts.headers)
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

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    extract_bearer_token(headers).or_else(|| extract_x_api_key(headers))
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ")?;
    non_empty_token(token)
}

fn extract_x_api_key(headers: &HeaderMap) -> Option<String> {
    let token = headers.get("x-api-key")?.to_str().ok()?;
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
