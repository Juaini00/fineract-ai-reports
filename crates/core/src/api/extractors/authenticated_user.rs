use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts},
};
use uuid::Uuid;

use crate::{api::error::ApiError, auth::service::AuthService};

pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub role: String,
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
    AuthService: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::unauthorized("missing access token"))?;

        let user = AuthService::from_ref(state)
            .authenticate_access_token(token)
            .await
            .map_err(|_| ApiError::internal(anyhow::anyhow!("authentication service unavailable")))?
            .ok_or_else(|| ApiError::unauthorized("invalid access token"))?;

        Ok(Self {
            user_id: user.user_id,
            session_id: user.session_id,
            role: user.role,
        })
    }
}
