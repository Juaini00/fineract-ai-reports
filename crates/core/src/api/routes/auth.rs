use axum::{
    Router,
    routing::{get, post},
};

use crate::api::{
    AppState,
    handlers::auth::{create_api_key, get_user_me, login, logout, refresh},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(get_user_me))
        .route("/auth/api-keys", post(create_api_key))
}
