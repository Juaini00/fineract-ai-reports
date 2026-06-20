use app_core::api::{error::ApiError, extractors::validated_json::ValidatedJson, response};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::info;
use uuid::Uuid;

use crate::{
    api::{
        ChatAppState,
        auth::AuthenticatedChatClient,
        dto::{CreateChatJobRequest, CreateChatSessionRequest},
    },
    model::{CreateChatJobInput, CreateChatSessionInput},
};

#[tracing::instrument(skip(state, client, request), fields(api_key_id = %client.api_key_id))]
pub async fn create_session(
    AuthenticatedChatClient(client): AuthenticatedChatClient,
    State(state): State<ChatAppState>,
    ValidatedJson(request): ValidatedJson<CreateChatSessionRequest>,
) -> Result<Response, ApiError> {
    let session = state
        .chat_service
        .create_session(CreateChatSessionInput {
            client,
            title: request.title,
        })
        .await
        .map_err(ApiError::internal)?;

    info!(session_id = %session.id, "chat session created");

    Ok(response::success(StatusCode::CREATED, session).into_response())
}

#[tracing::instrument(skip(state, client), fields(api_key_id = %client.api_key_id, session_id = %session_id))]
pub async fn get_session(
    AuthenticatedChatClient(client): AuthenticatedChatClient,
    State(state): State<ChatAppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let Some(session) = state
        .chat_service
        .get_session(client, session_id)
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
    AuthenticatedChatClient(client): AuthenticatedChatClient,
    State(state): State<ChatAppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let messages = state
        .chat_service
        .list_messages(client, session_id)
        .await
        .map_err(ApiError::internal)?;

    info!(
        message_count = messages.len(),
        "chat session messages listed"
    );

    Ok(response::success(StatusCode::OK, messages).into_response())
}

#[tracing::instrument(skip(state, client, request), fields(api_key_id = %client.api_key_id))]
pub async fn create_job(
    AuthenticatedChatClient(client): AuthenticatedChatClient,
    State(state): State<ChatAppState>,
    ValidatedJson(request): ValidatedJson<CreateChatJobRequest>,
) -> Result<Response, ApiError> {
    let job = state
        .chat_service
        .create_job(CreateChatJobInput {
            client,
            session_id: request.session_id,
            message: request.message,
        })
        .await
        .map_err(ApiError::internal)?;

    info!(
        session_id = %job.session_id,
        job_id = %job.job_id,
        user_message_id = %job.user_message_id,
        "chat job created"
    );

    Ok(response::success(StatusCode::CREATED, job).into_response())
}

#[tracing::instrument(skip(state, client), fields(api_key_id = %client.api_key_id, job_id = %job_id))]
pub async fn get_job(
    AuthenticatedChatClient(client): AuthenticatedChatClient,
    State(state): State<ChatAppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let Some(job) = state
        .chat_service
        .get_job(client, job_id)
        .await
        .map_err(ApiError::internal)?
    else {
        return Err(ApiError::not_found("chat job not found"));
    };

    info!(job_id = %job.id, status = %job.status, current_step = %job.current_step, "chat job fetched");

    Ok(response::success(StatusCode::OK, job).into_response())
}
