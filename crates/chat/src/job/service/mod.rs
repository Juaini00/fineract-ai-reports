use std::{
    collections::BTreeMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use anyhow::Result;
use app_core::auth::model::PrincipalContext;
use app_core::config::{CanonicalGatewayMode, ChatFeatureConfig, EmbeddingConfig, LlmConfig};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use tracing::warn;
use uuid::Uuid;

use crate::assistant::execution::runtime::CanonicalRuntimeContext;
use crate::assistant::llm::planner_client::LlmPlannerClient;
use crate::assistant::{
    AssistantGraphRuntime, AssistantGraphTopology, CanonicalStateRepository, ContextBuilder,
    ContextWindowPolicy, DeterministicExtraction, EffectiveConstraints, ExtractionProvenance,
    FactSourceKind, JobMemory, MarkdownRenderer, OriginalIntent, PlannerInputSnapshot,
    PrincipalProjection, ResponseRenderer, RuntimeUserInput, SemanticRouter, TerminalState,
    deterministic_observations, executable_constraint_contracts,
    llm::{
        SharedLlmClient,
        rig_client::RigLlmClient,
        traced_client::{LlmTraceContext, TracedLlmClient},
    },
    merge_observations, original_request_observations, stable_uuid,
};
use crate::audit::{AuditEvent, AuditHandle, llm_trace_repository::LlmTraceRepository};
use crate::conversation::model::ChatMessage;
use crate::conversation::repository::{
    MessageRepository, assistant_memory::SessionMemoryRepository,
};
use crate::job::model::{
    ChatJob, ChatJobAuditTimeline, CreateChatJobInput, CreatedChatJob, RespondToChatJobInput,
};
use crate::job::repository::{
    JobRepository, PersistResponseOutcome, assistant_memory::JobMemoryRepository,
};
use crate::knowledge::embedding::VoyageEmbeddingClient;
use crate::knowledge::index::repository::KnowledgeRepository;
use crate::knowledge::model::KnowledgeCatalog;
use crate::policy::authorization::project_admin_principal;
use clarification_response::validate_submission;

pub mod clarification_response;
mod events;
mod run;
mod shadow;
mod test_llm;

use run::CanonicalTurn;
use test_llm::TestLlmClient;

#[derive(Clone)]
pub struct JobService {
    jobs: JobRepository,
    messages: MessageRepository,
    job_memory: JobMemoryRepository,
    canonical_state: CanonicalStateRepository,
    canonical_mode: CanonicalGatewayMode,
    session_memory: SessionMemoryRepository,
    context_builder: ContextBuilder,
    knowledge: KnowledgeRepository,
    runtime_knowledge_enabled: bool,
    fineract_pool: PgPool,
    catalog: Arc<KnowledgeCatalog>,
    llm: Option<SharedLlmClient>,
    llm_traces: LlmTraceRepository,
    audit: AuditHandle,
    redis_url: String,
    redis: Option<redis::Client>,
}

impl JobService {
    pub fn new(
        jobs: JobRepository,
        messages: MessageRepository,
        app_pool: PgPool,
        fineract_pool: PgPool,
        catalog: Arc<KnowledgeCatalog>,
        _embedding_client: VoyageEmbeddingClient,
        _llm_planner: LlmPlannerClient,
        llm_config: LlmConfig,
        embedding_config: EmbeddingConfig,
        chat_features: ChatFeatureConfig,
        redis_url: String,
        redis: Option<redis::Client>,
        audit: AuditHandle,
    ) -> Self {
        let test_llm_enabled =
            llm_config.provider == "test" && llm_config.api_key == "__ai_report_test_llm__";
        let llm = if test_llm_enabled {
            Some(Arc::new(TestLlmClient) as SharedLlmClient)
        } else if llm_config.api_key.trim().is_empty() {
            None
        } else {
            RigLlmClient::new(&llm_config, Some(&embedding_config))
                .map(|client| Some(Arc::new(client) as SharedLlmClient))
                .unwrap_or_else(|error| {
                    warn!(%error, "semantic router LLM disabled");
                    None
                })
        };
        Self {
            jobs,
            messages: messages.clone(),
            job_memory: JobMemoryRepository::new(app_pool.clone()),
            canonical_state: CanonicalStateRepository::new(app_pool.clone()),
            canonical_mode: chat_features.canonical_gateway_mode,
            session_memory: SessionMemoryRepository::new(app_pool.clone()),
            context_builder: ContextBuilder::new(
                messages.clone(),
                SessionMemoryRepository::new(app_pool.clone()),
                ContextWindowPolicy::new(
                    chat_features.context_soft_token_limit,
                    chat_features.context_hard_token_limit,
                    chat_features.context_max_recent_messages,
                    chat_features.context_max_relevant_jobs,
                ),
            ),
            knowledge: KnowledgeRepository::new(app_pool.clone()),
            runtime_knowledge_enabled: llm_config.provider != "test",
            fineract_pool,
            catalog,
            llm,
            llm_traces: LlmTraceRepository::new(app_pool),
            audit,
            redis_url,
            redis,
        }
    }

    #[tracing::instrument(skip(self, input), fields(user_id = %input.client.user_id))]
    pub async fn create(&self, input: CreateChatJobInput) -> Result<Option<CreatedChatJob>> {
        let mut client = input.client;
        project_admin_principal(&mut client, &self.catalog, &self.fineract_pool).await?;
        let Some(mut created) = self
            .jobs
            .create(
                client.user_id,
                input.session_id,
                input.message.clone(),
                serde_json::to_value(&client)?,
                json!({ "runtime": "semantic_assistant_graph" }),
                json!({}),
                json!({}),
            )
            .await?
        else {
            return Ok(None);
        };
        self.session_memory
            .get_or_create(created.session_id, client.user_id)
            .await?;
        let job_created_at = self
            .jobs
            .get_internal_for_user(created.job_id, client.user_id)
            .await?
            .expect("newly created job exists")
            .created_at;
        let mut audit_event = AuditEvent::new(
            client.user_id,
            created.job_id,
            "request_received",
            "service",
            "completed",
        );
        audit_event.session_id = Some(created.session_id);
        audit_event.legacy_api_key_id = client.legacy_api_key_id;
        self.audit.record(audit_event);
        let outcome = self
            .run_graph_skeleton(
                created.session_id,
                created.job_id,
                &client,
                input.message.as_str().into(),
                CanonicalTurn {
                    message_id: created.user_message_id,
                    observed_at: job_created_at,
                    reference_instant: job_created_at,
                    initial: true,
                },
            )
            .await?;
        if let Some(outcome) = outcome {
            created.status = outcome.status.into();
            created.current_step = outcome.current_step.into();
        }
        Ok(Some(created))
    }

    #[tracing::instrument(skip(self, client), fields(user_id = %client.user_id, job_id = %job_id))]
    pub async fn get(&self, client: PrincipalContext, job_id: Uuid) -> Result<Option<ChatJob>> {
        self.jobs
            .get_for_user(job_id, client.user_id, client.role == "admin")
            .await
    }

    #[tracing::instrument(skip(self, client), fields(user_id = %client.user_id, job_id = %job_id))]
    pub async fn audit(
        &self,
        client: PrincipalContext,
        job_id: Uuid,
    ) -> Result<Option<ChatJobAuditTimeline>> {
        let Some(events) = self
            .jobs
            .list_audit_events_for_user(job_id, client.user_id, client.role == "admin")
            .await?
        else {
            return Ok(None);
        };

        Ok(Some(ChatJobAuditTimeline { job_id, events }))
    }

    #[tracing::instrument(skip(self, input), fields(user_id = %input.client.user_id, job_id = %input.job_id))]
    pub async fn respond(&self, input: RespondToChatJobInput) -> Result<RespondToChatJobOutcome> {
        let structured = input.clarification_id.is_some()
            || input.clarification_revision.is_some()
            || !input.answers.is_empty();
        if structured
            && (input.clarification_id.is_none() || input.clarification_revision.is_none())
        {
            let field = if input.clarification_id.is_none() {
                "clarification_id"
            } else {
                "clarification_revision"
            };
            return Ok(RespondToChatJobOutcome::Validation(vec![field.to_owned()]));
        }

        let mut client = input.client;
        project_admin_principal(&mut client, &self.catalog, &self.fineract_pool).await?;
        if self
            .jobs
            .get_internal_for_user(input.job_id, client.user_id)
            .await?
            .is_none()
        {
            return Ok(RespondToChatJobOutcome::NotFound);
        }
        let payload = if structured {
            match self.job_memory.get(input.job_id, client.user_id).await? {
                Some(memory) => match memory.pending_clarification {
                    Some(payload) => payload,
                    None => return Ok(RespondToChatJobOutcome::NotActive),
                },
                None => return Ok(RespondToChatJobOutcome::NotFound),
            }
        } else {
            // Legacy clients intentionally retain the original message/option continuation behavior.
            crate::assistant::ClarificationPayload {
                version: 0,
                id: Uuid::nil(),
                revision: 0,
                kind: crate::assistant::ClarificationKind::FreeText,
                question: String::new(),
                options: vec![],
                fields: vec![],
                attempt: 0,
                source_intent: None,
                allow_free_text: true,
                is_missing_execution_parameters: false,
            }
        };
        let submission = match validate_submission(
            &payload,
            &client,
            input.clarification_id,
            input.clarification_revision,
            input.selected_option_id,
            input.source_message,
            input.answers,
        ) {
            Ok(submission) => submission,
            Err(error) => return Ok(RespondToChatJobOutcome::Validation(error.fields)),
        };
        let outcome = self
            .jobs
            .respond(input.job_id, client.user_id, submission.clone())
            .await?;
        let PersistResponseOutcome::Inserted(message) = outcome else {
            return Ok(match outcome {
                PersistResponseOutcome::NotFound => RespondToChatJobOutcome::NotFound,
                PersistResponseOutcome::NotActive if structured => {
                    RespondToChatJobOutcome::NotActive
                }
                PersistResponseOutcome::NotActive => RespondToChatJobOutcome::NotFound,
                PersistResponseOutcome::Stale => RespondToChatJobOutcome::Stale,
                PersistResponseOutcome::Inserted(_) => unreachable!(),
            });
        };
        let reference_instant = self
            .jobs
            .get_internal_for_user(input.job_id, client.user_id)
            .await?
            .expect("responded job exists")
            .created_at;
        self.run_graph_skeleton(
            message.session_id,
            input.job_id,
            &client,
            RuntimeUserInput {
                message: submission.display_message,
                source_message: message.content.clone(),
                selected_option_id: submission.selected_option_id,
                clarification_id: submission.clarification_id,
                clarification_revision: submission.clarification_revision,
                constraint_patch: submission.constraint_patch,
            },
            CanonicalTurn {
                message_id: message.id,
                observed_at: message.created_at,
                reference_instant,
                initial: false,
            },
        )
        .await?;
        Ok(RespondToChatJobOutcome::Inserted(message))
    }
}

#[derive(Debug)]
pub enum RespondToChatJobOutcome {
    Inserted(ChatMessage),
    NotFound,
    NotActive,
    Stale,
    Validation(Vec<String>),
}

pub(crate) fn redis_url_log_value(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let Some((_, host)) = rest.split_once('@') else {
        return url.to_string();
    };
    format!("{scheme}://***@{host}")
}
