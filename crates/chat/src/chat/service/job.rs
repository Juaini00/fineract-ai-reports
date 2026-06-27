use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, bail};
use app_core::auth::model::ClientContext;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use tracing::warn;
use uuid::Uuid;

use crate::chat::classifier::{
    ClarificationOption, ClassificationCandidate, ClassificationOutcome, ClassificationResult,
    clarify_retrieved_capabilities, classify_clarification_response, classify_retrieved_capability,
};
use crate::chat::executor::execute_plan;
use crate::chat::formatter::format_report_response;
use crate::chat::model::{
    ChatJob, ChatMessage, CreateChatJobInput, CreatedChatJob, RespondToChatJobInput,
};
use crate::chat::planner::{build_execution_plan, evaluate_policy};
use crate::chat::repository::{JobRepository, MessageRepository};
use crate::knowledge::embedding::VoyageEmbeddingClient;
use crate::knowledge::index::repository::{KnowledgeRepository, RetrievedKnowledgeCandidate};
use crate::knowledge::model::{CapabilityKnowledge, KnowledgeCatalog};

#[derive(Clone)]
pub struct JobService {
    jobs: JobRepository,
    messages: MessageRepository,
    fineract_pool: PgPool,
    catalog: Arc<KnowledgeCatalog>,
    knowledge: KnowledgeRepository,
    embedding_client: VoyageEmbeddingClient,
    redis: Option<redis::Client>,
}

impl JobService {
    pub fn new(
        jobs: JobRepository,
        messages: MessageRepository,
        app_pool: PgPool,
        fineract_pool: PgPool,
        catalog: Arc<KnowledgeCatalog>,
        embedding_client: VoyageEmbeddingClient,
        redis: Option<redis::Client>,
    ) -> Self {
        Self {
            jobs,
            messages,
            fineract_pool,
            catalog,
            knowledge: KnowledgeRepository::new(app_pool),
            embedding_client,
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
                        warn!(job_id = %job_id, error = %error, "redis publish event failed");
                    }
                    if matches!(kind, "final" | "error") {
                        let state_key = format!("chat_job:{job_id}:live_state");
                        let state = if kind == "final" { "completed" } else { "failed" };
                        let _: redis::RedisResult<()> = redis::AsyncCommands::set_ex(
                            &mut conn,
                            state_key,
                            state.to_string(),
                            3600,
                        )
                        .await;
                    }
                }
                Err(error) => warn!(job_id = %job_id, error = %error, "redis connect failed"),
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, input), fields(api_key_id = %input.client.api_key_id))]
    pub async fn create(&self, input: CreateChatJobInput) -> Result<CreatedChatJob> {
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
            "can_view_pii": input.client.can_view_pii,
        });
        let classification = self.classify_with_retrieval(&message, &input.client).await;
        let execution_plan = build_execution_plan(&classification, &self.catalog);
        let policy_decision = evaluate_policy(&input.client, execution_plan.as_ref());
        let classification_json = serde_json::to_value(&classification)?;
        let execution_plan_json = serde_json::to_value(&execution_plan)?;
        let policy_decision_json = serde_json::to_value(&policy_decision)?;

        let job = self
            .jobs
            .create(
                input.client.api_key_id,
                input.session_id,
                message,
                client_context_json,
                classification_json,
                execution_plan_json,
                policy_decision_json,
            )
            .await?;

        self.jobs
            .insert_checkpoint(
                job.job_id,
                "queued",
                "job_created",
                json!({
                    "session_id": job.session_id,
                    "user_message_id": job.user_message_id,
                    "status": job.status,
                }),
            )
            .await?;

        self.emit_event(
            job.job_id,
            "status",
            Some("queued"),
            json!({
                "status": job.status,
                "current_step": job.current_step,
            }),
        )
        .await?;

        let worker = self.clone();
        let session_id = job.session_id;
        let job_id = job.job_id;
        let plan_for_worker = execution_plan.clone();
        let policy_for_worker = policy_decision.clone();
        let classification_for_worker = classification.clone();
        tokio::spawn(async move {
            if let Err(error) = worker
                .run_pipeline(
                    session_id,
                    job_id,
                    classification_for_worker,
                    plan_for_worker,
                    policy_for_worker,
                )
                .await
            {
                warn!(job_id = %job_id, error = %error, "chat job background pipeline failed");
            }
        });

        Ok(job)
    }

    async fn run_pipeline(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        classification: ClassificationResult,
        execution_plan: Option<crate::chat::planner::ExecutionPlan>,
        policy_decision: crate::chat::planner::PolicyDecision,
    ) -> Result<()> {
        if classification.outcome == ClassificationOutcome::ClarificationRequired {
            self.write_clarification(session_id, job_id, &classification)
                .await?;
            return Ok(());
        }

        if let Some(plan) = execution_plan.as_ref() {
            self.execute_and_finish(session_id, job_id, plan, &policy_decision)
                .await?;
        } else if classification.outcome == ClassificationOutcome::Unsupported {
            self.fail_unsupported(job_id).await?;
        }

        Ok(())
    }

    async fn classify_with_retrieval(
        &self,
        message: &str,
        client: &ClientContext,
    ) -> ClassificationResult {
        let today = Utc::now().date_naive();
        if is_write_intent(message) {
            return unsupported_result("write_intent", Vec::new());
        }

        if client.allowed_capabilities.is_empty() {
            return unsupported_result("no_allowed_capabilities", Vec::new());
        }

        match self.embedding_client.embed_query(message).await {
            Ok(embedding) => match self
                .knowledge
                .search_capabilities(embedding.clone(), &client.allowed_capabilities, 3)
                .await
            {
                Ok(candidates) => {
                    let context = self
                        .knowledge
                        .search_context(embedding, 5)
                        .await
                        .unwrap_or_else(|error| {
                            warn!(error = %error, "knowledge context search failed; continuing without context");
                            Vec::new()
                        });
                    let mut result = self
                        .classify_from_candidates(message, today, &candidates)
                        .unwrap_or_else(|| unsupported_result("vector_no_match", candidates));
                    attach_context_candidates(&mut result, &context);
                    return result;
                }
                Err(error) => {
                    warn!(error = %error, "knowledge vector search failed; using catalog lexical retrieval");
                }
            },
            Err(error) => {
                warn!(error = %error, "query embedding failed; using catalog lexical retrieval");
            }
        }

        let candidates = self.catalog_lexical_candidates(message, &client.allowed_capabilities, 3);
        self.classify_from_candidates(message, today, &candidates)
            .unwrap_or_else(|| unsupported_result("catalog_no_match", candidates))
    }

    fn catalog_lexical_candidates(
        &self,
        message: &str,
        allowed_capabilities: &[String],
        limit: usize,
    ) -> Vec<RetrievedKnowledgeCandidate> {
        let message_tokens = tokens(message);
        if message_tokens.is_empty() {
            return Vec::new();
        }

        let mut candidates = self
            .catalog
            .capabilities
            .iter()
            .filter(|capability| {
                capability.status == "approved_mvp"
                    && allowed_capabilities.iter().any(|id| id == &capability.id)
            })
            .filter_map(|capability| {
                let text = capability_retrieval_text(capability);
                let score = lexical_confidence(&message_tokens, &text);
                (score >= 0.40).then(|| RetrievedKnowledgeCandidate {
                    source_type: "capability".to_string(),
                    source_id: capability.id.clone(),
                    title: format!("Capability {}", capability.id),
                    retrieval_text: text,
                    metadata_json: Value::Null,
                    distance: 1.0 - f64::from(score),
                })
            })
            .collect::<Vec<_>>();

        candidates.sort_by(|left, right| left.distance.total_cmp(&right.distance));
        candidates.truncate(limit);
        candidates
    }

    fn classify_from_candidates(
        &self,
        message: &str,
        today: chrono::NaiveDate,
        candidates: &[RetrievedKnowledgeCandidate],
    ) -> Option<ClassificationResult> {
        let top = candidates.first()?;
        let top_capability = self.catalog_capability(&top.source_id)?;
        let confidence = vector_confidence(top.distance);

        if confidence < 0.40 {
            return None;
        }

        let classification_candidates = candidates
            .iter()
            .map(|candidate| ClassificationCandidate {
                capability: candidate.source_id.clone(),
                confidence: vector_confidence(candidate.distance),
                source_type: Some(candidate.source_type.clone()),
            })
            .collect::<Vec<_>>();

        let close_candidates = candidates
            .iter()
            .filter(|candidate| vector_confidence(candidate.distance) >= confidence - 0.05)
            .filter_map(|candidate| self.catalog_capability(&candidate.source_id))
            .collect::<Vec<_>>();

        if close_candidates.len() > 1 || confidence < 0.55 {
            let options = close_candidates
                .into_iter()
                .map(capability_option)
                .collect::<Vec<_>>();
            return Some(clarify_retrieved_capabilities(
                message,
                today,
                Some(top_capability.domain.clone()),
                options,
                confidence,
                classification_candidates,
            ));
        }

        Some(classify_retrieved_capability(
            message,
            today,
            &top_capability.domain,
            &top_capability.id,
            &top_capability.output_mode,
            confidence,
            classification_candidates,
        ))
    }

    fn catalog_capability(&self, capability_id: &str) -> Option<&CapabilityKnowledge> {
        self.catalog.capabilities.iter().find(|capability| {
            capability.id == capability_id && capability.status == "approved_mvp"
        })
    }

    #[tracing::instrument(skip(self, client), fields(api_key_id = %client.api_key_id, job_id = %job_id))]
    pub async fn get(&self, client: ClientContext, job_id: Uuid) -> Result<Option<ChatJob>> {
        self.jobs.get_for_client(job_id, client.api_key_id).await
    }

    #[tracing::instrument(skip(self, input), fields(api_key_id = %input.client.api_key_id, job_id = %input.job_id))]
    pub async fn respond(&self, input: RespondToChatJobInput) -> Result<Option<ChatMessage>> {
        let message = input.message.trim().to_string();
        if message.is_empty() {
            bail!("message is required");
        }

        let Some(job) = self
            .jobs
            .get_for_client(input.job_id, input.client.api_key_id)
            .await?
        else {
            return Ok(None);
        };

        let Some(response) = self
            .jobs
            .respond(input.job_id, input.client.api_key_id, message)
            .await?
        else {
            return Ok(None);
        };

        if let Some(original) = job
            .state_json
            .get("classification")
            .and_then(|value| serde_json::from_value::<ClassificationResult>(value.clone()).ok())
        {
            let classification = classify_clarification_response(&original, &response.content);
            let execution_plan = build_execution_plan(&classification, &self.catalog);
            let policy_decision = evaluate_policy(&input.client, execution_plan.as_ref());

            self.jobs
                .update_plan_state(
                    input.job_id,
                    serde_json::to_value(&classification)?,
                    serde_json::to_value(&execution_plan)?,
                    serde_json::to_value(&policy_decision)?,
                )
                .await?;

            let worker = self.clone();
            let session_id = job.session_id;
            let job_id = input.job_id;
            tokio::spawn(async move {
                if let Err(error) = worker
                    .run_pipeline(session_id, job_id, classification, execution_plan, policy_decision)
                    .await
                {
                    warn!(job_id = %job_id, error = %error, "chat job clarification pipeline failed");
                }
            });
        }

        Ok(Some(response))
    }

    async fn write_clarification(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        classification: &ClassificationResult,
    ) -> Result<()> {
        let content = classification
            .clarification
            .clone()
            .unwrap_or_else(|| "Please clarify your request.".to_string());

        self.jobs.wait_for_user_input(job_id).await?;
        self.messages
            .insert_assistant_message(
                session_id,
                job_id,
                content,
                json!({
                    "type": "clarification",
                    "options": classification.options,
                }),
            )
            .await?;
        self.jobs
            .insert_checkpoint(
                job_id,
                "taking_decision",
                "clarification_required",
                json!({ "options": classification.options }),
            )
            .await?;
        self.emit_event(
            job_id,
            "clarification",
            Some("taking_decision"),
            json!({ "options": classification.options }),
        )
        .await?;

        Ok(())
    }

    async fn fail_unsupported(&self, job_id: Uuid) -> Result<()> {
        self.jobs
            .fail(
                job_id,
                json!({
                    "code": "unsupported_request",
                    "message": "No approved reporting capability matched this request.",
                }),
            )
            .await?;
        self.jobs
            .insert_checkpoint(
                job_id,
                "taking_decision",
                "job_failed",
                json!({ "code": "unsupported_request" }),
            )
            .await?;
        self.emit_event(
            job_id,
            "error",
            Some("taking_decision"),
            json!({
                "code": "unsupported_request",
                "message": "No approved reporting capability matched this request.",
            }),
        )
        .await?;

        Ok(())
    }

    async fn execute_and_finish(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        plan: &crate::chat::planner::ExecutionPlan,
        policy_decision: &crate::chat::planner::PolicyDecision,
    ) -> Result<()> {
        let started_at = Instant::now();
        match execute_plan(&self.fineract_pool, &self.catalog, plan, policy_decision).await {
            Ok(mut result) => {
                let latency_ms = started_at.elapsed().as_millis() as u64;
                let row_count = result
                    .get("row_count")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);

                if let Some(result) = result.as_object_mut() {
                    result.insert("latency_ms".to_string(), json!(latency_ms));
                }

                if let Some(content) = format_report_response(plan, &result) {
                    self.messages
                        .insert_assistant_response(session_id, job_id, content)
                        .await?;
                }
                self.jobs.complete(job_id, result).await?;
                self.jobs
                    .insert_checkpoint(
                        job_id,
                        "response",
                        "response_completed",
                        json!({
                            "row_count": row_count,
                            "latency_ms": latency_ms,
                        }),
                    )
                    .await?;
                self.emit_event(
                    job_id,
                    "final",
                    Some("response"),
                    json!({
                        "status": "completed",
                        "row_count": row_count,
                        "latency_ms": latency_ms,
                    }),
                )
                .await?;
            }
            Err(error) => {
                let latency_ms = started_at.elapsed().as_millis() as u64;
                warn!(job_id = %job_id, error = %error, "chat job execution failed");

                self.jobs
                    .fail(
                        job_id,
                        json!({
                            "code": "execution_failed",
                            "message": "Report execution failed.",
                            "latency_ms": latency_ms,
                        }),
                    )
                    .await?;
                self.jobs
                    .insert_checkpoint(
                        job_id,
                        "response",
                        "job_failed",
                        json!({
                            "code": "execution_failed",
                            "latency_ms": latency_ms,
                        }),
                    )
                    .await?;
                self.emit_event(
                    job_id,
                    "error",
                    Some("response"),
                    json!({
                        "code": "execution_failed",
                        "message": "Report execution failed.",
                        "latency_ms": latency_ms,
                    }),
                )
                .await?;
            }
        }

        Ok(())
    }
}

fn vector_confidence(distance: f64) -> f32 {
    (1.0 - distance).clamp(0.0, 1.0) as f32
}

fn capability_option(capability: &CapabilityKnowledge) -> ClarificationOption {
    ClarificationOption {
        label: capability
            .examples
            .first()
            .cloned()
            .unwrap_or_else(|| capability.id.clone()),
        capability: capability.id.clone(),
    }
}

fn unsupported_result(
    source: &str,
    candidates: Vec<RetrievedKnowledgeCandidate>,
) -> ClassificationResult {
    ClassificationResult {
        outcome: ClassificationOutcome::Unsupported,
        domain: None,
        capability: None,
        confidence: 0.0,
        params: json!({}),
        clarification: None,
        options: Vec::new(),
        source: Some(source.to_string()),
        candidates: candidates
            .into_iter()
            .map(|candidate| ClassificationCandidate {
                capability: candidate.source_id,
                confidence: vector_confidence(candidate.distance),
                source_type: Some(candidate.source_type),
            })
            .collect(),
    }
}

/// Append non-capability retrieval rows (data_area, domain, query) to the
/// classification's candidate list for audit/observability. They do not
/// influence the local-rule decision, but DeepSeek planner fallback can read
/// them off `chat_jobs.state_json.classification.candidates`.
fn attach_context_candidates(
    result: &mut ClassificationResult,
    context: &[RetrievedKnowledgeCandidate],
) {
    result
        .candidates
        .extend(context.iter().map(|candidate| ClassificationCandidate {
            capability: candidate.source_id.clone(),
            confidence: vector_confidence(candidate.distance),
            source_type: Some(candidate.source_type.clone()),
        }));
}

fn capability_retrieval_text(capability: &CapabilityKnowledge) -> String {
    [
        capability.id.clone(),
        capability.domain.clone(),
        capability.output_mode.clone(),
        capability.data_areas.join(" "),
        capability.metrics.join(" "),
        capability.examples.join(" "),
        capability.required_parameters.join(" "),
        capability.optional_parameters.join(" "),
    ]
    .join(" ")
    .to_lowercase()
}

fn lexical_confidence(message_tokens: &[String], text: &str) -> f32 {
    let matches = message_tokens
        .iter()
        .filter(|token| text.contains(token.as_str()))
        .count();
    (matches as f32 / message_tokens.len().min(6) as f32).min(0.95)
}

fn tokens(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.len() >= 3)
        .map(ToString::to_string)
        .collect()
}

fn is_write_intent(message: &str) -> bool {
    let normalized = message.to_lowercase();
    contains_any_local(&normalized, &["create", "open", "add", "new"])
        && contains_any_local(&normalized, &["account", "customer", "client"])
}

fn contains_any_local(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}
