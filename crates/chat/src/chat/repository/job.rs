use anyhow::{Result, bail};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::chat::model::{ChatJob, ChatMessage, CreatedChatJob};
use crate::chat::repository::message::ChatMessageRow;
use crate::chat::repository::session::SessionRepository;

#[derive(Clone)]
pub struct JobRepository {
    pool: PgPool,
    sessions: SessionRepository,
}

impl JobRepository {
    pub fn new(pool: PgPool, sessions: SessionRepository) -> Self {
        Self { pool, sessions }
    }

    /// Create a job atomically with its user message. Creates the session
    /// first if the caller did not pass one.
    pub async fn create(
        &self,
        api_key_id: Uuid,
        session_id: Option<Uuid>,
        message: String,
        client_context_json: serde_json::Value,
        classification_json: serde_json::Value,
        execution_plan_json: serde_json::Value,
        policy_decision_json: serde_json::Value,
    ) -> Result<CreatedChatJob> {
        let session_id = match session_id {
            Some(session_id) => {
                if self
                    .sessions
                    .get_for_client(session_id, api_key_id)
                    .await?
                    .is_none()
                {
                    bail!("chat session not found for client");
                }
                session_id
            }
            None => self.sessions.create(api_key_id, None).await?.id,
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
                api_key_id,
                status,
                current_step,
                message,
                state_json,
                expires_at
            )
            VALUES ($1, $2, $3, 'queued', 'queued', $4, $5, $6)
            "#,
        )
        .bind(job_id)
        .bind(session_id)
        .bind(api_key_id)
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

        tx.commit().await?;

        Ok(CreatedChatJob {
            session_id,
            job_id,
            user_message_id,
            status: "queued".to_string(),
            current_step: "queued".to_string(),
        })
    }

    pub async fn get_for_client(&self, job_id: Uuid, api_key_id: Uuid) -> Result<Option<ChatJob>> {
        let row = sqlx::query_as::<_, ChatJobRow>(
            r#"
            SELECT
                id,
                session_id,
                api_key_id,
                user_message_id,
                status,
                current_step,
                resume_from_step,
                message,
                state_json,
                result_json,
                error_json,
                created_at,
                updated_at,
                expires_at,
                completed_at,
                failed_at,
                cancelled_at
            FROM chat_jobs
            WHERE id = $1
              AND api_key_id = $2
            "#,
        )
        .bind(job_id)
        .bind(api_key_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn wait_for_user_input(&self, job_id: Uuid) -> Result<()> {
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
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_plan_state(
        &self,
        job_id: Uuid,
        classification_json: serde_json::Value,
        execution_plan_json: serde_json::Value,
        policy_decision_json: serde_json::Value,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE chat_jobs
            SET
                state_json = jsonb_set(
                    jsonb_set(
                        jsonb_set(state_json, '{classification}', $1::jsonb),
                        '{execution_plan}',
                        $2::jsonb
                    ),
                    '{policy_decision}',
                    $3::jsonb
                ),
                updated_at = now()
            WHERE id = $4
            "#,
        )
        .bind(classification_json)
        .bind(execution_plan_json)
        .bind(policy_decision_json)
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Append a clarification response from the user, requeue the job, and
    /// write the matching checkpoint + status event in one transaction.
    pub async fn respond(
        &self,
        job_id: Uuid,
        api_key_id: Uuid,
        message: String,
    ) -> Result<Option<ChatMessage>> {
        let mut tx = self.pool.begin().await?;

        let Some(target) = sqlx::query_as::<_, JobResponseTargetRow>(
            r#"
            SELECT session_id, current_step
            FROM chat_jobs
            WHERE id = $1
              AND api_key_id = $2
              AND status = 'waiting_for_user_input'
            "#,
        )
        .bind(job_id)
        .bind(api_key_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(None);
        };

        let message_id = Uuid::new_v4();
        let checkpoint_id = Uuid::new_v4();
        let event_id = Uuid::new_v4();

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
        .bind(&message)
        .bind(json!({ "type": "clarification_response" }))
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
        }))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(Some(message_row.into()))
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

    pub async fn fail(&self, job_id: Uuid, error_json: serde_json::Value) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE chat_jobs
            SET
                status = 'failed',
                current_step = 'response',
                error_json = $1,
                failed_at = now(),
                updated_at = now()
            WHERE id = $2
            "#,
        )
        .bind(error_json)
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[derive(Debug, FromRow)]
struct JobResponseTargetRow {
    session_id: Uuid,
    current_step: String,
}

#[derive(Debug, FromRow)]
struct ChatJobRow {
    id: Uuid,
    session_id: Uuid,
    api_key_id: Uuid,
    user_message_id: Option<Uuid>,
    status: String,
    current_step: String,
    resume_from_step: Option<String>,
    message: String,
    state_json: serde_json::Value,
    result_json: Option<serde_json::Value>,
    error_json: Option<serde_json::Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    failed_at: Option<DateTime<Utc>>,
    cancelled_at: Option<DateTime<Utc>>,
}

impl From<ChatJobRow> for ChatJob {
    fn from(row: ChatJobRow) -> Self {
        Self {
            id: row.id,
            session_id: row.session_id,
            api_key_id: row.api_key_id,
            user_message_id: row.user_message_id,
            status: row.status,
            current_step: row.current_step,
            resume_from_step: row.resume_from_step,
            message: row.message,
            state_json: row.state_json,
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
