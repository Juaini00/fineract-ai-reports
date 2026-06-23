use axum::{
    Router,
    routing::{get, post},
};

use crate::api::ChatAppState;
use crate::api::handlers::session;

pub fn router() -> Router<ChatAppState> {
    Router::new()
        .route("/chat/sessions", post(session::create))
        .route("/chat/sessions/{session_id}", get(session::get))
        .route(
            "/chat/sessions/{session_id}/messages",
            get(session::list_messages),
        )
}
