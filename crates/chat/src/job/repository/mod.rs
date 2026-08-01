pub mod assistant_memory;

pub use assistant_memory::{GraphCheckpoint, JobMemoryRepository};

use anyhow::{Result, bail};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::conversation::model::ChatMessage;
use crate::conversation::repository::SessionRepository;
use crate::conversation::repository::message::{ChatMessageRow, MessageRepository};
use crate::job::model::{
    ChatJob, ChatJobAuditEvent, ChatJobEvent, CreatedChatJob, ValidatedClarificationSubmission,
};
use crate::management::model::{
    AuditAggregateType, AuditEventType, AuditOutcome, AuditSummary, NormalizedErrorCode,
    PolicyResult, SafeIdentifier, SanitizedError,
};
use crate::management::{ManagementAuditEvent, enqueue};

#[derive(Debug)]
pub enum PersistResponseOutcome {
    Inserted(ChatMessage),
    NotFound,
    NotActive,
    Stale,
}

pub enum AssistantResponseTerminal {
    Completed { outcome: AuditOutcome },
    Failed { error_json: serde_json::Value },
}

/// Runtime execution facts extracted from `memory.execution_summary`, passed
/// alongside the terminal state so policy/execution audit events land in the
/// same transaction as the terminal-state write.
#[derive(Clone)]
pub struct ExecutionAuditContext {
    pub capability_id: SafeIdentifier,
    pub query_id: SafeIdentifier,
    pub row_count: Option<u64>,
    pub allowed: bool,
    /// True when the result was clamped by `hard_cap` / `global_max_rows` (B6).
    /// Drives the `execution.result_truncated` audit event.
    pub truncated: bool,
    /// True when the SQL execution hit its per-query statement timeout (B6).
    /// Drives the `execution.timed_out` audit event.
    pub timed_out: bool,
}

#[derive(Clone)]
pub struct JobRepository {
    pool: PgPool,
    sessions: SessionRepository,
    messages: MessageRepository,
}

impl JobRepository {
    pub fn new(pool: PgPool, sessions: SessionRepository, messages: MessageRepository) -> Self {
        Self {
            pool,
            sessions,
            messages,
        }
    }

    /// Create a job atomically with its user message. Creates the session
    /// first if the caller did not pass one.
    pub async fn create(
        &self,
        user_id: Uuid,
        session_id: Option<Uuid>,
        message: String,
        client_context_json: serde_json::Value,
        classification_json: serde_json::Value,
        execution_plan_json: serde_json::Value,
        policy_decision_json: serde_json::Value,
    ) -> Result<Option<CreatedChatJob>> {
        let session_id = match session_id {
            Some(session_id) => {
                if self
                    .sessions
                    .get_for_user(session_id, user_id, false)
                    .await?
                    .is_none()
                {
                    return Ok(None);
                }
                session_id
            }
            None => self.sessions.create(user_id, None).await?.id,
        };

        let user_message_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        let expires_at = Utc::now() + Duration::days(7);
        let state_json = json!({
            "client": client_context_json,
            "input": { "message": message },
            "classification": classification_json,
            "execution_plan": execution_plan_json,
            "policy_decision": policy_decision_json,
        });
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO chat_jobs (
                id,
                session_id,
                user_id,
                api_key_id,
                status,
                current_step,
                message,
                state_json,
                expires_at
            )
            VALUES ($1, $2, $3, NULL, 'queued', 'queued', $4, $5, $6)
            "#,
        )
        .bind(job_id)
        .bind(session_id)
        .bind(user_id)
        .bind(&message)
        .bind(state_json)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO chat_messages (
                id,
                session_id,
                job_id,
                role,
                content,
                metadata_json
            )
            VALUES ($1, $2, $3, 'user', $4, '{}'::jsonb)
            "#,
        )
        .bind(user_message_id)
        .bind(session_id)
        .bind(job_id)
        .bind(&message)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE chat_jobs
            SET user_message_id = $1
            WHERE id = $2
            "#,
        )
        .bind(user_message_id)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;

        enqueue(
            &mut tx,
            ManagementAuditEvent {
                aggregate_type: AuditAggregateType::ChatJob,
                aggregate_id: job_id,
                job_id: Some(job_id),
                session_id: Some(session_id),
                actor_user_id: Some(user_id),
                event_type: AuditEventType::ChatJobCreated,
                outcome: AuditOutcome::Success,
                summary: AuditSummary::JobCreated,
                sanitized_error: None,
                occurred_at: Utc::now(),
            },
        )
        .await?;
        tx.commit().await?;

        Ok(Some(CreatedChatJob {
            session_id,
            job_id,
            user_message_id,
            status: "queued".to_string(),
            current_step: "queued".to_string(),
        }))
    }

    pub async fn get_for_user(
        &self,
        job_id: Uuid,
        user_id: Uuid,
        include_legacy: bool,
    ) -> Result<Option<ChatJob>> {
        let row = sqlx::query_as::<_, ChatJobRow>(
            r#"
            SELECT
                cj.id,
                cj.session_id,
                cj.user_id,
                cj.api_key_id,
                cj.user_message_id,
                cj.status,
                cj.current_step,
                cj.resume_from_step,
                cj.message,
                cj.state_json,
                cj.state_revision,
                cj.result_json,
                cj.error_json,
                cj.created_at,
                cj.updated_at,
                cj.expires_at,
                cj.completed_at,
                cj.failed_at,
                cj.cancelled_at
            FROM chat_jobs cj
            JOIN chat_sessions cs ON cs.id = cj.session_id
            WHERE cj.id = $1
              AND (cj.user_id = $2 OR ($3 AND cj.user_id IS NULL))
              AND cs.archived_at IS NULL
            "#,
        )
        .bind(job_id)
        .bind(user_id)
        .bind(include_legacy)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn get_internal_for_user(
        &self,
        job_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ChatJob>> {
        let row = sqlx::query_as::<_, ChatJobRow>(
            r#"
            SELECT cj.id, cj.session_id, cj.user_id, cj.api_key_id, cj.user_message_id, cj.status,
                   cj.current_step, cj.resume_from_step, cj.message, cj.state_json, cj.state_revision,
                   cj.result_json, cj.error_json, cj.created_at, cj.updated_at, cj.expires_at,
                   cj.completed_at, cj.failed_at, cj.cancelled_at
            FROM chat_jobs cj
            JOIN chat_sessions cs ON cs.id = cj.session_id
            WHERE cj.id = $1 AND cj.user_id = $2 AND cs.archived_at IS NULL
            "#,
        )
        .bind(job_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn list_audit_events_for_user(
        &self,
        job_id: Uuid,
        user_id: Uuid,
        include_legacy: bool,
    ) -> Result<Option<Vec<ChatJobAuditEvent>>> {
        if self
            .get_for_user(job_id, user_id, include_legacy)
            .await?
            .is_none()
        {
            return Ok(None);
        }

        let rows = sqlx::query_as::<_, ChatJobAuditEventRow>(
            r#"
            SELECT
                id,
                job_id,
                session_id,
                user_id,
                api_key_id,
                event_type,
                stage,
                layer,
                blueprint_step,
                status,
                duration_ms,
                input_summary_json,
                output_summary_json,
                decision_json,
                flags_json,
                error_json,
                created_at
            FROM chat_job_audit_events
            WHERE job_id = $1
            ORDER BY created_at, id
            "#,
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(Some(rows.into_iter().map(Into::into).collect()))
    }

    /// Durable SSE event replay for a late/reconnecting subscriber. Sibling of
    /// `list_audit_events_for_user`, but reads `chat_job_events` (the small
    /// client-safe SSE log) rather than `chat_job_audit_events` (the full
    /// internal audit trail).
    pub async fn list_events_for_replay(
        &self,
        job_id: Uuid,
        user_id: Uuid,
        include_legacy: bool,
    ) -> Result<Option<Vec<ChatJobEvent>>> {
        if self
            .get_for_user(job_id, user_id, include_legacy)
            .await?
            .is_none()
        {
            return Ok(None);
        }

        let rows = sqlx::query_as::<_, ChatJobEventRow>(
            r#"
            SELECT event_type, step, payload_json, created_at
            FROM chat_job_events
            WHERE job_id = $1
            ORDER BY created_at, id
            "#,
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(Some(rows.into_iter().map(Into::into).collect()))
    }

    pub async fn wait_for_user_input_and_record_clarification_requested(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            UPDATE chat_jobs
            SET
                status = 'waiting_for_user_input',
                current_step = 'taking_decision',
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        enqueue(
            &mut tx,
            ManagementAuditEvent {
                aggregate_type: AuditAggregateType::ChatJob,
                aggregate_id: job_id,
                job_id: Some(job_id),
                session_id: Some(session_id),
                actor_user_id: Some(actor_user_id),
                event_type: AuditEventType::ChatClarificationRequested,
                outcome: AuditOutcome::Clarification,
                summary: AuditSummary::ClarificationRequested,
                sanitized_error: None,
                occurred_at: Utc::now(),
            },
        )
        .await?;
        tx.commit().await?;

        Ok(())
    }

    pub async fn update_plan_state(
        &self,
        job_id: Uuid,
        user_id: Uuid,
        expected_revision: i64,
        classification_json: serde_json::Value,
        execution_plan_json: serde_json::Value,
        policy_decision_json: serde_json::Value,
    ) -> Result<()> {
        let state = json!({
            "classification": classification_json,
            "execution_plan": execution_plan_json,
            "policy_decision": policy_decision_json,
        });
        let result = sqlx::query(
            r#"
            UPDATE chat_jobs
            SET
                state_json = state_json || $1::jsonb,
                state_revision = state_revision + 1,
                updated_at = now()
            WHERE id = $2
              AND user_id = $3
              AND state_revision = $4
            "#,
        )
        .bind(state)
        .bind(job_id)
        .bind(user_id)
        .bind(expected_revision)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            bail!("chat job state was updated by another request");
        }

        Ok(())
    }

    /// Best-effort merge of a per-request retrieval audit trace into
    /// `state_json.retrieval_trace`. Pure jsonb merge, no `state_revision`
    /// bump — this is audit-only and must not race the pipeline that owns
    /// the revision counter.
    pub async fn merge_retrieval_trace(
        &self,
        job_id: Uuid,
        user_id: Uuid,
        trace: serde_json::Value,
    ) -> Result<()> {
        let patch = json!({ "retrieval_trace": trace });
        sqlx::query(
            r#"
            UPDATE chat_jobs
            SET state_json = state_json || $1::jsonb,
                updated_at = now()
            WHERE id = $2
              AND user_id = $3
            "#,
        )
        .bind(patch)
        .bind(job_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Append a clarification response from the user, requeue the job, and
    /// write the matching checkpoint + status event in one transaction.
    pub async fn respond(
        &self,
        job_id: Uuid,
        user_id: Uuid,
        submission: ValidatedClarificationSubmission,
    ) -> Result<PersistResponseOutcome> {
        let mut tx = self.pool.begin().await?;

        let Some(target) = sqlx::query_as::<_, JobResponseTargetRow>(
            r#"
            SELECT cj.session_id, cj.current_step, cj.status
            FROM chat_jobs cj
            JOIN chat_sessions cs ON cs.id = cj.session_id
            WHERE cj.id = $1
              AND cj.user_id = $2
              AND cs.archived_at IS NULL
            FOR UPDATE OF cj, cs
            "#,
        )
        .bind(job_id)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(PersistResponseOutcome::NotFound);
        };
        if target.status != "waiting_for_user_input" {
            return Ok(PersistResponseOutcome::NotActive);
        }
        if let (Some(id), Some(revision)) = (
            submission.clarification_id,
            submission.clarification_revision,
        ) {
            let pending: Option<serde_json::Value> = sqlx::query_scalar(
                "SELECT pending_clarification_json FROM assistant_job_memory WHERE job_id = $1 FOR UPDATE",
            ).bind(job_id).fetch_optional(&mut *tx).await?.flatten();
            let matches = pending
                .and_then(|value| {
                    serde_json::from_value::<crate::assistant::ClarificationPayload>(value).ok()
                })
                .is_some_and(|payload| payload.id == id && payload.revision == revision);
            if !matches {
                return Ok(PersistResponseOutcome::Stale);
            }
        }

        let message_id = Uuid::new_v4();
        let checkpoint_id = Uuid::new_v4();
        let event_id = Uuid::new_v4();
        let metadata = json!({
            "type": "clarification_response",
            "clarification_id": submission.clarification_id,
            "clarification_revision": submission.clarification_revision,
            "selected_option_id": submission.selected_option_id,
            "answers": submission.answers,
            "constraint_patch": submission.constraint_patch,
            "source_message": submission.source_message,
        });

        let message_row = sqlx::query_as::<_, ChatMessageRow>(
            r#"
            INSERT INTO chat_messages (
                id,
                session_id,
                job_id,
                role,
                content,
                metadata_json
            )
            VALUES ($1, $2, $3, 'clarification', $4, $5)
            RETURNING id, session_id, job_id, role, content, metadata_json, created_at
            "#,
        )
        .bind(message_id)
        .bind(target.session_id)
        .bind(job_id)
        .bind(&submission.display_message)
        .bind(&metadata)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE chat_jobs
            SET
                status = 'queued',
                current_step = 'queued',
                resume_from_step = $1,
                updated_at = now()
            WHERE id = $2
            "#,
        )
        .bind(&target.current_step)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO chat_job_checkpoints (
                id,
                job_id,
                step,
                checkpoint_type,
                state_json
            )
            VALUES ($1, $2, $3, 'user_response_received', $4)
            "#,
        )
        .bind(checkpoint_id)
        .bind(job_id)
        .bind(&target.current_step)
        .bind(json!({
            "message_id": message_id,
            "resume_from_step": target.current_step,
            "selected_option_id": submission.selected_option_id.clone(),
        }))
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO chat_job_events (
                id,
                job_id,
                event_type,
                step,
                payload_json
            )
            VALUES ($1, $2, 'status', 'queued', $3)
            "#,
        )
        .bind(event_id)
        .bind(job_id)
        .bind(json!({
            "status": "queued",
            "current_step": "queued",
            "message_id": message_id,
            "selected_option_id": submission.selected_option_id.clone(),
        }))
        .execute(&mut *tx)
        .await?;

        enqueue(
            &mut tx,
            ManagementAuditEvent {
                aggregate_type: AuditAggregateType::ChatJob,
                aggregate_id: job_id,
                job_id: Some(job_id),
                session_id: Some(target.session_id),
                actor_user_id: Some(user_id),
                event_type: AuditEventType::ChatClarificationReceived,
                outcome: AuditOutcome::Clarification,
                summary: AuditSummary::ClarificationReceived,
                sanitized_error: None,
                occurred_at: Utc::now(),
            },
        )
        .await?;
        tx.commit().await?;

        Ok(PersistResponseOutcome::Inserted(message_row.into()))
    }

    pub async fn insert_checkpoint(
        &self,
        job_id: Uuid,
        step: &str,
        checkpoint_type: &str,
        state_json: serde_json::Value,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO chat_job_checkpoints (
                id,
                job_id,
                step,
                checkpoint_type,
                state_json
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(job_id)
        .bind(step)
        .bind(checkpoint_type)
        .bind(state_json)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn insert_event(
        &self,
        job_id: Uuid,
        event_type: &str,
        step: Option<&str>,
        payload_json: serde_json::Value,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO chat_job_events (
                id,
                job_id,
                event_type,
                step,
                payload_json
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(job_id)
        .bind(event_type)
        .bind(step)
        .bind(payload_json)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn complete(&self, job_id: Uuid, result_json: serde_json::Value) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE chat_jobs
            SET
                status = 'completed',
                current_step = 'response',
                result_json = $1,
                error_json = NULL,
                completed_at = now(),
                updated_at = now()
            WHERE id = $2
            "#,
        )
        .bind(result_json)
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn persist_assistant_response_and_terminal_state(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        actor_user_id: Uuid,
        content: String,
        metadata_json: serde_json::Value,
        result_json: serde_json::Value,
        terminal: AssistantResponseTerminal,
        execution: Option<ExecutionAuditContext>,
    ) -> Result<ChatMessage> {
        let mut tx = self.pool.begin().await?;
        let message = self
            .messages
            .insert_assistant_message_in_transaction(
                &mut tx,
                session_id,
                job_id,
                content,
                metadata_json,
            )
            .await?;
        let (event_type, outcome, summary, sanitized_error) = match terminal {
            AssistantResponseTerminal::Completed { outcome } => {
                sqlx::query(
                    "UPDATE chat_jobs SET status = 'completed', current_step = 'response', result_json = $1, error_json = NULL, completed_at = now(), updated_at = now() WHERE id = $2",
                )
                .bind(result_json)
                .bind(job_id)
                .execute(&mut *tx)
                .await?;
                (
                    AuditEventType::ChatJobCompleted,
                    outcome,
                    AuditSummary::JobCompleted,
                    None,
                )
            }
            AssistantResponseTerminal::Failed { error_json } => {
                sqlx::query(
                    "UPDATE chat_jobs SET status = 'failed', current_step = 'response', result_json = $1, error_json = $2, failed_at = now(), updated_at = now() WHERE id = $3",
                )
                .bind(result_json)
                .bind(error_json)
                .bind(job_id)
                .execute(&mut *tx)
                .await?;
                (
                    AuditEventType::ChatJobFailed,
                    AuditOutcome::Failed,
                    AuditSummary::JobFailed,
                    Some(SanitizedError {
                        code: NormalizedErrorCode::Unknown,
                    }),
                )
            }
        };
        enqueue(
            &mut tx,
            ManagementAuditEvent {
                aggregate_type: AuditAggregateType::ChatJob,
                aggregate_id: job_id,
                job_id: Some(job_id),
                session_id: Some(session_id),
                actor_user_id: Some(actor_user_id),
                event_type,
                outcome,
                summary,
                sanitized_error,
                occurred_at: Utc::now(),
            },
        )
        .await?;
        if let Some(execution) = execution {
            for event in execution_lifecycle_events(session_id, job_id, actor_user_id, &execution) {
                enqueue(&mut tx, event).await?;
            }
        }
        tx.commit().await?;
        Ok(message)
    }

    pub async fn store_assistant_response_result(
        &self,
        job_id: Uuid,
        result_json: serde_json::Value,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE chat_jobs
            SET
                result_json = $1,
                updated_at = now()
            WHERE id = $2
            "#,
        )
        .bind(result_json)
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Marks a job failed, but never overwrites a job that already reached a
    /// terminal status (`completed`/`failed`). A late failure to emit the
    /// terminal SSE event must not un-complete a job whose result is already
    /// durably persisted (I1) — returns `false` when the job was already
    /// terminal so the caller knows not to publish a misleading error event.
    pub async fn fail(&self, job_id: Uuid, error_json: serde_json::Value) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE chat_jobs
            SET
                status = 'failed',
                current_step = 'response',
                error_json = $1,
                failed_at = now(),
                updated_at = now()
            WHERE id = $2
              AND status NOT IN ('completed', 'failed')
            "#,
        )
        .bind(error_json)
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[derive(Debug, FromRow)]
struct JobResponseTargetRow {
    session_id: Uuid,
    current_step: String,
    status: String,
}

#[derive(Debug, FromRow)]
struct ChatJobRow {
    id: Uuid,
    session_id: Uuid,
    user_id: Option<Uuid>,
    api_key_id: Option<Uuid>,
    user_message_id: Option<Uuid>,
    status: String,
    current_step: String,
    resume_from_step: Option<String>,
    message: String,
    state_json: serde_json::Value,
    state_revision: i64,
    result_json: Option<serde_json::Value>,
    error_json: Option<serde_json::Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    failed_at: Option<DateTime<Utc>>,
    cancelled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct ChatJobAuditEventRow {
    id: Uuid,
    job_id: Uuid,
    session_id: Option<Uuid>,
    user_id: Option<Uuid>,
    api_key_id: Option<Uuid>,
    event_type: String,
    stage: String,
    layer: String,
    blueprint_step: Option<String>,
    status: String,
    duration_ms: Option<i64>,
    input_summary_json: serde_json::Value,
    output_summary_json: serde_json::Value,
    decision_json: serde_json::Value,
    flags_json: serde_json::Value,
    error_json: Option<serde_json::Value>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct ChatJobEventRow {
    event_type: String,
    step: Option<String>,
    payload_json: serde_json::Value,
    created_at: DateTime<Utc>,
}

impl From<ChatJobEventRow> for ChatJobEvent {
    fn from(row: ChatJobEventRow) -> Self {
        Self {
            event_type: row.event_type,
            step: row.step,
            payload_json: row.payload_json,
            created_at: row.created_at,
        }
    }
}

impl From<ChatJobAuditEventRow> for ChatJobAuditEvent {
    fn from(row: ChatJobAuditEventRow) -> Self {
        Self {
            id: row.id,
            job_id: row.job_id,
            session_id: row.session_id,
            user_id: row.user_id,
            api_key_id: row.api_key_id,
            event_type: row.event_type,
            stage: row.stage,
            layer: row.layer,
            blueprint_step: row.blueprint_step,
            status: row.status,
            duration_ms: row.duration_ms,
            input_summary_json: row.input_summary_json,
            output_summary_json: row.output_summary_json,
            decision_json: row.decision_json,
            flags_json: row.flags_json,
            error_json: row.error_json,
            created_at: row.created_at,
        }
    }
}

impl From<ChatJobRow> for ChatJob {
    fn from(row: ChatJobRow) -> Self {
        Self {
            id: row.id,
            session_id: row.session_id,
            user_id: row.user_id,
            api_key_id: row.api_key_id,
            user_message_id: row.user_message_id,
            status: row.status,
            current_step: row.current_step,
            resume_from_step: row.resume_from_step,
            message: row.message,
            state_json: row.state_json,
            state_revision: row.state_revision,
            result_json: row.result_json,
            error_json: row.error_json,
            created_at: row.created_at,
            updated_at: row.updated_at,
            expires_at: row.expires_at,
            completed_at: row.completed_at,
            failed_at: row.failed_at,
            cancelled_at: row.cancelled_at,
        }
    }
}

fn execution_lifecycle_events(
    session_id: Uuid,
    job_id: Uuid,
    actor_user_id: Uuid,
    execution: &ExecutionAuditContext,
) -> Vec<ManagementAuditEvent> {
    let now = Utc::now();
    let base = |event_type, outcome, summary| ManagementAuditEvent {
        aggregate_type: AuditAggregateType::ChatJob,
        aggregate_id: job_id,
        job_id: Some(job_id),
        session_id: Some(session_id),
        actor_user_id: Some(actor_user_id),
        event_type,
        outcome,
        summary,
        sanitized_error: None,
        occurred_at: now,
    };
    let policy_result = if execution.allowed {
        PolicyResult::Allowed
    } else {
        PolicyResult::Denied
    };
    let mut events = vec![base(
        AuditEventType::PolicyEvaluated,
        if execution.allowed {
            AuditOutcome::Success
        } else {
            AuditOutcome::Blocked
        },
        AuditSummary::PolicyEvaluated {
            capability_id: execution.capability_id.clone(),
            result: policy_result,
        },
    )];
    if execution.allowed {
        events.push(base(
            AuditEventType::ExecutionAuthorized,
            AuditOutcome::Success,
            AuditSummary::Execution {
                query_id: execution.query_id.clone(),
                row_count: None,
            },
        ));
        if !execution.timed_out {
            events.push(base(
                AuditEventType::ExecutionCompleted,
                AuditOutcome::Success,
                AuditSummary::Execution {
                    query_id: execution.query_id.clone(),
                    row_count: execution.row_count,
                },
            ));
        }
        if execution.truncated {
            events.push(base(
                AuditEventType::ExecutionResultTruncated,
                AuditOutcome::Success,
                AuditSummary::Execution {
                    query_id: execution.query_id.clone(),
                    row_count: execution.row_count,
                },
            ));
        }
        if execution.timed_out {
            events.push(base(
                AuditEventType::ExecutionTimedOut,
                AuditOutcome::Failed,
                AuditSummary::Execution {
                    query_id: execution.query_id.clone(),
                    row_count: None,
                },
            ));
        }
    } else {
        events.push(base(
            AuditEventType::ExecutionBlocked,
            AuditOutcome::Blocked,
            AuditSummary::Execution {
                query_id: execution.query_id.clone(),
                row_count: None,
            },
        ));
    }
    events
}
