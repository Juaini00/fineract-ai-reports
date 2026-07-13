use anyhow::Result;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::{clarification::ClarificationPayload, memory::SessionMemory};

#[derive(Clone)]
pub struct SessionMemoryRepository {
    pool: PgPool,
}

impl SessionMemoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_or_create(&self, session_id: Uuid) -> Result<SessionMemory> {
        let row = sqlx::query_as::<_, SessionMemoryRow>(
            r#"
            INSERT INTO assistant_session_memory (session_id)
            VALUES ($1)
            ON CONFLICT (session_id) DO UPDATE SET updated_at = assistant_session_memory.updated_at
            RETURNING session_id, summary, active_domain, pending_clarification_json, entities_json, revision
            "#,
        )
        .bind(session_id)
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
                entities_json = $4, revision = revision + 1, updated_at = now()
            WHERE session_id = $5 AND revision = $6
            RETURNING session_id, summary, active_domain, pending_clarification_json, entities_json, revision
            "#,
        )
        .bind(&memory.summary)
        .bind(&memory.active_domain)
        .bind(memory.pending_clarification.as_ref().map(serde_json::to_value).transpose()?)
        .bind(&memory.entities)
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
            RETURNING session_id, summary, active_domain, pending_clarification_json, entities_json, revision
            "#,
        )
        .bind(pending.map(serde_json::to_value).transpose()?)
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }
}

#[derive(FromRow)]
struct SessionMemoryRow {
    session_id: Uuid,
    summary: Option<String>,
    active_domain: Option<String>,
    pending_clarification_json: Option<serde_json::Value>,
    entities_json: serde_json::Value,
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
            entities: row.entities_json,
            revision: row.revision,
        }
    }
}
