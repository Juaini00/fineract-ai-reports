use anyhow::Result;
use app_core::auth::model::ClientContext;
use serde_json::json;
use uuid::Uuid;

use crate::chat::repository::MessageRepository;

use super::{
    ContextMessage, ContextWarning, ContextWarningCode, ContextWindow, ContextWindowPolicy,
    RelevantJobSummary, SessionMemoryRepository,
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
        let mut relevant_jobs: Vec<RelevantJobSummary> =
            serde_json::from_value(memory.relevant_jobs.clone()).unwrap_or_default();
        relevant_jobs.extend(
            self.sessions
                .recent_completed_job_summaries(session_id, self.policy.max_relevant_jobs as i64)
                .await?,
        );
        relevant_jobs.truncate(self.policy.max_relevant_jobs);
        let source_intent = memory
            .pending_clarification
            .as_ref()
            .and_then(|payload| payload.source_intent.as_ref())
            .map(serde_json::to_value)
            .transpose()?
            .or(memory.pending_clarification_source_intent.clone());
        let client_scope = json!({
            "api_key_id": client.api_key_id,
            "office_ids": client.allowed_office_ids,
            "capabilities": client.allowed_capabilities,
            "allow_all_offices": client.allow_all_offices,
            "allow_all_capabilities": client.allow_all_capabilities,
            "can_view_pii": client.can_view_pii,
            "active_domain": memory.active_domain.clone(),
            "selected_entities": memory.entities.clone(),
        });
        let mut warnings = Vec::new();
        let mut estimated_tokens = estimate_tokens(memory.summary.as_deref().unwrap_or_default())
            + estimate_tokens(&memory.entities.to_string())
            + estimate_tokens(&memory.relevant_jobs.to_string())
            + estimate_tokens(
                &memory
                    .pending_clarification
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?
                    .unwrap_or_default(),
            )
            + estimate_tokens(
                &source_intent
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            )
            + estimate_tokens(&client_scope.to_string())
            + memory
                .active_domain
                .as_deref()
                .map(estimate_tokens)
                .unwrap_or_default()
            + messages.iter().map(message_tokens).sum::<usize>()
            + relevant_jobs.iter().map(job_tokens).sum::<usize>();
        if estimated_tokens > self.policy.hard_token_limit {
            warnings.push(ContextWarning {
                code: ContextWarningCode::SessionContextExceeded,
                message: "Session context exceeds the hard routing limit.".into(),
            });
            while estimated_tokens > self.policy.soft_token_limit && !messages.is_empty() {
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
            relevant_jobs,
            pending_clarification: memory.pending_clarification,
            source_intent,
            source_snippets: Vec::new(),
            client_scope,
            warnings,
        })
    }
}

fn job_tokens(job: &RelevantJobSummary) -> usize {
    estimate_tokens(&job.job_id)
        + estimate_tokens(job.domain.as_deref().unwrap_or_default())
        + estimate_tokens(job.intent.as_deref().unwrap_or_default())
        + estimate_tokens(&job.summary)
        + estimate_tokens(&job.retrieval_plan.to_string())
        + estimate_tokens(&job.evidence_decision.to_string())
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
