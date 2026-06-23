use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::chat::model::ChatMessage;

#[derive(Clone)]
pub struct MessageRepository {
    pool: PgPool,
}

impl MessageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_for_client(
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

    pub async fn insert_assistant_response(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        content: String,
    ) -> Result<ChatMessage> {
        self.insert_assistant_message(
            session_id,
            job_id,
            content,
            json!({ "type": "report_response" }),
        )
        .await
    }

    pub async fn insert_assistant_message(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        content: String,
        metadata_json: serde_json::Value,
    ) -> Result<ChatMessage> {
        let id = Uuid::new_v4();
        let row = sqlx::query_as::<_, ChatMessageRow>(
            r#"
            INSERT INTO chat_messages (
                id,
                session_id,
                job_id,
                role,
                content,
                metadata_json
            )
            VALUES ($1, $2, $3, 'assistant', $4, $5)
            RETURNING id, session_id, job_id, role, content, metadata_json, created_at
            "#,
        )
        .bind(id)
        .bind(session_id)
        .bind(job_id)
        .bind(content)
        .bind(metadata_json)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }
}

// Visible inside the chat module so JobRepository's clarification-response
// transaction can RETURNING-map a chat_messages row without duplicating the type.
#[derive(Debug, FromRow)]
pub(in crate::chat) struct ChatMessageRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub job_id: Option<Uuid>,
    pub role: String,
    pub content: String,
    pub metadata_json: serde_json::Value,
    pub created_at: DateTime<Utc>,
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
