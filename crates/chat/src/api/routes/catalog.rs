use axum::{
    Router,
    routing::{get, post},
};

use crate::api::ChatAppState;
use crate::api::handlers::catalog;

pub fn router() -> Router<ChatAppState> {
    Router::new()
        .route("/catalog/validate", post(catalog::validate))
        .route("/vector-index/rebuild", post(catalog::vector_index_rebuild))
        .route("/vector-index/status", get(catalog::vector_index_status))
}
