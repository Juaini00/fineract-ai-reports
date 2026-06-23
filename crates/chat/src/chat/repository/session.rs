use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::chat::model::ChatSession;

#[derive(Clone)]
pub struct SessionRepository {
    pool: PgPool,
}

impl SessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, api_key_id: Uuid, title: Option<String>) -> Result<ChatSession> {
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

    pub async fn get_for_client(
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
}

#[derive(Debug, FromRow)]
struct ChatSessionRow {
    id: Uuid,
    api_key_id: Uuid,
    title: Option<String>,
    status: String,
    context_json: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    archived_at: Option<DateTime<Utc>>,
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
