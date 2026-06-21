use app_core::api::AppState;
use app_core::auth::service::AuthService;
use axum::{Router, extract::FromRef};

use crate::chat::repository::ChatRepository;
use crate::chat::service::ChatService;

pub mod dto;
pub mod handlers;
pub mod routes;

#[derive(Clone)]
pub struct ChatAppState {
    pub core: AppState,
    pub chat_service: ChatService,
}

impl ChatAppState {
    pub fn new(core: AppState) -> Self {
        let chat_repository = ChatRepository::new(core.pools.app.clone());
        let chat_service = ChatService::new(chat_repository);

        Self { core, chat_service }
    }
}

impl FromRef<ChatAppState> for AuthService {
    fn from_ref(state: &ChatAppState) -> Self {
        state.core.auth_service.clone()
    }
}

pub fn router(state: ChatAppState) -> Router {
    Router::new()
        .merge(routes::chat::router())
        .with_state(state)
}
