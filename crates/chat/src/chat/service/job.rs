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
    OTHER_ACTIVITY_CAPABILITY, clarify_retrieved_capabilities, classify_clarification_response,
    classify_retrieved_capability,
};
use crate::chat::executor::execute_plan;
use crate::chat::formatter::format_report_response;
use crate::chat::llm::{LlmPlannerClient, LlmPlannerDecision};
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
    llm_planner: LlmPlannerClient,
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
        llm_planner: LlmPlannerClient,
        redis: Option<redis::Client>,
    ) -> Self {
        Self {
            jobs,
            messages,
            fineract_pool,
            catalog,
            knowledge: KnowledgeRepository::new(app_pool),
            embedding_client,
            llm_planner,
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
        let policy_decision =
            evaluate_policy(&input.client, execution_plan.as_ref(), &self.catalog);
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

        if let Some(result) = self.classify_savings_activity_list(message, today, client) {
            return result;
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
                    let top_capability_conf = candidates
                        .first()
                        .map(|candidate| vector_confidence(candidate.distance))
                        .unwrap_or(0.0);
                    let mut result = self
                        .classify_from_candidates(
                            message,
                            today,
                            &client.allowed_capabilities,
                            &candidates,
                        )
                        .unwrap_or_else(|| {
                            unsupported_result("vector_no_match", candidates.clone())
                        });
                    if result.outcome != ClassificationOutcome::Unsupported
                        && self.context_overrides_capability(message, &context, top_capability_conf)
                    {
                        result = unsupported_result("off_domain_match", candidates);
                    }
                    attach_context_candidates(&mut result, &context);
                    result = self
                        .llm_clarification_fallback(message, today, result)
                        .await;
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
        let result = self
            .classify_from_candidates(message, today, &client.allowed_capabilities, &candidates)
            .unwrap_or_else(|| unsupported_result("catalog_no_match", candidates));
        self.llm_clarification_fallback(message, today, result)
            .await
    }

    fn classify_savings_activity_list(
        &self,
        message: &str,
        today: chrono::NaiveDate,
        client: &ClientContext,
    ) -> Option<ClassificationResult> {
        if !is_savings_activity_request(message) {
            return None;
        }
        let capability = self.catalog_capability("savings_activity_list")?;
        if !client
            .allowed_capabilities
            .iter()
            .any(|allowed| allowed == &capability.id)
        {
            return None;
        }

        Some(classify_retrieved_capability(
            message,
            today,
            &capability.domain,
            &capability.id,
            &capability.output_mode,
            0.95,
            vec![ClassificationCandidate {
                capability: capability.id.clone(),
                confidence: 0.95,
                source_type: Some("deterministic".to_string()),
            }],
        ))
    }

    async fn llm_clarification_fallback(
        &self,
        message: &str,
        today: chrono::NaiveDate,
        result: ClassificationResult,
    ) -> ClassificationResult {
        if result.outcome != ClassificationOutcome::ClarificationRequired
            || result.options.is_empty()
            || !self.llm_planner.is_enabled()
        {
            return result;
        }

        match self
            .llm_planner
            .choose_capability(message, &result.options)
            .await
        {
            Ok(LlmPlannerDecision::Capability(capability_id)) => {
                let Some(capability) = self.catalog_capability(&capability_id) else {
                    return result;
                };
                let mut classification = classify_retrieved_capability(
                    message,
                    today,
                    &capability.domain,
                    &capability.id,
                    &capability.output_mode,
                    0.74,
                    result.candidates.clone(),
                );
                classification.source = Some("llm_planner".to_string());
                classification
            }
            Ok(LlmPlannerDecision::Clarify(question)) => ClassificationResult {
                clarification: Some(question),
                source: Some("llm_planner".to_string()),
                ..result
            },
            Ok(LlmPlannerDecision::Unsupported) => ClassificationResult {
                outcome: ClassificationOutcome::Unsupported,
                clarification: None,
                options: Vec::new(),
                source: Some("llm_planner".to_string()),
                ..result
            },
            Err(error) => {
                warn!(error = %error, "LLM planner fallback failed; keeping deterministic classification");
                result
            }
        }
    }

    /// Returns true when the top context candidate (a non-capability row from
    /// `search_context`) wins decisively over the top capability candidate AND
    /// its source is a deferred/rejected area or domain. This is the signal that
    /// the user asked about a topic outside the API key's reporting surface, and
    /// the savings capability that scored mid-confidence is the wrong answer.
    /// ponytail: simple two-number compare. Upgrade to per-source-type weights
    /// only if false positives appear in production.
    fn context_overrides_capability(
        &self,
        message: &str,
        context: &[RetrievedKnowledgeCandidate],
        top_capability_confidence: f32,
    ) -> bool {
        let Some((top, top_conf)) = context
            .iter()
            .filter(|candidate| {
                is_deferred_context_source(
                    &self.catalog,
                    &candidate.source_type,
                    &candidate.source_id,
                )
            })
            .map(|candidate| (candidate, vector_confidence(candidate.distance)))
            .max_by(|left, right| left.1.total_cmp(&right.1))
        else {
            return false;
        };

        if top_conf >= 0.50 && top_conf > top_capability_confidence + 0.10 {
            return true;
        }

        top_conf >= 0.38 && has_off_domain_cue(message, &top.source_id)
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
        allowed_capabilities: &[String],
        candidates: &[RetrievedKnowledgeCandidate],
    ) -> Option<ClassificationResult> {
        use crate::chat::classifier::{DecideOutcome, append_others_option, decide_from_scores};

        let top = candidates.first()?;
        let top_capability = self.catalog_capability_for_candidate(top)?;

        let classification_candidates = candidates
            .iter()
            .filter_map(|candidate| {
                self.catalog_capability_for_candidate(candidate)
                    .map(|capability| (candidate, capability))
            })
            .map(|(candidate, capability)| ClassificationCandidate {
                capability: capability.id.clone(),
                confidence: vector_confidence(candidate.distance),
                source_type: Some(candidate.source_type.clone()),
            })
            .collect::<Vec<_>>();

        let sorted_scores: Vec<f32> = classification_candidates
            .iter()
            .map(|c| c.confidence)
            .collect();
        let sorted_ids: Vec<&str> = classification_candidates
            .iter()
            .map(|c| c.capability.as_str())
            .collect();

        let policy = &self.catalog.classification;
        match decide_from_scores(policy, &sorted_scores, &sorted_ids) {
            DecideOutcome::Unsupported => None,
            DecideOutcome::Match { capability } => {
                let capability = self.catalog_capability(&capability)?;
                let confidence = sorted_scores.first().copied().unwrap_or(0.0);
                Some(classify_retrieved_capability(
                    message,
                    today,
                    &capability.domain,
                    &capability.id,
                    &capability.output_mode,
                    confidence,
                    classification_candidates,
                ))
            }
            DecideOutcome::Clarify => {
                let close_capabilities = candidates
                    .iter()
                    .filter_map(|candidate| self.catalog_capability_for_candidate(candidate))
                    .collect::<Vec<_>>();
                let mut options = if is_activity_request(message) {
                    self.activity_options(message, &top_capability.domain, allowed_capabilities)
                } else {
                    close_capabilities
                        .into_iter()
                        .map(|capability| capability_option(capability, message))
                        .collect::<Vec<_>>()
                };
                options = append_others_option(options, &policy.others_label);
                let confidence = sorted_scores.first().copied().unwrap_or(0.0);
                Some(clarify_retrieved_capabilities(
                    message,
                    today,
                    Some(top_capability.domain.clone()),
                    options,
                    confidence,
                    classification_candidates,
                ))
            }
        }
    }

    fn catalog_capability(&self, capability_id: &str) -> Option<&CapabilityKnowledge> {
        self.catalog.capabilities.iter().find(|capability| {
            capability.id == capability_id && capability.status == "approved_mvp"
        })
    }

    fn catalog_capability_for_candidate(
        &self,
        candidate: &RetrievedKnowledgeCandidate,
    ) -> Option<&CapabilityKnowledge> {
        if candidate.source_type == "capability" {
            return self.catalog_capability(&candidate.source_id);
        }

        if candidate.source_type == "query" {
            return self.catalog.capabilities.iter().find(|capability| {
                capability.query_id == candidate.source_id && capability.status == "approved_mvp"
            });
        }

        None
    }

    fn activity_options(
        &self,
        message: &str,
        domain: &str,
        allowed_capabilities: &[String],
    ) -> Vec<ClarificationOption> {
        let output_modes = if contains_any_local(
            &message.to_lowercase(),
            &["monthly", "per month", "by month", "breakdown"],
        ) {
            ["monthly_breakdown", "monthly_top_n"].as_slice()
        } else {
            ["list", "total", "top_n"].as_slice()
        };

        let mut options = self
            .catalog
            .capabilities
            .iter()
            .filter(|capability| {
                capability.status == "approved_mvp"
                    && capability.domain == domain
                    && output_modes.contains(&capability.output_mode.as_str())
                    && allowed_capabilities.iter().any(|id| id == &capability.id)
            })
            .map(|capability| capability_option(capability, message))
            .collect::<Vec<_>>();

        options.push(other_activity_option(message));
        options
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
            let mut classification = self
                .classify_savings_activity_list(
                    &response.content,
                    Utc::now().date_naive(),
                    &input.client,
                )
                .unwrap_or_else(|| classify_clarification_response(&original, &response.content));

            // Semantic principle: the user's reply is a natural-language
            // statement of intent, not an ID lookup. If they didn't pick a
            // listed option (numeric, label, or capability id) — regardless of
            // whether an Others option was offered or not — treat the reply as
            // a fresh prompt and run the full semantic retrieval pipeline over
            // it. Detection is by structured source token, not by matching the
            // human-facing clarification text.
            let source = classification.source.as_deref().unwrap_or("");
            let picked_a_listed_option = matches!(
                source,
                "clarification_option" | "clarification_other_selected"
            );
            let is_clarification =
                classification.outcome == ClassificationOutcome::ClarificationRequired;
            if is_clarification && !picked_a_listed_option {
                classification = self
                    .classify_with_retrieval(&response.content, &input.client)
                    .await;
            }
            let execution_plan = build_execution_plan(&classification, &self.catalog);
            let policy_decision =
                evaluate_policy(&input.client, execution_plan.as_ref(), &self.catalog);

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
                    .run_pipeline(
                        session_id,
                        job_id,
                        classification,
                        execution_plan,
                        policy_decision,
                    )
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

                if let Some(content) =
                    format_report_response(&self.catalog, plan, policy_decision, &result)
                {
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

fn is_deferred_context_source(
    catalog: &KnowledgeCatalog,
    source_type: &str,
    source_id: &str,
) -> bool {
    match source_type {
        "domain" => catalog
            .domains
            .iter()
            .find(|domain| domain.id == source_id)
            .is_some_and(|domain| is_non_executable_status(&domain.status)),
        "data_area" => catalog
            .data_areas
            .iter()
            .find(|area| area.id == source_id)
            .is_some_and(|area| is_non_executable_status(&area.status)),
        _ => false,
    }
}

/// Statuses where the catalog explicitly declares the area / domain is NOT
/// currently executable: deferred (work pending), rejected (won't do),
/// out_of_scope (hard reject), or candidate (documented but no approved MVP
/// capability yet — per knowledge-catalog.md §2.5 + group_center default rule).
fn is_non_executable_status(status: &str) -> bool {
    matches!(
        status,
        "deferred"
            | "deferred_group"
            | "rejected"
            | "rejected_group"
            | "out_of_scope"
            | "candidate"
    )
}

fn has_off_domain_cue(message: &str, source_id: &str) -> bool {
    let message = message.to_lowercase();
    let source_id = source_id.to_lowercase();

    (source_id.contains("loan") && message.contains("loan"))
        || (source_id.contains("accounting")
            && contains_any_local(&message, &["accounting", "journal", "ledger", "gl"]))
        || (source_id.contains("tax") && message.contains("tax"))
        || (source_id.contains("group")
            && contains_any_local(&message, &["group", "groups", "center", "centers"]))
}

fn vector_confidence(distance: f64) -> f32 {
    (1.0 - distance).clamp(0.0, 1.0) as f32
}

fn is_activity_request(message: &str) -> bool {
    contains_any_local(
        &message.to_lowercase(),
        &["activity", "activities", "transaction", "transactions"],
    )
}

fn is_savings_activity_request(message: &str) -> bool {
    let message = message.to_lowercase();
    contains_any_local(&message, &["saving", "savings"])
        && contains_any_local(&message, &["transaction", "transactions"])
}

fn capability_option(capability: &CapabilityKnowledge, message: &str) -> ClarificationOption {
    ClarificationOption {
        label: capability_option_label(capability, message),
        capability: capability.id.clone(),
        output_mode: Some(capability.output_mode.clone()),
    }
}

fn other_activity_option(message: &str) -> ClarificationOption {
    ClarificationOption {
        label: format!("Other activity{}", period_label(message)),
        capability: OTHER_ACTIVITY_CAPABILITY.to_string(),
        output_mode: None,
    }
}

fn capability_option_label(capability: &CapabilityKnowledge, message: &str) -> String {
    let format = match capability.output_mode.as_str() {
        "total" => "Total",
        "top_n" => "Largest",
        "list" => "List",
        "monthly_breakdown" => "Monthly total",
        "monthly_top_n" => "Monthly largest",
        _ => return capability.id.clone(),
    };
    let subject = capability_subject(capability);
    let period = period_label(message);

    format!("{format} {subject}{period}")
}

fn capability_subject(capability: &CapabilityKnowledge) -> String {
    let without_domain = capability
        .id
        .strip_prefix(&format!("{}_", capability.domain))
        .unwrap_or(&capability.id);
    let without_mode = without_domain
        .strip_suffix("_monthly_breakdown")
        .or_else(|| without_domain.strip_suffix("_monthly_top_n"))
        .or_else(|| without_domain.strip_suffix("_top_n"))
        .or_else(|| without_domain.strip_suffix("_total"))
        .or_else(|| without_domain.strip_suffix("_list"))
        .unwrap_or(without_domain);

    without_mode.replace('_', " ")
}

fn period_label(message: &str) -> &'static str {
    let message = message.to_lowercase();
    if contains_any_local(&message, &["today"]) {
        " today"
    } else if contains_any_local(&message, &["this week", "minggu ini"]) {
        " this week"
    } else if contains_any_local(&message, &["last week", "minggu lalu"]) {
        " last week"
    } else if contains_any_local(&message, &["this month", "bulan ini"]) {
        " this month"
    } else if contains_any_local(&message, &["last month", "bulan lalu", "bulan kemarin"]) {
        " last month"
    } else if contains_any_local(
        &message,
        &["this year", "year to date", "year-to-date", "ytd"],
    ) {
        " this year"
    } else {
        " for the requested period"
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
/// influence the local-rule decision, but LLM planner fallback can read
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

#[cfg(test)]
mod tests;
