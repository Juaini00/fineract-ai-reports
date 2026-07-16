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

    pub async fn create(&self, user_id: Uuid, title: Option<String>) -> Result<ChatSession> {
        let id = Uuid::new_v4();

        let row = sqlx::query_as::<_, ChatSessionRow>(
            r#"
            INSERT INTO chat_sessions (
                id,
                user_id,
                api_key_id,
                title,
                status,
                context_json
            )
            VALUES ($1, $2, NULL, $3, 'active', '{}'::jsonb)
            RETURNING
                id,
                user_id,
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
        .bind(user_id)
        .bind(title)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn list_for_user(
        &self,
        user_id: Uuid,
        include_legacy: bool,
    ) -> Result<Vec<ChatSession>> {
        let rows = sqlx::query_as::<_, ChatSessionRow>(
            r#"
            SELECT
                cs.id,
                cs.user_id,
                cs.api_key_id,
                cs.title,
                cs.status,
                cs.context_json,
                cs.created_at,
                cs.updated_at,
                cs.expires_at,
                cs.archived_at
            FROM chat_sessions cs
            WHERE (cs.user_id = $1 OR ($2 AND cs.user_id IS NULL))
              AND cs.archived_at IS NULL
            ORDER BY cs.updated_at DESC, cs.created_at DESC
            "#,
        )
        .bind(user_id)
        .bind(include_legacy)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get_for_user(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        include_legacy: bool,
    ) -> Result<Option<ChatSession>> {
        let row = sqlx::query_as::<_, ChatSessionRow>(
            r#"
            SELECT
                id,
                user_id,
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
              AND (user_id = $2 OR ($3 AND user_id IS NULL))
              AND archived_at IS NULL
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .bind(include_legacy)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }
}

#[derive(Debug, FromRow)]
struct ChatSessionRow {
    id: Uuid,
    user_id: Option<Uuid>,
    api_key_id: Option<Uuid>,
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
            user_id: row.user_id,
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
