use axum::{
    Router,
    routing::{get, post},
};

use crate::api::{
    AppState,
    handlers::auth::{create_api_key, get_me},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/api-keys", post(create_api_key))
        .route("/auth/me", get(get_me))
}
