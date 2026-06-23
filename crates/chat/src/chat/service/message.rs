use anyhow::Result;
use app_core::auth::model::ClientContext;
use uuid::Uuid;

use crate::chat::model::ChatMessage;
use crate::chat::repository::MessageRepository;

#[derive(Clone)]
pub struct MessageService {
    messages: MessageRepository,
}

impl MessageService {
    pub fn new(messages: MessageRepository) -> Self {
        Self { messages }
    }

    #[tracing::instrument(skip(self, client), fields(api_key_id = %client.api_key_id, session_id = %session_id))]
    pub async fn list_for_session(
        &self,
        client: ClientContext,
        session_id: Uuid,
    ) -> Result<Vec<ChatMessage>> {
        self.messages
            .list_for_client(session_id, client.api_key_id)
            .await
    }
}
