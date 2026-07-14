use std::sync::Arc;

use anyhow::Result;
use app_core::auth::model::ClientContext;
use app_core::config::{ChatFeatureConfig, EmbeddingConfig, LlmConfig};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use tracing::warn;
use uuid::Uuid;

use crate::assistant::{
    AssistantDomain, AssistantEntity, AssistantEntityType, AssistantGraphRuntime,
    AssistantGraphTopology, AssistantIntentKind, AssistantLanguage, ContextBuilder,
    ContextReference, ContextWindowPolicy, JobMemoryRepository, LlmTraceRepository,
    MarkdownRenderer, Quantity, ResponseRenderer, RuntimeUserInput, SemanticRouter,
    SessionMemoryRepository, TerminalState,
    llm::{
        EmbeddingResponse, LlmClient, LlmPurpose, LlmResponse, SharedLlmClient, TokenUsage,
        rig_client::RigLlmClient,
        traced_client::{LlmTraceContext, TracedLlmClient},
    },
};
use crate::audit::AuditHandle;
use crate::chat::llm::LlmPlannerClient;
use crate::chat::model::{
    ChatJob, ChatJobAuditTimeline, ChatMessage, CreateChatJobInput, CreatedChatJob,
    RespondToChatJobInput,
};
use crate::chat::repository::{JobRepository, MessageRepository};
use crate::knowledge::embedding::VoyageEmbeddingClient;
use crate::knowledge::index::repository::KnowledgeRepository;
use crate::knowledge::model::KnowledgeCatalog;
use crate::policy::authorization::resolve_wildcard_grants;

#[derive(Clone)]
pub struct JobService {
    jobs: JobRepository,
    messages: MessageRepository,
    job_memory: JobMemoryRepository,
    session_memory: SessionMemoryRepository,
    context_builder: ContextBuilder,
    knowledge: KnowledgeRepository,
    runtime_knowledge_enabled: bool,
    fineract_pool: PgPool,
    catalog: Arc<KnowledgeCatalog>,
    llm: Option<SharedLlmClient>,
    llm_traces: LlmTraceRepository,
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
        _audit: AuditHandle,
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
            redis_url,
            redis,
        }
    }

    /// Emit a chat-job event: durable PG insert + best-effort Redis publish
    /// (`chat_job:{id}:latest_event`). SSE handlers poll the Redis key.
    /// ponytail: best-effort — Redis failures are warned but not propagated, since PG is the source of truth.
    async fn emit_event(
        &self,
        job_id: Uuid,
        kind: &str,
        step: Option<&str>,
        payload: Value,
    ) -> Result<()> {
        self.jobs
            .insert_event(job_id, kind, step, payload.clone())
            .await?;
        if let Some(client) = &self.redis {
            let body = json!({
                "kind": kind,
                "step": step,
                "payload": payload,
                "at": Utc::now(),
            })
            .to_string();
            let key = format!("chat_job:{job_id}:latest_event");
            match client.get_multiplexed_async_connection().await {
                Ok(mut conn) => {
                    let result: redis::RedisResult<()> =
                        redis::AsyncCommands::set_ex(&mut conn, key, body, 3600).await;
                    if let Err(error) = result {
                        warn!(job_id = %job_id, redis_url = %redis_url_log_value(&self.redis_url), error = %error, "redis publish event failed");
                    }
                    if matches!(kind, "final" | "error") {
                        let state_key = format!("chat_job:{job_id}:live_state");
                        let state = if kind == "final" {
                            "completed"
                        } else {
                            "failed"
                        };
                        let _: redis::RedisResult<()> = redis::AsyncCommands::set_ex(
                            &mut conn,
                            state_key,
                            state.to_string(),
                            3600,
                        )
                        .await;
                    }
                }
                Err(error) => {
                    warn!(job_id = %job_id, redis_url = %redis_url_log_value(&self.redis_url), error = %error, "redis connect failed")
                }
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, input), fields(api_key_id = %input.client.api_key_id))]
    pub async fn create(&self, input: CreateChatJobInput) -> Result<CreatedChatJob> {
        let mut client = input.client;
        resolve_wildcard_grants(&mut client, &self.catalog, &self.fineract_pool).await?;

        let mut created = self
            .jobs
            .create(
                client.api_key_id,
                input.session_id,
                input.message.clone(),
                serde_json::to_value(&client)?,
                json!({ "runtime": "semantic_assistant_graph" }),
                json!({}),
                json!({}),
            )
            .await?;
        self.session_memory
            .get_or_create(created.session_id)
            .await?;
        let outcome = self
            .run_graph_skeleton(
                created.session_id,
                created.job_id,
                &client,
                input.message.as_str().into(),
            )
            .await?;
        if let Some(outcome) = outcome {
            created.status = outcome.status.into();
            created.current_step = outcome.current_step.into();
        }
        Ok(created)
    }

    #[tracing::instrument(skip(self, client), fields(api_key_id = %client.api_key_id, job_id = %job_id))]
    pub async fn get(&self, client: ClientContext, job_id: Uuid) -> Result<Option<ChatJob>> {
        self.jobs.get_for_client(job_id, client.api_key_id).await
    }

    #[tracing::instrument(skip(self, client), fields(api_key_id = %client.api_key_id, job_id = %job_id))]
    pub async fn audit(
        &self,
        client: ClientContext,
        job_id: Uuid,
    ) -> Result<Option<ChatJobAuditTimeline>> {
        let Some(events) = self
            .jobs
            .list_audit_events_for_client(job_id, client.api_key_id)
            .await?
        else {
            return Ok(None);
        };

        Ok(Some(ChatJobAuditTimeline { job_id, events }))
    }

    #[tracing::instrument(skip(self, input), fields(api_key_id = %input.client.api_key_id, job_id = %input.job_id))]
    pub async fn respond(&self, input: RespondToChatJobInput) -> Result<Option<ChatMessage>> {
        let mut client = input.client;
        resolve_wildcard_grants(&mut client, &self.catalog, &self.fineract_pool).await?;

        let Some(message) = self
            .jobs
            .respond(
                input.job_id,
                client.api_key_id,
                input.source_message.clone(),
                input.selected_option_id.clone(),
            )
            .await?
        else {
            return Ok(None);
        };
        self.run_graph_skeleton(
            message.session_id,
            input.job_id,
            &client,
            RuntimeUserInput {
                message: input.message,
                source_message: message.content.clone(),
                selected_option_id: input.selected_option_id,
            },
        )
        .await?;
        Ok(Some(message))
    }

    async fn run_graph_skeleton(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        client: &ClientContext,
        input: RuntimeUserInput,
    ) -> Result<Option<JobRunOutcome>> {
        let context = self.context_builder.build(session_id, client).await?;
        let memory = match self.job_memory.get(job_id).await? {
            Some(memory) => memory,
            None => self.job_memory.create(job_id, "receive_message").await?,
        };
        let expected_revision = memory.revision;
        let traced_llm = self.llm.as_ref().map(|llm| {
            Arc::new(TracedLlmClient::new(
                llm.clone(),
                self.llm_traces.clone(),
                Some(LlmTraceContext {
                    job_id: Some(job_id),
                    session_id: Some(session_id),
                    api_key_id: client.api_key_id,
                    graph_state: Some("route_intent".into()),
                }),
            )) as SharedLlmClient
        });
        let router = traced_llm
            .as_ref()
            .map(|llm| SemanticRouter::new(llm.clone(), &self.catalog));
        let result = AssistantGraphRuntime::run_with_router(
            memory,
            context,
            router.as_ref(),
            traced_llm.as_ref(),
            self.runtime_knowledge_enabled.then_some(&self.knowledge),
            Some(&self.fineract_pool),
            Some(&self.catalog),
            Some(client),
            input,
        )
        .await;
        AssistantGraphTopology::new().validate_sequence(&result.transitions)?;
        let memory = self
            .job_memory
            .save(&result.memory, expected_revision)
            .await?;
        self.session_memory
            .update_after_job(
                session_id,
                &memory,
                result.pending_clarification.as_ref().map(|p| p.as_ref()),
            )
            .await?;
        self.job_memory
            .insert_checkpoint(
                &memory,
                json!({
                    "transitions": result.transitions.clone(),
                    "execution_summary": memory.execution_summary,
                }),
            )
            .await?;
        for transition in &result.transitions {
            AssistantGraphTopology::new().validate_transition(transition)?;
            self.job_memory
                .checkpoint_transition(
                    memory.job_id,
                    transition,
                    memory.revision,
                    json!({
                        "transition": transition,
                        "execution_summary": memory.execution_summary,
                    }),
                )
                .await?;
        }

        let Some(response) = &memory.structured_response else {
            return Ok(None);
        };
        let rendered = response
            .rendered_markdown
            .clone()
            .unwrap_or_else(|| MarkdownRenderer.render(response));
        let result_json = json!({
            "structured_response": response,
            "warnings": response.warnings.clone(),
            "markdown": rendered.clone(),
            "graph_state": memory.graph_state.clone(),
            "terminal_state": memory.terminal_state,
            "selected_capability": memory.selected_capability.clone(),
        });
        self.messages
            .insert_assistant_message(
                session_id,
                job_id,
                rendered.clone(),
                json!({ "type": "assistant_response", "assistant_response": response }),
            )
            .await?;
        let terminal_state = memory
            .terminal_state
            .unwrap_or(TerminalState::FailedOperational);
        let outcome = JobRunOutcome::from_terminal_state(terminal_state);
        match terminal_state {
            TerminalState::Completed => {
                self.jobs
                    .complete_with_assistant_response(job_id, result_json.clone())
                    .await?;
            }
            TerminalState::WaitingForUserInput => {
                self.jobs
                    .store_assistant_response_result(job_id, result_json.clone())
                    .await?;
                self.jobs.wait_for_user_input(job_id).await?;
            }
            TerminalState::FailedOperational => {
                self.jobs
                    .store_assistant_response_result(job_id, result_json.clone())
                    .await?;
                self.jobs
                    .fail(
                        job_id,
                        json!({
                            "code": "assistant_failed",
                            "message": "The assistant could not complete this request.",
                        }),
                    )
                    .await?;
            }
            TerminalState::BlockedByPolicy
            | TerminalState::Unsupported
            | TerminalState::OutOfDomain
            | TerminalState::ContextWindowExceeded => {
                self.jobs.complete(job_id, result_json.clone()).await?;
            }
        }
        self.emit_event(
            job_id,
            outcome.event_kind,
            Some("complete_or_wait"),
            json!({
                "response_type": response.response_type,
                "structured_response": response,
                "markdown": rendered,
            }),
        )
        .await?;
        Ok(Some(outcome))
    }
}

struct JobRunOutcome {
    status: &'static str,
    current_step: &'static str,
    event_kind: &'static str,
}

impl JobRunOutcome {
    fn from_terminal_state(state: TerminalState) -> Self {
        match state {
            TerminalState::WaitingForUserInput => Self {
                status: "waiting_for_user_input",
                current_step: "taking_decision",
                event_kind: "clarification",
            },
            TerminalState::FailedOperational => Self {
                status: "failed",
                current_step: "response",
                event_kind: "error",
            },
            _ => Self {
                status: "completed",
                current_step: "response",
                event_kind: "final",
            },
        }
    }
}

struct TestLlmClient;

#[async_trait]
impl LlmClient for TestLlmClient {
    async fn structured_value(
        &self,
        _purpose: LlmPurpose,
        _system: &str,
        user: &str,
        _schema: serde_json::Value,
    ) -> Result<LlmResponse<serde_json::Value>> {
        let message = serde_json::from_str::<serde_json::Value>(user)
            .ok()
            .and_then(|value| {
                value
                    .get("message")
                    .and_then(|message| message.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| user.to_owned());
        let lower = message.to_lowercase();
        let (intent, domain) = if lower == "hi" || lower == "hello" {
            (AssistantIntentKind::Greeting, AssistantDomain::Unknown)
        } else if lower.contains("bisa apa") || lower.contains("help") {
            (AssistantIntentKind::Help, AssistantDomain::Unknown)
        } else if lower.contains("laptop") {
            (AssistantIntentKind::OutOfDomain, AssistantDomain::Unknown)
        } else if lower.contains("loan")
            || lower.contains("charges")
            || lower.contains("fees")
            || lower.contains("tax")
            || lower.contains("accounting")
            || lower.contains("journal")
            || lower.contains(" gl ")
        {
            (
                AssistantIntentKind::UnsupportedInDomain,
                AssistantDomain::Unknown,
            )
        } else if lower.contains("raw account") {
            (AssistantIntentKind::UnsafeRequest, AssistantDomain::Client)
        } else if lower.contains("balance") || lower.contains("yang") {
            (
                AssistantIntentKind::ClarificationReply,
                AssistantDomain::Client,
            )
        } else if lower.contains("tony") || lower.contains("nama") {
            (AssistantIntentKind::DataLookup, AssistantDomain::Client)
        } else if lower.contains("office") || lower.contains("organization") {
            (
                AssistantIntentKind::ReportRequest,
                AssistantDomain::Organization,
            )
        } else if lower.contains("client") {
            (AssistantIntentKind::ReportRequest, AssistantDomain::Client)
        } else {
            (AssistantIntentKind::ReportRequest, AssistantDomain::Savings)
        };
        let mut entities = Vec::new();
        if lower.contains("tony") {
            entities.push(AssistantEntity {
                entity_type: AssistantEntityType::PersonName,
                value: "Tony".into(),
                canonical: Some("Tony".into()),
                confidence: Some(1.0),
            });
        }
        if lower.contains("account count") || lower.contains("savings accounts") {
            entities.push(AssistantEntity {
                entity_type: AssistantEntityType::Metric,
                value: "savings account count".into(),
                canonical: Some("savings account count".into()),
                confidence: Some(1.0),
            });
        } else if lower.contains("balance") {
            entities.push(AssistantEntity {
                entity_type: AssistantEntityType::Metric,
                value: "savings balance".into(),
                canonical: Some("savings balance".into()),
                confidence: Some(1.0),
            });
        } else if lower.contains("deposit") {
            entities.push(AssistantEntity {
                entity_type: AssistantEntityType::Metric,
                value: "deposit".into(),
                canonical: Some("deposit".into()),
                confidence: Some(1.0),
            });
        }
        let quantity = lower
            .split(|ch: char| !ch.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .find_map(|part| part.parse::<i64>().ok())
            .map(|value| Quantity::TopN { value });
        Ok(LlmResponse {
            value: json!({
                "intent": intent,
                "domain": domain,
                "language": AssistantLanguage::En,
                "entities": entities,
                "constraints": { "quantity": quantity },
                "context_reference": ContextReference::None,
                "confidence": 0.9,
                "reason": "test semantic router"
            }),
            usage: TokenUsage::default(),
            cost_usd: None,
            provider: "test".into(),
            model: "test".into(),
            latency_ms: 0,
        })
    }

    async fn embed(&self, _purpose: LlmPurpose, text: &str) -> Result<EmbeddingResponse> {
        let text = text.to_lowercase();
        Ok(EmbeddingResponse {
            vector: vec![
                text.matches("client").count() as f32 + text.matches("tony").count() as f32,
                text.matches("saving").count() as f32,
                text.matches("balance").count() as f32,
                text.matches("deposit").count() as f32,
            ],
            usage: TokenUsage::default(),
            cost_usd: None,
            provider: "test".into(),
            model: "test".into(),
            latency_ms: 0,
        })
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
