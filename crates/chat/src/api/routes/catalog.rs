use axum::{Router, routing::post};

use crate::api::ChatAppState;
use crate::api::handlers::catalog;

pub fn router() -> Router<ChatAppState> {
    Router::new().route("/catalog/validate", post(catalog::validate))
}
