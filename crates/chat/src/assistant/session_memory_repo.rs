use anyhow::Result;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::{RelevantJobSummary, clarification::ClarificationPayload, memory::SessionMemory};

#[derive(Clone)]
pub struct SessionMemoryRepository {
    pool: PgPool,
}

impl SessionMemoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_or_create(&self, session_id: Uuid, user_id: Uuid) -> Result<SessionMemory> {
        let row = sqlx::query_as::<_, SessionMemoryRow>(
            r#"
            INSERT INTO assistant_session_memory (session_id)
            SELECT id FROM chat_sessions WHERE id = $1 AND user_id = $2
            ON CONFLICT (session_id) DO UPDATE SET updated_at = assistant_session_memory.updated_at
            RETURNING session_id, summary, active_domain, pending_clarification_json,
                pending_clarification_source_intent_json, entities_json, relevant_jobs_json,
                context_warnings_json, revision
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    pub async fn save(
        &self,
        memory: &SessionMemory,
        expected_revision: i64,
    ) -> Result<SessionMemory> {
        let row = sqlx::query_as::<_, SessionMemoryRow>(
            r#"
            UPDATE assistant_session_memory
            SET summary = $1, active_domain = $2, pending_clarification_json = $3,
                pending_clarification_source_intent_json = $4, entities_json = $5,
                relevant_jobs_json = $6, context_warnings_json = $7,
                revision = revision + 1, updated_at = now()
            WHERE session_id = $8 AND revision = $9
            RETURNING session_id, summary, active_domain, pending_clarification_json,
                pending_clarification_source_intent_json, entities_json, relevant_jobs_json,
                context_warnings_json, revision
            "#,
        )
        .bind(&memory.summary)
        .bind(&memory.active_domain)
        .bind(
            memory
                .pending_clarification
                .as_ref()
                .map(serde_json::to_value)
                .transpose()?,
        )
        .bind(&memory.pending_clarification_source_intent)
        .bind(&memory.entities)
        .bind(&memory.relevant_jobs)
        .bind(&memory.context_warnings)
        .bind(memory.session_id)
        .bind(expected_revision)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Into::into).ok_or_else(|| {
            anyhow::anyhow!("assistant session memory was updated by another request")
        })
    }

    pub async fn set_pending_clarification(
        &self,
        session_id: Uuid,
        pending: Option<&ClarificationPayload>,
    ) -> Result<SessionMemory> {
        let row = sqlx::query_as::<_, SessionMemoryRow>(
            r#"
            UPDATE assistant_session_memory
            SET pending_clarification_json = $1, revision = revision + 1, updated_at = now()
            WHERE session_id = $2
            RETURNING session_id, summary, active_domain, pending_clarification_json,
                pending_clarification_source_intent_json, entities_json, relevant_jobs_json,
                context_warnings_json, revision
            "#,
        )
        .bind(pending.map(serde_json::to_value).transpose()?)
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    pub async fn set_pending_clarification_with_source_intent(
        &self,
        session_id: Uuid,
        pending: Option<&ClarificationPayload>,
        source_intent: Option<&serde_json::Value>,
    ) -> Result<SessionMemory> {
        let row = sqlx::query_as::<_, SessionMemoryRow>(
            r#"
            UPDATE assistant_session_memory
            SET pending_clarification_json = $1, pending_clarification_source_intent_json = $2,
                revision = revision + 1, updated_at = now()
            WHERE session_id = $3
            RETURNING session_id, summary, active_domain, pending_clarification_json,
                pending_clarification_source_intent_json, entities_json, relevant_jobs_json,
                context_warnings_json, revision
            "#,
        )
        .bind(pending.map(serde_json::to_value).transpose()?)
        .bind(source_intent)
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    pub async fn recent_completed_job_summaries(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<RelevantJobSummary>> {
        let rows = sqlx::query_as::<_, CompletedJobMemoryRow>(
            r#"
            SELECT m.job_id, m.intent_json, m.retrieval_plan_json, m.evidence_decision_json,
                   m.selected_capability, m.execution_summary_json, j.created_at
            FROM assistant_job_memory m
            JOIN chat_jobs j ON j.id = m.job_id
            WHERE j.session_id = $1 AND j.user_id = $2 AND m.terminal_state = 'completed'
            ORDER BY j.created_at DESC
            LIMIT $3
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn update_after_job(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        job: &super::JobMemory,
        pending: Option<Option<&ClarificationPayload>>,
    ) -> Result<SessionMemory> {
        let mut memory = self.get_or_create(session_id, user_id).await?;
        memory.active_domain = job
            .intent
            .as_ref()
            .map(|intent| format!("{:?}", intent.domain).to_lowercase());
        memory.entities = job
            .intent
            .as_ref()
            .map(|intent| serde_json::to_value(&intent.entities))
            .transpose()?
            .unwrap_or(memory.entities);
        memory.context_warnings = job.warnings.clone();
        if matches!(job.terminal_state, Some(super::TerminalState::Completed)) {
            let mut jobs: Vec<RelevantJobSummary> =
                serde_json::from_value(memory.relevant_jobs.clone()).unwrap_or_default();
            jobs.insert(
                0,
                RelevantJobSummary {
                    job_id: job.job_id.to_string(),
                    session_id: Some(session_id.to_string()),
                    domain: memory.active_domain.clone(),
                    intent: job
                        .intent
                        .as_ref()
                        .map(|intent| format!("{:?}", intent.intent).to_lowercase()),
                    created_at: None,
                    summary: job
                        .selected_capability
                        .clone()
                        .or_else(|| job.intent.as_ref().map(|intent| intent.reason.clone()))
                        .unwrap_or_else(|| "completed assistant job".into()),
                    retrieval_plan: job.retrieval_plan.clone(),
                    evidence_decision: job.evidence_decision.clone(),
                    evidence_refs: job.selected_capability.clone().into_iter().collect(),
                },
            );
            jobs.truncate(10);
            memory.relevant_jobs = serde_json::to_value(jobs)?;
        }
        if let Some(pending) = pending {
            memory.pending_clarification = pending.cloned();
            memory.pending_clarification_source_intent = memory
                .pending_clarification
                .as_ref()
                .and_then(|p| p.source_intent.as_ref())
                .map(serde_json::to_value)
                .transpose()?;
        }
        let revision = memory.revision;
        self.save(&memory, revision).await
    }
}

#[derive(FromRow)]
struct CompletedJobMemoryRow {
    job_id: Uuid,
    intent_json: Option<serde_json::Value>,
    retrieval_plan_json: serde_json::Value,
    evidence_decision_json: serde_json::Value,
    selected_capability: Option<String>,
    execution_summary_json: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<CompletedJobMemoryRow> for RelevantJobSummary {
    fn from(row: CompletedJobMemoryRow) -> Self {
        Self {
            job_id: row.job_id.to_string(),
            session_id: None,
            domain: row
                .intent_json
                .as_ref()
                .and_then(|v| v.get("domain"))
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
            intent: row
                .intent_json
                .as_ref()
                .and_then(|v| v.get("intent"))
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned),
            created_at: Some(row.created_at.to_rfc3339()),
            summary: row.selected_capability.unwrap_or_else(|| {
                row.execution_summary_json
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("completed assistant job")
                    .to_string()
            }),
            retrieval_plan: row.retrieval_plan_json,
            evidence_decision: row.evidence_decision_json,
            evidence_refs: Vec::new(),
        }
    }
}

#[derive(FromRow)]
struct SessionMemoryRow {
    session_id: Uuid,
    summary: Option<String>,
    active_domain: Option<String>,
    pending_clarification_json: Option<serde_json::Value>,
    pending_clarification_source_intent_json: Option<serde_json::Value>,
    entities_json: serde_json::Value,
    relevant_jobs_json: serde_json::Value,
    context_warnings_json: serde_json::Value,
    revision: i64,
}

impl From<SessionMemoryRow> for SessionMemory {
    fn from(row: SessionMemoryRow) -> Self {
        Self {
            session_id: row.session_id,
            summary: row.summary,
            active_domain: row.active_domain,
            pending_clarification: row
                .pending_clarification_json
                .and_then(|value| serde_json::from_value(value).ok()),
            pending_clarification_source_intent: row.pending_clarification_source_intent_json,
            pending: None,
            entities: row.entities_json,
            relevant_jobs: row.relevant_jobs_json,
            context_warnings: row.context_warnings_json,
            revision: row.revision,
        }
    }
}
