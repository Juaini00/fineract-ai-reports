use std::{
    collections::BTreeMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use anyhow::Result;
use app_core::auth::model::PrincipalContext;
use app_core::config::{CanonicalGatewayMode, ChatFeatureConfig, EmbeddingConfig, LlmConfig};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use tracing::warn;
use uuid::Uuid;

use crate::assistant::runtime::CanonicalRuntimeContext;
use crate::assistant::{
    AssistantDomain, AssistantEntity, AssistantEntityType, AssistantGraphRuntime,
    AssistantGraphTopology, AssistantIntentKind, AssistantLanguage, CanonicalStateRepository,
    ContextBuilder, ContextReference, ContextWindowPolicy, DeterministicExtraction,
    EffectiveConstraints, ExtractionProvenance, FactSourceKind, JobMemory, JobMemoryRepository,
    LlmTraceRepository, MarkdownRenderer, OriginalIntent, PlannerInputSnapshot,
    PrincipalProjection, Quantity, RequestGrouping, RequestOperation, RequestOutput, RequestPii,
    RequestShape, RequestSubject, ResponseRenderer, RuntimeUserInput, SemanticRouter,
    SessionMemoryRepository, TerminalState, deterministic_observations,
    executable_constraint_contracts,
    llm::{
        EmbeddingResponse, LlmClient, LlmPurpose, LlmResponse, SharedLlmClient, TokenUsage,
        rig_client::RigLlmClient,
        traced_client::{LlmTraceContext, TracedLlmClient},
    },
    merge_observations, original_request_observations, stable_uuid,
};
use crate::audit::{AuditEvent, AuditHandle};
use crate::chat::llm::LlmPlannerClient;
use crate::chat::model::{
    ChatJob, ChatJobAuditTimeline, ChatMessage, CreateChatJobInput, CreatedChatJob,
    RespondToChatJobInput,
};
use crate::chat::repository::{JobRepository, MessageRepository};
use crate::knowledge::embedding::VoyageEmbeddingClient;
use crate::knowledge::index::repository::KnowledgeRepository;
use crate::knowledge::model::KnowledgeCatalog;
use crate::policy::authorization::project_admin_principal;

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
            .get_for_user(created.job_id, client.user_id, false)
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
            .get_for_user(input.job_id, client.user_id, false)
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

    async fn run_graph_skeleton(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        client: &PrincipalContext,
        input: RuntimeUserInput,
        canonical_turn: CanonicalTurn,
    ) -> Result<Option<JobRunOutcome>> {
        let context = self.context_builder.build(session_id, client).await?;
        let memory = match self.job_memory.get(job_id, client.user_id).await? {
            Some(memory) => memory,
            None => {
                self.job_memory
                    .create(job_id, client.user_id, "receive_message")
                    .await?
            }
        };
        let expected_revision = memory.revision;
        let runtime_llm = self.llm.as_ref().map(|llm| {
            Arc::new(TracedLlmClient::new(
                llm.clone(),
                self.llm_traces.clone(),
                Some(LlmTraceContext {
                    job_id: Some(job_id),
                    session_id: Some(session_id),
                    user_id: client.user_id,
                    legacy_api_key_id: client.legacy_api_key_id,
                    graph_state: Some("route_intent".into()),
                }),
            )) as SharedLlmClient
        });
        let router = runtime_llm
            .as_ref()
            .map(|llm| SemanticRouter::new(llm.clone(), &self.catalog));
        let catalog_version = if self.canonical_mode == CanonicalGatewayMode::Authoritative {
            self.knowledge
                .latest_embedded_catalog()
                .await?
                .map(|version| version.id)
        } else {
            None
        };
        let canonical = CanonicalRuntimeContext {
            mode: self.canonical_mode,
            repository: self.canonical_state.clone(),
            catalog_version,
            message_id: canonical_turn.message_id,
            observed_at: canonical_turn.observed_at,
            reference_instant: canonical_turn.reference_instant,
            timezone: "Asia/Jakarta".into(),
            revision: expected_revision,
            initial: canonical_turn.initial,
        };
        let mut result = AssistantGraphRuntime::run_with_router(
            memory,
            context,
            router.as_ref(),
            runtime_llm.as_ref(),
            self.runtime_knowledge_enabled.then_some(&self.knowledge),
            Some(&self.fineract_pool),
            Some(&self.catalog),
            Some(client),
            Some(&canonical),
            input,
        )
        .await;
        if self.canonical_mode == CanonicalGatewayMode::Shadow
            && let Err(_error) = self
                .shadow_write(
                    &mut result.memory,
                    client,
                    canonical_turn,
                    expected_revision,
                )
                .await
        {
            warn!(job_id = %job_id, "canonical shadow write failed");
        }
        // Best-effort audit trace (issue 06): never fail the request on this write.
        if let Some(trace) = result.retrieval_trace.clone() {
            self.jobs
                .merge_retrieval_trace(job_id, client.user_id, trace)
                .await
                .ok();
        }
        AssistantGraphTopology::new().validate_sequence(&result.transitions)?;
        let memory = self
            .job_memory
            .save(&result.memory, expected_revision)
            .await?;
        self.session_memory
            .update_after_job(
                session_id,
                client.user_id,
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
                    "planner_snapshot_id": memory.planner_snapshot_id,
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
                        "planner_snapshot_id": memory.planner_snapshot_id,
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

    async fn shadow_write(
        &self,
        memory: &mut JobMemory,
        client: &PrincipalContext,
        turn: CanonicalTurn,
        revision: i64,
    ) -> Result<()> {
        let source_id = turn.message_id.to_string();
        let extraction = memory
            .current_user_message_metadata
            .get("deterministic_extraction")
            .cloned()
            .and_then(|value| serde_json::from_value::<DeterministicExtraction>(value).ok())
            .unwrap_or_default();
        let effective = if turn.initial {
            if self
                .canonical_state
                .get_original_intent(memory.job_id)
                .await?
                .is_some()
            {
                self.canonical_state
                    .get_effective_constraints(memory.job_id, 0)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("missing canonical baseline"))?
            } else {
                let intent = memory
                    .intent
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("missing accepted initial parse"))?;
                let mut original = OriginalIntent {
                    id: stable_uuid(memory.job_id, 1),
                    job_id: memory.job_id,
                    schema_version: 1,
                    raw_message_id: turn.message_id,
                    locale: intent.language.clone(),
                    action: intent.intent.clone(),
                    entities: intent.entities.clone(),
                    metrics: intent.constraints.metric.clone().into_iter().collect(),
                    groupings: Vec::new(),
                    output: None,
                    parameters: BTreeMap::new(),
                    pii_request: false,
                    extraction_provenance: vec![ExtractionProvenance {
                        extractor: "semantic_router".into(),
                        version: "legacy_shadow_v1".into(),
                        source_identifiers: vec![source_id.clone()],
                        source_spans: Vec::new(),
                        rule: None,
                        reference_instant: None,
                        timezone: None,
                    }],
                    created_at: turn.reference_instant,
                };
                if let Some(provenance) = &extraction.temporal_provenance {
                    original.extraction_provenance.push(ExtractionProvenance {
                        extractor: "deterministic_temporal_resolver".into(),
                        version: "v1".into(),
                        source_identifiers: vec![source_id.clone()],
                        source_spans: vec![provenance.phrase_span],
                        rule: Some(provenance.rule.clone()),
                        reference_instant: Some(provenance.reference_instant),
                        timezone: Some(provenance.timezone.clone()),
                    });
                }
                let observations = original_request_observations(
                    memory.job_id,
                    &source_id,
                    intent,
                    &extraction,
                    turn.observed_at,
                );
                let mut effective = merge_observations(
                    memory.job_id,
                    0,
                    &observations,
                    &executable_constraint_contracts(),
                )?;
                effective.id = stable_uuid(memory.job_id, 2);
                effective.created_at = turn.observed_at;
                self.canonical_state
                    .insert_initial_state(&original, &observations, &effective)
                    .await?
                    .2
            }
        } else {
            let first_sequence = self
                .canonical_state
                .list_observations(memory.job_id)
                .await?
                .len() as i64
                + 1;
            let observations = deterministic_observations(
                memory.job_id,
                &source_id,
                first_sequence,
                FactSourceKind::Clarification,
                &extraction,
                turn.observed_at,
            );
            self.canonical_state
                .append_observations(memory.job_id, &observations)
                .await?;
            self.canonical_state
                .derive_and_insert_effective(
                    memory.job_id,
                    revision,
                    &executable_constraint_contracts(),
                )
                .await?
        };
        self.shadow_snapshot(memory, client, turn.reference_instant, &effective)
            .await?;
        let canonical_hash = sanitized_hash(&effective.values);
        let legacy_hash = sanitized_hash(&memory.tool_params);
        tracing::info!(
            job_id = %memory.job_id,
            revision = effective.revision,
            decision_code = ?memory.terminal_state,
            selected_capability_id = memory.selected_capability.as_deref().unwrap_or("none"),
            field_count = effective.values.len(),
            field_names = ?effective.values.keys().collect::<Vec<_>>(),
            canonical_hash,
            legacy_hash,
            "canonical shadow comparison"
        );
        Ok(())
    }

    async fn shadow_snapshot(
        &self,
        memory: &mut JobMemory,
        client: &PrincipalContext,
        reference_instant: DateTime<Utc>,
        effective: &EffectiveConstraints,
    ) -> Result<()> {
        let (Some(capability), Some(catalog_version), Some(original)) = (
            memory.selected_capability.clone(),
            self.knowledge.latest_embedded_catalog().await?,
            self.canonical_state
                .get_original_intent(memory.job_id)
                .await?,
        ) else {
            return Ok(());
        };
        let snapshot = PlannerInputSnapshot {
            id: stable_uuid(memory.job_id, effective.revision as u128 + 100),
            job_id: memory.job_id,
            revision: effective.revision,
            original_intent_id: original.id,
            effective_constraints_id: effective.id,
            capability_catalog_version: catalog_version.id,
            principal_projection: PrincipalProjection {
                user_id: client.user_id,
                role: client.role.clone(),
                capability_ids: client.capability_ids.clone(),
                office_ids: client.office_ids.clone(),
                can_view_pii: client.can_view_pii,
                legacy_api_key_id: client.legacy_api_key_id,
            },
            reference_instant,
            timezone: "Asia/Jakarta".into(),
            selected_capability_id: capability,
            normalized_parameters: memory.tool_params.clone(),
            created_at: effective.created_at,
        };
        memory.planner_snapshot_id = Some(
            self.canonical_state
                .insert_planner_snapshot(&snapshot)
                .await?
                .id,
        );
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct CanonicalTurn {
    message_id: Uuid,
    observed_at: DateTime<Utc>,
    reference_instant: DateTime<Utc>,
    initial: bool,
}

fn sanitized_hash(value: &impl serde::Serialize) -> u64 {
    let mut hasher = DefaultHasher::new();
    serde_json::to_vec(value)
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
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
        // Reranker calls carry a `candidates` array; sniff and answer without
        // faking a router intent.
        if let Some(value) = serde_json::from_str::<Value>(user).ok()
            && let Some(candidates) = value.get("candidates").and_then(|c| c.as_array())
            && !candidates.is_empty()
        {
            let query = value
                .get("query")
                .and_then(|q| q.as_str())
                .unwrap_or("")
                .to_lowercase();
            return Ok(LlmResponse {
                value: test_reranker_pick(&query, candidates),
                usage: TokenUsage::default(),
                cost_usd: None,
                provider: "test".into(),
                model: "test".into(),
                latency_ms: 0,
            });
        }
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
        } else if lower.contains("office") || lower.contains("organization") {
            (
                AssistantIntentKind::ReportRequest,
                AssistantDomain::Organization,
            )
        } else if lower.contains("balance") || lower.contains("yang") {
            (
                AssistantIntentKind::ClarificationReply,
                AssistantDomain::Client,
            )
        } else if lower.contains("tony") || lower.contains("nama") {
            (AssistantIntentKind::DataLookup, AssistantDomain::Client)
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
        } else if lower.contains("withdrawal") {
            entities.push(AssistantEntity {
                entity_type: AssistantEntityType::Metric,
                value: "withdrawal".into(),
                canonical: Some("withdrawal".into()),
                confidence: Some(1.0),
            });
        }
        let quantity = lower
            .split(|ch: char| !ch.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .find_map(|part| part.parse::<i64>().ok())
            .map(|value| Quantity::TopN { value });
        let request_shape = if lower.contains("random") || lower.contains("sembarang") {
            RequestShape {
                operation: RequestOperation::RandomSample,
                subject: RequestSubject::Client,
                grouping: RequestGrouping::None,
                output: RequestOutput::List,
                pii: RequestPii::ClientIdentity,
            }
        } else if lower.contains("hierarchy") {
            RequestShape {
                operation: RequestOperation::Summary,
                subject: RequestSubject::OrganizationHierarchy,
                grouping: RequestGrouping::None,
                output: RequestOutput::Summary,
                pii: RequestPii::None,
            }
        } else if lower.contains("office") || lower.contains("organization") {
            let per_month = lower.contains("monthly")
                || lower.contains("per month")
                || lower.contains("per bulan");
            let ranks = lower.contains("top")
                || lower.contains("ranking")
                || lower.contains("dormant")
                || lower.contains("busiest")
                || lower.contains("list");
            let (operation, grouping, output) = if per_month {
                (
                    RequestOperation::Trend,
                    RequestGrouping::Month,
                    RequestOutput::TimeSeries,
                )
            } else if ranks {
                (
                    RequestOperation::Rank,
                    RequestGrouping::Office,
                    RequestOutput::Ranking,
                )
            } else {
                (
                    RequestOperation::Summary,
                    RequestGrouping::None,
                    RequestOutput::Summary,
                )
            };
            RequestShape {
                operation,
                subject: RequestSubject::Office,
                grouping,
                output,
                pii: RequestPii::None,
            }
        } else if lower.contains("tony") || lower.contains("nama") {
            RequestShape {
                operation: RequestOperation::Lookup,
                subject: RequestSubject::Client,
                grouping: RequestGrouping::None,
                output: RequestOutput::Lookup,
                pii: RequestPii::ClientIdentity,
            }
        } else if lower.contains("client") && (lower.contains("top") || lower.contains("most")) {
            RequestShape {
                operation: RequestOperation::Rank,
                subject: RequestSubject::Client,
                grouping: RequestGrouping::None,
                output: RequestOutput::Ranking,
                pii: RequestPii::ClientIdentity,
            }
        } else if lower.contains("saving")
            || lower.contains("deposit")
            || lower.contains("withdrawal")
        {
            let per_month = lower.contains("monthly")
                || lower.contains("per month")
                || lower.contains("per bulan");
            let top = lower.contains("top") || lower.contains("teratas");
            let total = lower.contains("total") && !top;
            let portfolio = lower.contains("portfolio") || lower.contains("balance summary");
            let (operation, grouping, output, subject) = if portfolio {
                (
                    RequestOperation::Summary,
                    RequestGrouping::None,
                    RequestOutput::Summary,
                    RequestSubject::SavingsAccount,
                )
            } else if per_month && top {
                (
                    RequestOperation::Rank,
                    RequestGrouping::Month,
                    RequestOutput::Ranking,
                    RequestSubject::SavingsTransaction,
                )
            } else if per_month {
                (
                    RequestOperation::Trend,
                    RequestGrouping::Month,
                    RequestOutput::TimeSeries,
                    RequestSubject::SavingsTransaction,
                )
            } else if top {
                (
                    RequestOperation::Rank,
                    RequestGrouping::None,
                    RequestOutput::Ranking,
                    RequestSubject::SavingsTransaction,
                )
            } else if total {
                (
                    RequestOperation::Total,
                    RequestGrouping::None,
                    RequestOutput::Scalar,
                    RequestSubject::SavingsTransaction,
                )
            } else {
                (
                    RequestOperation::Unknown,
                    RequestGrouping::Unknown,
                    RequestOutput::Unknown,
                    RequestSubject::Unknown,
                )
            };
            RequestShape {
                operation,
                subject,
                grouping,
                output,
                pii: RequestPii::Unknown,
            }
        } else {
            RequestShape::default()
        };
        Ok(LlmResponse {
            value: json!({
                "intent": intent,
                "domain": domain,
                "request_shape": request_shape,
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

/// Test-only reranker heuristic: pick the candidate whose id/title/description
/// shares the most alphanumeric tokens with the query, tie-broken by original
/// retrieval score. High-margin winner → Select at confidence 0.9. Otherwise
/// Clarify with the top-4 candidates as alternatives. Mirrors the semantic
/// picks a real LLM would make well enough for integration tests.
fn test_reranker_pick(query: &str, candidates: &[Value]) -> Value {
    // 4-char prefix substring matching: handles simple plurals/inflections
    // ("deposit" ↔ "deposits", "month" ↔ "monthly") without a stemmer.
    let query_probes: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 3)
        .map(|s| {
            let lower = s.to_lowercase();
            if lower.len() > 4 {
                lower[..4].to_string()
            } else {
                lower
            }
        })
        .collect();
    let mut ordered: Vec<(usize, usize, &Value)> = candidates
        .iter()
        .enumerate()
        .map(|(idx, c)| {
            let examples = c
                .get("examples")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| e.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let hay = format!(
                "{} {} {} {}",
                c.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                c.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                c.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                examples,
            )
            .to_lowercase();
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            let hits = query_probes
                .iter()
                .filter(|probe| seen.insert(probe.as_str()) && hay.contains(probe.as_str()))
                .count();
            (hits, idx, c)
        })
        .collect();
    // Specificity mismatch penalty: candidate id claims a grouping the query
    // never asked for (a "monthly" cap when the query lacks any monthly cue).
    // Cheap tie-breaker that mimics what a real LLM would penalize.
    let query_lower = query.to_lowercase();
    let query_wants_monthly = query_lower.contains("month") || query_lower.contains("per ");
    for entry in ordered.iter_mut() {
        let id = entry
            .2
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        if !query_wants_monthly && id.contains("monthly") && entry.0 > 0 {
            entry.0 -= 1;
        }
    }
    ordered.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let top_hits = ordered.first().map(|(h, _, _)| *h).unwrap_or(0);
    let next_hits = ordered.get(1).map(|(h, _, _)| *h).unwrap_or(0);
    let winner = ordered.first().and_then(|(_, _, c)| c.get("id"));

    if top_hits >= 3 && top_hits > next_hits {
        json!({
            "decision": "select",
            "capability_id": winner,
            "confidence": 0.9,
            "alternatives": [],
            "reason": "test reranker: dominant keyword match",
        })
    } else {
        // ponytail: 6 (not 4) so canonical siblings that alphabetically
        // sort late (e.g. `savings_deposit_total` follows `_top_n`) still
        // land in test clarification options. Real reranker returns 2-4.
        let alternatives: Vec<Value> = ordered
            .iter()
            .take(6)
            .filter_map(|(_, _, c)| c.get("id").cloned())
            .collect();
        json!({
            "decision": "clarify",
            "capability_id": null,
            "confidence": 0.0,
            "alternatives": alternatives,
            "reason": "test reranker: ambiguous top-1",
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
