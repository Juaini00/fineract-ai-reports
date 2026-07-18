use anyhow::Result;
use app_core::auth::model::PrincipalContext;
use uuid::Uuid;

use crate::conversation::model::ChatMessage;
use crate::conversation::repository::MessageRepository;

#[derive(Clone)]
pub struct MessageService {
    messages: MessageRepository,
}

impl MessageService {
    pub fn new(messages: MessageRepository) -> Self {
        Self { messages }
    }

    #[tracing::instrument(skip(self, client), fields(user_id = %client.user_id, session_id = %session_id))]
    pub async fn list_for_session(
        &self,
        client: PrincipalContext,
        session_id: Uuid,
    ) -> Result<Option<Vec<ChatMessage>>> {
        self.messages
            .list_for_user(session_id, client.user_id, client.role == "admin")
            .await
    }
}
