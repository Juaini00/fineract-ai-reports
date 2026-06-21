use axum::{
    Router,
    routing::{get, post},
};

use crate::api::{
    ChatAppState,
    handlers::chat::{create_job, create_session, get_job, get_session, list_messages},
};

pub fn router() -> Router<ChatAppState> {
    Router::new()
        .route("/chat/sessions", post(create_session))
        .route("/chat/sessions/{session_id}", get(get_session))
        .route("/chat/sessions/{session_id}/messages", get(list_messages))
        .route("/chat/jobs", post(create_job))
        .route("/chat/jobs/{job_id}", get(get_job))
}
