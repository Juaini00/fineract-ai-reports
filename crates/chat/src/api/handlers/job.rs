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
use std::{convert::Infallible, time::Duration};
use tracing::{info, warn};
use uuid::Uuid;

use crate::api::ChatAppState;
use crate::api::dto::job::{CreateChatJobRequest, RespondToChatJobRequest};
use crate::job::model::{ChatJobEvent, CreateChatJobInput, RespondToChatJobInput};
use crate::job::service::{RespondToChatJobOutcome, redis_url_log_value};

fn map_create_error(error: anyhow::Error) -> ApiError {
    match error.to_string().as_str() {
        "identifier_lookup_rate_limited" => ApiError::too_many_requests_with_code(
            "identifier_lookup_rate_limited",
            "Too many identifier lookup attempts. Try again later.",
        ),
        "identifier_lookup_rate_limit_unavailable" => ApiError::internal(anyhow::anyhow!(
            "identifier lookup is temporarily unavailable"
        )),
        _ => ApiError::internal(error),
    }
}

/// Statuses `chat_jobs.status` reaches at the end of a run — mirrors the
/// `chat_job:{id}:live_state` values `JobService::emit_event` writes on
/// `final`/`error`.
fn is_terminal_job_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "waiting_for_user_input")
}

fn durable_poll_stream(
    state: ChatAppState,
    client: app_core::auth::model::PrincipalContext,
    job_id: Uuid,
) -> impl futures::Stream<Item = Result<Event, Infallible>> {
    stream::unfold(Some(0usize), move |state_seen| {
        let state = state.clone();
        let client = client.clone();
        async move {
            let seen = state_seen?;
            tokio::time::sleep(Duration::from_millis(250)).await;
            let events = state
                .chat
                .jobs
                .replay_events(client, job_id)
                .await
                .unwrap_or_default()
                .unwrap_or_default();
            let next = events.into_iter().nth(seen);
            let terminal = next.as_ref().is_some_and(|event| {
                matches!(
                    event.event_type.as_str(),
                    "final" | "error" | "clarification"
                )
            });
            let event = next
                .as_ref()
                .map(replay_event_to_sse)
                .unwrap_or_else(|| Event::default().event("keepalive").data("{}"));
            Some((
                Ok::<_, Infallible>(event),
                (!terminal).then_some(seen + usize::from(next.is_some())),
            ))
        }
    })
}

/// Re-derives the SSE wire shape `JobService::emit_event` publishes live
/// (`{kind, step, payload, at}`), so a replayed durable event is
/// indistinguishable from one the client would have received live.
fn replay_event_to_sse(event: &ChatJobEvent) -> Event {
    let body = serde_json::json!({
        "kind": event.event_type,
        "step": event.step,
        "payload": event.payload_json,
        "at": event.created_at,
    })
    .to_string();
    Event::default().event(event.event_type.clone()).data(body)
}

/// Best-effort peek at whether the job finished *during* the subscribe race
/// (Task C1 part 2). Redis is live coordination only — any failure here
/// (down, timeout, key expired) must fall through to the normal live stream,
/// never fail or block the request.
async fn live_state_is_terminal(redis_client: &redis::Client, job_id: Uuid) -> bool {
    let Ok(mut conn) = redis_client.get_multiplexed_async_connection().await else {
        return false;
    };
    let key = format!("chat_job:{job_id}:live_state");
    let value: redis::RedisResult<Option<String>> = redis::AsyncCommands::get(&mut conn, key).await;
    matches!(value, Ok(Some(state)) if is_terminal_job_status(&state))
}

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
        .map_err(map_create_error)?
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
        .get(client.clone(), job_id)
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

    // Part 1 (C1): pub/sub has no history. If the job already finished
    // before this subscriber connected — page reload, restored tab, a fast
    // job, or just the gap between POST /chat/jobs and opening the stream —
    // subscribing would hang forever waiting for a `final`/`error` message
    // that already happened. Replay the durable log instead and end,
    // without ever subscribing.
    if is_terminal_job_status(&job.status) {
        let events = state
            .chat
            .jobs
            .replay_events(client, job_id)
            .await
            .map_err(ApiError::internal)?
            .unwrap_or_default();
        let replay_events: Vec<_> = events.iter().map(replay_event_to_sse).collect();
        let replay = stream::iter(replay_events.into_iter().map(Ok::<_, Infallible>));
        return Ok(Sse::new(status_event().chain(replay)).into_response());
    }

    let Some(redis_client) = state.core.pools.redis.clone() else {
        return Ok(
            Sse::new(status_event().chain(durable_poll_stream(state, client, job_id)))
                .keep_alive(KeepAlive::default())
                .into_response(),
        );
    };

    let redis_url = redis_url_log_value(&state.core.config.redis.url);
    let channel = format!("chat_job:{job_id}:events");
    let mut pubsub = match redis_client.get_async_pubsub().await {
        Ok(pubsub) => pubsub,
        Err(error) => {
            warn!(redis_url = %redis_url, error = %error, "redis pubsub connect failed during SSE, using durable polling");
            return Ok(
                Sse::new(status_event().chain(durable_poll_stream(state, client, job_id)))
                    .keep_alive(KeepAlive::default())
                    .into_response(),
            );
        }
    };
    if let Err(error) = pubsub.subscribe(&channel).await {
        warn!(redis_url = %redis_url, error = %error, channel = %channel, "redis subscribe failed during SSE, using durable polling");
        return Ok(
            Sse::new(status_event().chain(durable_poll_stream(state, client, job_id)))
                .keep_alive(KeepAlive::default())
                .into_response(),
        );
    }

    // Part 2 (C1): the job may have finished between the status snapshot
    // above and this subscribe call — events published during that race are
    // lost to pub/sub (no history), so a late `final`/`error` could be the
    // only thing this subscription ever sees, or nothing at all. Re-check
    // and, if the job already finished, replay the durable log instead of
    // trusting the channel.
    if live_state_is_terminal(&redis_client, job_id).await {
        let events = state
            .chat
            .jobs
            .replay_events(client, job_id)
            .await
            .map_err(ApiError::internal)?
            .unwrap_or_default();
        let replay_events: Vec<_> = events.iter().map(replay_event_to_sse).collect();
        let replay = stream::iter(replay_events.into_iter().map(Ok::<_, Infallible>));
        return Ok(Sse::new(status_event().chain(replay)).into_response());
    }

    // Part 3: subscribe to the job's pub/sub channel and forward each event
    // under its own `kind` as the SSE event name, terminating on
    // `final`/`error`.
    let message_stream = Box::pin(pubsub.into_on_message());
    let events = status_event().chain(stream::unfold(Some(message_stream), |state| async move {
        let mut message_stream = state?;
        let message = message_stream.next().await?;
        let payload: String = message.get_payload().unwrap_or_default();
        let kind = serde_json::from_str::<serde_json::Value>(&payload)
            .ok()
            .and_then(|value| value.get("kind")?.as_str().map(str::to_string))
            .unwrap_or_else(|| "update".to_string());
        let is_terminal = matches!(kind.as_str(), "final" | "error" | "clarification");
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
