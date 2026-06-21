use anyhow::{Result, bail};
use app_core::auth::model::ClientContext;
use serde_json::json;
use uuid::Uuid;

use crate::chat::model::{
    ChatJob, ChatMessage, ChatSession, CreateChatJobInput, CreateChatSessionInput, CreatedChatJob,
};
use crate::chat::repository::ChatRepository;

#[derive(Clone)]
pub struct ChatService {
    repository: ChatRepository,
}

impl ChatService {
    pub fn new(repository: ChatRepository) -> Self {
        Self { repository }
    }

    #[tracing::instrument(skip(self, input), fields(api_key_id = %input.client.api_key_id))]
    pub async fn create_session(&self, input: CreateChatSessionInput) -> Result<ChatSession> {
        let title = input
            .title
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty());

        self.repository
            .create_session(input.client.api_key_id, title)
            .await
    }

    #[tracing::instrument(skip(self, client), fields(api_key_id = %client.api_key_id, session_id = %session_id))]
    pub async fn get_session(
        &self,
        client: ClientContext,
        session_id: Uuid,
    ) -> Result<Option<ChatSession>> {
        self.repository
            .get_session_for_client(session_id, client.api_key_id)
            .await
    }

    #[tracing::instrument(skip(self, client), fields(api_key_id = %client.api_key_id, session_id = %session_id))]
    pub async fn list_messages(
        &self,
        client: ClientContext,
        session_id: Uuid,
    ) -> Result<Vec<ChatMessage>> {
        self.repository
            .list_messages_for_client(session_id, client.api_key_id)
            .await
    }

    #[tracing::instrument(skip(self, input), fields(api_key_id = %input.client.api_key_id))]
    pub async fn create_job(&self, input: CreateChatJobInput) -> Result<CreatedChatJob> {
        let message = input.message.trim().to_string();

        if message.is_empty() {
            bail!("message is required");
        }

        let client_context_json = json!({
            "api_key_id": input.client.api_key_id,
            "owner": input.client.owner,
            "key_prefix": input.client.key_prefix,
            "allowed_office_ids": input.client.allowed_office_ids,
            "allowed_capabilities": input.client.allowed_capabilities,
            "can_view_pii": input.client.can_view_pii
        });

        self.repository
            .create_job(
                input.client.api_key_id,
                input.session_id,
                message,
                client_context_json,
            )
            .await
    }

    #[tracing::instrument(skip(self, client), fields(api_key_id = %client.api_key_id, job_id = %job_id))]
    pub async fn get_job(&self, client: ClientContext, job_id: Uuid) -> Result<Option<ChatJob>> {
        self.repository
            .get_job_for_client(job_id, client.api_key_id)
            .await
    }
}
