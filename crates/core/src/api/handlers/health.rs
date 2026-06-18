use axum::{Json, extract::State, http::StatusCode};

use crate::api::{
    AppState,
    dto::health::{HealthResponse, ReadyResponse},
};

pub(crate) async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

pub(crate) async fn ready(State(state): State<AppState>) -> (StatusCode, Json<ReadyResponse>) {
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
