use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};

use crate::{api::error::ApiError, auth::model::PrincipalContext, auth::service::AuthService};

use super::authenticated_client::AuthenticatedClient;
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

        // Bearer is authoritative: a valid session authenticates the request no
        // matter what the X-API-Key is. A *valid* key may still narrow office
        // scope, contributing its `allowed_office_ids` (intersected downstream in
        // `chat::policy::authorization::project_admin_principal`). An absent,
        // invalid, revoked, expired, or ownerless key contributes no restriction
        // (empty scope = the bearer admin's full tenant reach). This cannot
        // escalate — the bearer already grants full tenant access — so a bad key
        // must never turn a bearer-authenticated request into a 401.
        let office_ids = match AuthenticatedClient::from_request_parts(parts, state).await {
            Ok(AuthenticatedClient(client)) if !client.allow_all_offices => {
                client.allowed_office_ids
            }
            _ => Vec::new(),
        };

        Ok(Self(PrincipalContext {
            user_id: user.user_id,
            role: user.role,
            capability_ids: Vec::new(),
            office_ids,
            can_view_pii: false,
            legacy_api_key_id: None,
        }))
    }
}
