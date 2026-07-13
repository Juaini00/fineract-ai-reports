use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};

use crate::{api::error::ApiError, auth::model::ClientContext, auth::service::AuthService};

use super::{authenticated_client::AuthenticatedClient, authenticated_user::AuthenticatedUser};

pub struct AuthenticatedChatClient(pub ClientContext);

impl<S> FromRequestParts<S> for AuthenticatedChatClient
where
    S: Send + Sync,
    AuthService: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthenticatedUser::from_request_parts(parts, state).await?;
        let AuthenticatedClient(client) =
            AuthenticatedClient::from_request_parts(parts, state).await?;

        if client.user_id != Some(user.user_id) {
            return Err(ApiError::forbidden(
                "API key does not belong to access token user",
            ));
        }

        Ok(Self(client))
    }
}
