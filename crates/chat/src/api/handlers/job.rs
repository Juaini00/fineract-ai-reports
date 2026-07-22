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
        sse::{Event, Sse},
    },
};
use futures::stream::{self, StreamExt};
use std::{convert::Infallible, time::Duration};
use tracing::{info, warn};
use uuid::Uuid;

use crate::api::ChatAppState;
use crate::api::dto::job::{CreateChatJobRequest, RespondToChatJobRequest};
use crate::job::model::{CreateChatJobInput, RespondToChatJobInput};
use crate::job::service::redis_url_log_value;

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

    let Some(redis_client) = state.core.pools.redis.clone() else {
        let events = stream::once(async move {
            Ok::<_, Infallible>(Event::default().event("status").data(snapshot))
        });
        return Ok(Sse::new(events).into_response());
    };

    // Poll Redis :latest_event every 1s, emit on change, terminate on :live_state ∈ {completed, failed}.
    // ponytail: polling; upgrade to PubSub if per-job event latency hurts UX.
    let event_key = format!("chat_job:{job_id}:latest_event");
    let state_key = format!("chat_job:{job_id}:live_state");
    let redis_url = redis_url_log_value(&state.core.config.redis.url);
    let stream = stream::unfold(
        (
            redis_client,
            event_key,
            state_key,
            redis_url,
            Some(snapshot),
            0u32,
            true,
        ),
        |(client, event_key, state_key, redis_url, snapshot, ticks, mut first)| async move {
            if let Some(initial) = snapshot {
                return Some((
                    Ok::<_, Infallible>(Event::default().event("status").data(initial)),
                    (client, event_key, state_key, redis_url, None, ticks, false),
                ));
            }
            if !first {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            first = false;
            if ticks >= 120 {
                return None;
            }
            let mut conn = match client.get_multiplexed_async_connection().await {
                Ok(conn) => conn,
                Err(error) => {
                    warn!(redis_url = %redis_url, error = %error, "redis connect failed during SSE");
                    return None;
                }
            };
            let event: Option<String> = redis::AsyncCommands::get(&mut conn, &event_key)
                .await
                .unwrap_or(None);
            let live_state: Option<String> = redis::AsyncCommands::get(&mut conn, &state_key)
                .await
                .unwrap_or(None);

            let event = event.unwrap_or_else(|| "{}".to_string());
            let sse = Event::default().event("update").data(event);

            let terminal = matches!(live_state.as_deref(), Some("completed") | Some("failed"));
            let next_ticks = if terminal { 121 } else { ticks + 1 };
            Some((
                Ok(sse),
                (client, event_key, state_key, redis_url, None, next_ticks, first),
            ))
        },
    )
    .take(125);

    Ok(Sse::new(stream).into_response())
}

#[tracing::instrument(skip(state, client, request), fields(user_id = %client.user_id, job_id = %job_id))]
pub async fn respond(
    AuthenticatedChatClient(client): AuthenticatedChatClient,
    State(state): State<ChatAppState>,
    Path(job_id): Path<Uuid>,
    ValidatedJson(request): ValidatedJson<RespondToChatJobRequest>,
) -> Result<Response, ApiError> {
    let selected_option_id = request
        .option_id
        .map(|option_id| option_id.trim().to_owned())
        .filter(|option_id| !option_id.is_empty());
    let source_message = request.message;
    let message = source_message.clone();

    let Some(message) = state
        .chat
        .jobs
        .respond(RespondToChatJobInput {
            client,
            job_id,
            source_message,
            selected_option_id,
            message,
        })
        .await
        .map_err(ApiError::internal)?
    else {
        return Err(ApiError::not_found("chat job not found"));
    };

    info!(job_id = %job_id, message_id = %message.id, "chat job response received");

    Ok(response::success(StatusCode::CREATED, message).into_response())
}
