use anyhow::Result;
use app_core::auth::model::ClientContext;
use serde_json::json;
use uuid::Uuid;

use crate::chat::repository::MessageRepository;

use super::{
    ContextMessage, ContextWarning, ContextWarningCode, ContextWindow, ContextWindowPolicy,
    SessionMemoryRepository,
};

#[derive(Clone)]
pub struct ContextBuilder {
    messages: MessageRepository,
    sessions: SessionMemoryRepository,
    policy: ContextWindowPolicy,
}

impl ContextBuilder {
    pub fn new(
        messages: MessageRepository,
        sessions: SessionMemoryRepository,
        policy: ContextWindowPolicy,
    ) -> Self {
        Self {
            messages,
            sessions,
            policy,
        }
    }

    pub async fn build(&self, session_id: Uuid, client: &ClientContext) -> Result<ContextWindow> {
        let memory = self.sessions.get_or_create(session_id).await?;
        let recent = self
            .messages
            .list_recent_for_session(session_id, self.policy.max_recent_messages as i64)
            .await?;
        let mut messages: Vec<ContextMessage> = recent
            .into_iter()
            .map(|message| ContextMessage {
                role: message.role,
                content: message.content,
                created_at: Some(message.created_at.to_rfc3339()),
            })
            .collect();
        let mut warnings = Vec::new();
        let mut estimated_tokens = estimate_tokens(memory.summary.as_deref().unwrap_or_default())
            + estimate_tokens(&memory.entities.to_string())
            + memory
                .active_domain
                .as_deref()
                .map(estimate_tokens)
                .unwrap_or_default()
            + messages.iter().map(message_tokens).sum::<usize>();
        if estimated_tokens > self.policy.hard_token_limit {
            warnings.push(ContextWarning {
                code: ContextWarningCode::SessionContextExceeded,
                message: "Session context exceeds the hard routing limit.".into(),
            });
            while estimated_tokens > self.policy.hard_token_limit && !messages.is_empty() {
                let removed = messages.remove(0);
                estimated_tokens = estimated_tokens.saturating_sub(message_tokens(&removed));
            }
        } else if estimated_tokens > self.policy.soft_token_limit {
            warnings.push(ContextWarning {
                code: ContextWarningCode::SessionContextNearLimit,
                message: "Session context is near the routing limit.".into(),
            });
        }

        Ok(ContextWindow {
            summary: memory.summary,
            active_domain: memory.active_domain.clone(),
            selected_entities: memory.entities.clone(),
            recent_messages: messages,
            relevant_jobs: Vec::new(),
            pending_clarification: memory.pending_clarification,
            client_scope: json!({
                "api_key_id": client.api_key_id,
                "office_ids": client.allowed_office_ids,
                "capabilities": client.allowed_capabilities,
                "allow_all_offices": client.allow_all_offices,
                "allow_all_capabilities": client.allow_all_capabilities,
                "can_view_pii": client.can_view_pii,
                "active_domain": memory.active_domain,
                "selected_entities": memory.entities,
            }),
            warnings,
        })
    }
}

fn estimate_tokens(text: &str) -> usize {
    text.split_whitespace().count()
}

fn message_tokens(message: &ContextMessage) -> usize {
    estimate_tokens(&message.role)
        + estimate_tokens(&message.content)
        + message
            .created_at
            .as_deref()
            .map(estimate_tokens)
            .unwrap_or_default()
}
