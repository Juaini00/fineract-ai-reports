use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};

use crate::{api::error::ApiError, auth::model::PrincipalContext, auth::service::AuthService};

use super::authenticated_client::{AuthenticatedClient, X_API_KEY_HEADER};
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

        // Carry the API key's office restriction into the principal so downstream
        // admin projection can intersect (not overwrite) the tenant office set.
        // An empty scope means "unrestricted", but that is only legitimate when
        // NO X-API-Key accompanies the request (bearer-only callers) or the key
        // sets `allow_all_offices`. A present-but-invalid key must fail closed —
        // propagate the auth error rather than silently escalating to full tenant.
        // A restricted key contributes its `allowed_office_ids`; the intersection
        // happens in `chat::policy::authorization::project_admin_principal`.
        let office_ids = if parts.headers.contains_key(X_API_KEY_HEADER) {
            let AuthenticatedClient(client) =
                AuthenticatedClient::from_request_parts(parts, state).await?;
            if client.allow_all_offices {
                Vec::new()
            } else {
                client.allowed_office_ids
            }
        } else {
            Vec::new()
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
