use axum::{Router, routing::get};

use crate::api::{
    AppState,
    handlers::health::{health, ready},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
}
