use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use uuid::Uuid;

use crate::{api::error::ApiError, auth::service::AuthService};

use super::authenticated_user::AuthenticatedUser;

/// Bearer-authenticated actor for management endpoints.
///
/// Management deliberately does not inspect `X-API-Key`: API keys may narrow
/// chat office scope but never authenticate or authorize management access.
pub struct AuthenticatedManagementAdmin {
    pub user_id: Uuid,
    pub session_id: Uuid,
}

impl<S> FromRequestParts<S> for AuthenticatedManagementAdmin
where
    S: Send + Sync,
    AuthService: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthenticatedUser::from_request_parts(parts, state).await?;
        if user.role != "admin" {
            return Err(ApiError::forbidden_with_code(
                "role_not_authorized",
                "This role is not authorized to use management.",
            ));
        }

        Ok(Self {
            user_id: user.user_id,
            session_id: user.session_id,
        })
    }
}
