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
use crate::job::repository::{JobRepository, assistant_memory::JobMemoryRepository};
use crate::knowledge::embedding::VoyageEmbeddingClient;
use crate::knowledge::index::repository::KnowledgeRepository;
use crate::knowledge::model::KnowledgeCatalog;
use crate::policy::authorization::project_admin_principal;

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
    pub async fn respond(&self, input: RespondToChatJobInput) -> Result<Option<ChatMessage>> {
        let mut client = input.client;
        project_admin_principal(&mut client, &self.catalog, &self.fineract_pool).await?;
        let Some(message) = self
            .jobs
            .respond(
                input.job_id,
                client.user_id,
                input.source_message.clone(),
                input.selected_option_id.clone(),
            )
            .await?
        else {
            return Ok(None);
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
                message: input.message,
                source_message: message.content.clone(),
                selected_option_id: input.selected_option_id,
            },
            CanonicalTurn {
                message_id: message.id,
                observed_at: message.created_at,
                reference_instant,
                initial: false,
            },
        )
        .await?;
        Ok(Some(message))
    }
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
