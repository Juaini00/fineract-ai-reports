use app_core::api::{
    error::ApiError,
    extractors::{authenticated_client::AuthenticatedClient, validated_json::ValidatedJson},
    response,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::info;
use uuid::Uuid;

use crate::api::ChatAppState;
use crate::api::dto::session::CreateChatSessionRequest;
use crate::chat::model::CreateChatSessionInput;

#[tracing::instrument(skip(state, client, request), fields(api_key_id = %client.api_key_id))]
pub async fn create(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ChatAppState>,
    ValidatedJson(request): ValidatedJson<CreateChatSessionRequest>,
) -> Result<Response, ApiError> {
    let session = state
        .chat
        .sessions
        .create(CreateChatSessionInput {
            client,
            title: request.title,
        })
        .await
        .map_err(ApiError::internal)?;

    info!(session_id = %session.id, "chat session created");

    Ok(response::success(StatusCode::CREATED, session).into_response())
}

#[tracing::instrument(skip(state, client), fields(api_key_id = %client.api_key_id, session_id = %session_id))]
pub async fn get(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ChatAppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let Some(session) = state
        .chat
        .sessions
        .get(client, session_id)
        .await
        .map_err(ApiError::internal)?
    else {
        return Err(ApiError::not_found("chat session not found"));
    };

    info!(session_id = %session.id, "chat session fetched");

    Ok(response::success(StatusCode::OK, session).into_response())
}

#[tracing::instrument(skip(state, client), fields(api_key_id = %client.api_key_id, session_id = %session_id))]
pub async fn list_messages(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ChatAppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let messages = state
        .chat
        .messages
        .list_for_session(client, session_id)
        .await
        .map_err(ApiError::internal)?;

    info!(
        message_count = messages.len(),
        "chat session messages listed"
    );

    Ok(response::success(StatusCode::OK, messages).into_response())
}
