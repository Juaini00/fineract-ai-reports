use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Serialize;

use crate::api::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[derive(Serialize)]
struct ReadyResponse {
    status: &'static str,
    checks: crate::db::ReadinessChecks,
}

async fn ready(State(state): State<AppState>) -> (StatusCode, Json<ReadyResponse>) {
    let checks = state.pools.readiness().await;
    let status = if checks.is_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let label = if status == StatusCode::OK {
        "ready"
    } else {
        "not_ready"
    };

    (
        status,
        Json(ReadyResponse {
            status: label,
            checks,
        }),
    )
}
