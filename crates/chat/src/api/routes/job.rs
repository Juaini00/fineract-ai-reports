use axum::{
    Router,
    routing::{get, post},
};

use crate::api::ChatAppState;
use crate::api::handlers::job;

pub fn router() -> Router<ChatAppState> {
    Router::new()
        .route("/chat/jobs", post(job::create))
        .route("/chat/jobs/{job_id}", get(job::get))
        .route("/chat/jobs/{job_id}/stream", get(job::stream))
        .route("/chat/jobs/{job_id}/responses", post(job::respond))
}
