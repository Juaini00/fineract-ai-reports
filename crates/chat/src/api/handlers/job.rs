use app_core::api::{
    error::ApiError,
    extractors::{authenticated_client::AuthenticatedClient, validated_json::ValidatedJson},
    response,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use futures::stream;
use std::convert::Infallible;
use tracing::info;
use uuid::Uuid;

use crate::api::ChatAppState;
use crate::api::dto::job::{CreateChatJobRequest, RespondToChatJobRequest};
use crate::chat::model::{CreateChatJobInput, RespondToChatJobInput};

#[tracing::instrument(skip(state, client, request), fields(api_key_id = %client.api_key_id))]
pub async fn create(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ChatAppState>,
    ValidatedJson(request): ValidatedJson<CreateChatJobRequest>,
) -> Result<Response, ApiError> {
    let job = state
        .chat
        .jobs
        .create(CreateChatJobInput {
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
pub async fn get(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ChatAppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let Some(job) = state
        .chat
        .jobs
        .get(client, job_id)
        .await
        .map_err(ApiError::internal)?
    else {
        return Err(ApiError::not_found("chat job not found"));
    };

    info!(job_id = %job.id, status = %job.status, current_step = %job.current_step, "chat job fetched");

    Ok(response::success(StatusCode::OK, job).into_response())
}

#[tracing::instrument(skip(state, client), fields(api_key_id = %client.api_key_id, job_id = %job_id))]
pub async fn stream(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ChatAppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let Some(job) = state
        .chat
        .jobs
        .get(client, job_id)
        .await
        .map_err(ApiError::internal)?
    else {
        return Err(ApiError::not_found("chat job not found"));
    };

    let payload = serde_json::json!({
        "job_id": job.id,
        "status": job.status,
        "current_step": job.current_step,
    })
    .to_string();

    let events =
        stream::once(
            async move { Ok::<_, Infallible>(Event::default().event("status").data(payload)) },
        );

    Ok(Sse::new(events).into_response())
}

#[tracing::instrument(skip(state, client, request), fields(api_key_id = %client.api_key_id, job_id = %job_id))]
pub async fn respond(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ChatAppState>,
    Path(job_id): Path<Uuid>,
    ValidatedJson(request): ValidatedJson<RespondToChatJobRequest>,
) -> Result<Response, ApiError> {
    let Some(message) = state
        .chat
        .jobs
        .respond(RespondToChatJobInput {
            client,
            job_id,
            message: request.message,
        })
        .await
        .map_err(ApiError::internal)?
    else {
        return Err(ApiError::not_found("chat job not found"));
    };

    info!(job_id = %job_id, message_id = %message.id, "chat job response received");

    Ok(response::success(StatusCode::CREATED, message).into_response())
}
