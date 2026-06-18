use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};

use crate::{
    api::{
        AppState,
        dto::auth::{AuthMeResponse, CreateApiKeyRequest, CreateApiKeyResponse},
        error::ApiError,
        extractors::{authenticated_client::AuthenticatedClient, validated_json::ValidatedJson},
        response,
    },
    auth::model::CreateApiKeyInput,
};

pub(crate) async fn create_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    ValidatedJson(request): ValidatedJson<CreateApiKeyRequest>,
) -> Result<Response, ApiError> {
    authorize_bootstrap_admin(&state, &headers)?;

    let created = state
        .auth_service
        .create_api_key(CreateApiKeyInput {
            name: request.name,
            owner: request.owner,
            expires_at: request.expires_at,
            allowed_office_ids: request.allowed_office_ids,
            allowed_capabilities: request.allowed_capabilities,
            can_view_pii: request.can_view_pii,
        })
        .await
        .map_err(ApiError::internal)?;

    Ok(response::success(
        StatusCode::CREATED,
        CreateApiKeyResponse {
            id: created.id,
            api_key: created.raw_key,
            message: "Store this API key securely. It will not be shown again.",
        },
    )
    .into_response())
}

pub(crate) async fn get_me(
    AuthenticatedClient(client): AuthenticatedClient,
) -> Result<Response, ApiError> {
    Ok(response::success(
        StatusCode::OK,
        AuthMeResponse {
            auth_type: "api_key",
            client,
        },
    )
    .into_response())
}

fn authorize_bootstrap_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let token = extract_bearer_token(headers)
        .ok_or_else(|| ApiError::unauthorized("missing Authorization: Bearer <bootstrap_token>"))?;

    if token == state.config.auth.bootstrap_admin_token {
        Ok(())
    } else {
        Err(ApiError::forbidden("invalid bootstrap admin token"))
    }
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = value.strip_prefix("Bearer ")?;
    Some(token.to_string())
}
