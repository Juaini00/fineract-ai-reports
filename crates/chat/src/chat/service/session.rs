use anyhow::Result;
use app_core::auth::model::PrincipalContext;
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

    #[tracing::instrument(skip(self, input), fields(user_id = %input.client.user_id))]
    pub async fn create(&self, input: CreateChatSessionInput) -> Result<ChatSession> {
        let title = input
            .title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());

        self.sessions.create(input.client.user_id, title).await
    }

    #[tracing::instrument(skip(self, client), fields(user_id = %client.user_id))]
    pub async fn list(&self, client: PrincipalContext) -> Result<Vec<ChatSession>> {
        self.sessions
            .list_for_user(client.user_id, client.role == "admin")
            .await
    }

    #[tracing::instrument(skip(self, client), fields(user_id = %client.user_id, session_id = %session_id))]
    pub async fn get(
        &self,
        client: PrincipalContext,
        session_id: Uuid,
    ) -> Result<Option<ChatSession>> {
        self.sessions
            .get_for_user(session_id, client.user_id, client.role == "admin")
            .await
    }
}
