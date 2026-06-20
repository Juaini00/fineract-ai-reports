use anyhow::{Result, bail};
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::model::{ChatJob, ChatMessage, ChatSession, CreatedChatJob};

#[derive(Clone)]
pub struct ChatRepository {
    pool: PgPool,
}

impl ChatRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_session(
        &self,
        api_key_id: Uuid,
        title: Option<String>,
    ) -> Result<ChatSession> {
        let id = Uuid::new_v4();

        let row = sqlx::query_as::<_, ChatSessionRow>(
            r#"
            INSERT INTO chat_sessions (
                id,
                api_key_id,
                title,
                status,
                context_json
            )
            VALUES ($1, $2, $3, 'active', '{}'::jsonb)
            RETURNING
                id,
                api_key_id,
                title,
                status,
                context_json,
                created_at,
                updated_at,
                expires_at,
                archived_at
            "#,
        )
        .bind(id)
        .bind(api_key_id)
        .bind(title)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn get_session_for_client(
        &self,
        session_id: Uuid,
        api_key_id: Uuid,
    ) -> Result<Option<ChatSession>> {
        let row = sqlx::query_as::<_, ChatSessionRow>(
            r#"
            SELECT
                id,
                api_key_id,
                title,
                status,
                context_json,
                created_at,
                updated_at,
                expires_at,
                archived_at
            FROM chat_sessions
            WHERE id = $1
              AND api_key_id = $2
              AND archived_at IS NULL
            "#,
        )
        .bind(session_id)
        .bind(api_key_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn list_messages_for_client(
        &self,
        session_id: Uuid,
        api_key_id: Uuid,
    ) -> Result<Vec<ChatMessage>> {
        let rows = sqlx::query_as::<_, ChatMessageRow>(
            r#"
            SELECT
                m.id,
                m.session_id,
                m.job_id,
                m.role,
                m.content,
                m.metadata_json,
                m.created_at
            FROM chat_messages m
            JOIN chat_sessions s ON s.id = m.session_id
            WHERE m.session_id = $1
              AND s.api_key_id = $2
              AND s.archived_at IS NULL
            ORDER BY m.created_at ASC
            "#,
        )
        .bind(session_id)
        .bind(api_key_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn create_job(
        &self,
        api_key_id: Uuid,
        session_id: Option<Uuid>,
        message: String,
        client_context_json: serde_json::Value,
    ) -> Result<CreatedChatJob> {
        let session_id = match session_id {
            Some(session_id) => {
                if self
                    .get_session_for_client(session_id, api_key_id)
                    .await?
                    .is_none()
                {
                    bail!("chat session not found for client");
                }

                session_id
            }
            None => {
                let session = self.create_session(api_key_id, None).await?;
                session.id
            }
        };

        let user_message_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        let expires_at = Utc::now() + Duration::days(7);
        let state_json = json!({
            "client": client_context_json,
            "input": {
                "message": message,
            },
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
            VALUES (
                $1,
                $2,
                $3,
                'queued',
                'queued',
                $4,
                $5,
                $6
            )
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

    pub async fn get_job_for_client(
        &self,
        job_id: Uuid,
        api_key_id: Uuid,
    ) -> Result<Option<ChatJob>> {
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
}

#[derive(Debug, FromRow)]
struct ChatSessionRow {
    id: Uuid,
    api_key_id: Uuid,
    title: Option<String>,
    status: String,
    context_json: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<ChatSessionRow> for ChatSession {
    fn from(row: ChatSessionRow) -> Self {
        Self {
            id: row.id,
            api_key_id: row.api_key_id,
            title: row.title,
            status: row.status,
            context_json: row.context_json,
            created_at: row.created_at,
            updated_at: row.updated_at,
            expires_at: row.expires_at,
            archived_at: row.archived_at,
        }
    }
}

#[derive(Debug, FromRow)]
struct ChatMessageRow {
    id: Uuid,
    session_id: Uuid,
    job_id: Option<Uuid>,
    role: String,
    content: String,
    metadata_json: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<ChatMessageRow> for ChatMessage {
    fn from(row: ChatMessageRow) -> Self {
        Self {
            id: row.id,
            session_id: row.session_id,
            job_id: row.job_id,
            role: row.role,
            content: row.content,
            metadata_json: row.metadata_json,
            created_at: row.created_at,
        }
    }
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
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    failed_at: Option<chrono::DateTime<chrono::Utc>>,
    cancelled_at: Option<chrono::DateTime<chrono::Utc>>,
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
