pub mod error;
pub mod extractors;
pub mod response;
pub mod routes;

use axum::Router;
use tower_http::trace::TraceLayer;

use crate::{
    auth::{repository::ApiKeyRepository, service::AuthService},
    config::AppConfig,
    db::DatabasePools,
};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub pools: DatabasePools,
    pub auth_service: AuthService,
}

impl AppState {
    pub fn new(config: AppConfig, pools: DatabasePools) -> Self {
        let api_key_repository = ApiKeyRepository::new(pools.app.clone());
        let auth_service = AuthService::new(config.auth.clone(), api_key_repository);

        Self {
            config,
            pools,
            auth_service,
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(routes::health::router())
        .merge(routes::auth::router())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
