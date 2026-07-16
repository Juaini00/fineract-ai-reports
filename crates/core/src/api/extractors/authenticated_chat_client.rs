use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};

use crate::{api::error::ApiError, auth::model::PrincipalContext, auth::service::AuthService};

use super::authenticated_user::AuthenticatedUser;

pub struct AuthenticatedChatClient(pub PrincipalContext);

impl<S> FromRequestParts<S> for AuthenticatedChatClient
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
                "This role is not authorized to use chat.",
            ));
        }

        Ok(Self(PrincipalContext {
            user_id: user.user_id,
            role: user.role,
            capability_ids: Vec::new(),
            office_ids: Vec::new(),
            can_view_pii: false,
            legacy_api_key_id: None,
        }))
    }
}
