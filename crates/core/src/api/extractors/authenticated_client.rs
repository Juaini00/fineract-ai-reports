use axum::{
    extract::{FromRef, FromRequestParts},
    http::{HeaderMap, header, request::Parts},
};

use crate::{api::error::ApiError, auth::model::ClientContext, auth::service::AuthService};

const X_API_KEY_HEADER: &str = "x-api-key";

pub struct AuthenticatedClient(pub ClientContext);

impl<S> FromRequestParts<S> for AuthenticatedClient
where
    S: Send + Sync,
    AuthService: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let raw_key = extract_api_key(&parts.headers)?
            .ok_or_else(|| ApiError::unauthorized("missing API key"))?;

        let auth_service = AuthService::from_ref(state);
        let client = auth_service
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

    Ok(extract_bearer_token(headers).or_else(|| extract_x_api_key(headers)))
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
