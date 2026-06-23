use anyhow::Result;
use app_core::auth::model::ClientContext;
use uuid::Uuid;

use crate::chat::model::{ChatSession, CreateChatSessionInput};
use crate::chat::repository::SessionRepository;

#[derive(Clone)]
pub struct SessionService {
    sessions: SessionRepository,
}

impl SessionService {
    pub fn new(sessions: SessionRepository) -> Self {
        Self { sessions }
    }

    #[tracing::instrument(skip(self, input), fields(api_key_id = %input.client.api_key_id))]
    pub async fn create(&self, input: CreateChatSessionInput) -> Result<ChatSession> {
        let title = input
            .title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());

        self.sessions.create(input.client.api_key_id, title).await
    }

    #[tracing::instrument(skip(self, client), fields(api_key_id = %client.api_key_id, session_id = %session_id))]
    pub async fn get(
        &self,
        client: ClientContext,
        session_id: Uuid,
    ) -> Result<Option<ChatSession>> {
        self.sessions
            .get_for_client(session_id, client.api_key_id)
            .await
    }
}
