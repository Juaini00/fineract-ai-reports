use app_core::api::AppState;

use crate::repository::ChatRepository;
use crate::service::ChatService;

pub mod auth;
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
