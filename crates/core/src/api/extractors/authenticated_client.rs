use axum::{
    extract::{FromRef, FromRequestParts},
    http::{HeaderMap, request::Parts},
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
            .ok_or_else(|| ApiError::unauthorized("missing X-API-Key header"))?;

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
    Ok(extract_x_api_key(headers))
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
