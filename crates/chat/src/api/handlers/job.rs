use app_core::api::{
    error::ApiError,
    extractors::{
        authenticated_chat_client::AuthenticatedChatClient, validated_json::ValidatedJson,
    },
    response,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures::stream::{self, StreamExt};
use std::convert::Infallible;
use tracing::{info, warn};
use uuid::Uuid;

use crate::api::ChatAppState;
use crate::api::dto::job::{CreateChatJobRequest, RespondToChatJobRequest};
use crate::job::model::{CreateChatJobInput, RespondToChatJobInput};
use crate::job::service::{RespondToChatJobOutcome, redis_url_log_value};

#[tracing::instrument(skip(state, client, request), fields(user_id = %client.user_id))]
pub async fn create(
    AuthenticatedChatClient(client): AuthenticatedChatClient,
    State(state): State<ChatAppState>,
    ValidatedJson(request): ValidatedJson<CreateChatJobRequest>,
) -> Result<Response, ApiError> {
    let Some(job) = state
        .chat
        .jobs
        .create(CreateChatJobInput {
            client,
            session_id: request.session_id,
            message: request.message,
        })
        .await
        .map_err(ApiError::internal)?
    else {
        return Err(ApiError::not_found("chat session not found"));
    };

    info!(
        session_id = %job.session_id,
        job_id = %job.job_id,
        user_message_id = %job.user_message_id,
        "chat job created"
    );

    Ok(response::success(StatusCode::CREATED, job).into_response())
}

#[tracing::instrument(skip(state, client), fields(user_id = %client.user_id, job_id = %job_id))]
pub async fn get(
    AuthenticatedChatClient(client): AuthenticatedChatClient,
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

#[tracing::instrument(skip(state, client), fields(user_id = %client.user_id, job_id = %job_id))]
pub async fn audit(
    AuthenticatedChatClient(client): AuthenticatedChatClient,
    State(state): State<ChatAppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let Some(audit) = state
        .chat
        .jobs
        .audit(client, job_id)
        .await
        .map_err(ApiError::internal)?
    else {
        return Err(ApiError::not_found("chat job not found"));
    };

    Ok(response::success(StatusCode::OK, audit).into_response())
}

#[tracing::instrument(skip(state, client), fields(user_id = %client.user_id, job_id = %job_id))]
pub async fn stream(
    AuthenticatedChatClient(client): AuthenticatedChatClient,
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

    let snapshot = serde_json::json!({
        "job_id": job.id,
        "status": job.status,
        "current_step": job.current_step,
    })
    .to_string();

    let status_event = || {
        stream::once(
            async move { Ok::<_, Infallible>(Event::default().event("status").data(snapshot)) },
        )
    };

    let Some(redis_client) = state.core.pools.redis.clone() else {
        return Ok(Sse::new(status_event())
            .keep_alive(KeepAlive::default())
            .into_response());
    };

    let redis_url = redis_url_log_value(&state.core.config.redis.url);
    let channel = format!("chat_job:{job_id}:events");
    let mut pubsub = match redis_client.get_async_pubsub().await {
        Ok(pubsub) => pubsub,
        Err(error) => {
            warn!(redis_url = %redis_url, error = %error, "redis pubsub connect failed during SSE, falling back to snapshot");
            return Ok(Sse::new(status_event())
                .keep_alive(KeepAlive::default())
                .into_response());
        }
    };
    if let Err(error) = pubsub.subscribe(&channel).await {
        warn!(redis_url = %redis_url, error = %error, channel = %channel, "redis subscribe failed during SSE, falling back to snapshot");
        return Ok(Sse::new(status_event())
            .keep_alive(KeepAlive::default())
            .into_response());
    }

    // Subscribe to the job's pub/sub channel (Task 4) and forward each event under
    // its own `kind` as the SSE event name, terminating on `final`/`error`.
    let message_stream = Box::pin(pubsub.into_on_message());
    let events = status_event().chain(stream::unfold(Some(message_stream), |state| async move {
        let mut message_stream = state?;
        let message = message_stream.next().await?;
        let payload: String = message.get_payload().unwrap_or_default();
        let kind = serde_json::from_str::<serde_json::Value>(&payload)
            .ok()
            .and_then(|value| value.get("kind")?.as_str().map(str::to_string))
            .unwrap_or_else(|| "update".to_string());
        let is_terminal = matches!(kind.as_str(), "final" | "error");
        let event = Event::default().event(kind).data(payload);
        let next_state = if is_terminal {
            None
        } else {
            Some(message_stream)
        };
        Some((Ok::<_, Infallible>(event), next_state))
    }));

    Ok(Sse::new(events)
        .keep_alive(KeepAlive::default())
        .into_response())
}

#[tracing::instrument(skip(state, client, request), fields(user_id = %client.user_id, job_id = %job_id))]
pub async fn respond(
    AuthenticatedChatClient(client): AuthenticatedChatClient,
    State(state): State<ChatAppState>,
    Path(job_id): Path<Uuid>,
    ValidatedJson(request): ValidatedJson<RespondToChatJobRequest>,
) -> Result<Response, ApiError> {
    let outcome = state
        .chat
        .jobs
        .respond(RespondToChatJobInput {
            client,
            job_id,
            clarification_id: request.clarification_id,
            clarification_revision: request.clarification_revision,
            selected_option_id: request.option_id,
            source_message: request.message,
            answers: request.answers,
        })
        .await
        .map_err(ApiError::internal)?;
    let message = match outcome {
        RespondToChatJobOutcome::Inserted(message) => message,
        RespondToChatJobOutcome::NotFound => return Err(ApiError::not_found("chat job not found")),
        RespondToChatJobOutcome::NotActive => {
            return Err(ApiError::conflict_with_code(
                "clarification_not_active",
                "Clarification is no longer active.",
            ));
        }
        RespondToChatJobOutcome::Stale => {
            return Err(ApiError::conflict_with_code(
                "clarification_stale",
                "Clarification has changed. Refresh and try again.",
            ));
        }
        RespondToChatJobOutcome::Validation(fields) => {
            return Err(ApiError::bad_request_with_code(
                "clarification_validation_error",
                "Clarification response is invalid.",
                Some(serde_json::json!({ "fields": fields })),
            ));
        }
    };

    info!(job_id = %job_id, message_id = %message.id, "chat job response received");

    Ok(response::success(StatusCode::CREATED, message).into_response())
}
