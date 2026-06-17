use anyhow::Result;
use serde_json::json;
use sqlx::{FromRow, PgPool, types::Json};
use uuid::Uuid;

use crate::auth::model::{ActiveApiKeyRecord, NewApiKeyRecord};

#[derive(Debug, FromRow)]
struct ApiKeyRow {
    id: Uuid,
    name: String,
    owner: String,
    key_prefix: String,
    allowed_office_ids: Json<Vec<i64>>,
    allowed_capabilities: Json<Vec<String>>,
    can_view_pii: bool,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone)]
pub struct ApiKeyRepository {
    pool: PgPool,
}

impl ApiKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, record: NewApiKeyRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO api_keys (
                id,
                name,
                owner,
                key_prefix,
                key_hash,
                allowed_office_ids,
                allowed_capabilities,
                can_view_pii,
                expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(record.id)
        .bind(record.name)
        .bind(record.owner)
        .bind(record.key_prefix)
        .bind(record.key_hash)
        .bind(Json(json!(record.allowed_office_ids)))
        .bind(Json(json!(record.allowed_capabilities)))
        .bind(record.can_view_pii)
        .bind(record.expires_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn find_active_by_hash(&self, key_hash: &str) -> Result<Option<ActiveApiKeyRecord>> {
        let row = sqlx::query_as::<_, ApiKeyRow>(
            r#"
            SELECT
                id,
                name,
                owner,
                key_prefix,
                allowed_office_ids,
                allowed_capabilities,
                can_view_pii,
                expires_at
            FROM api_keys
            WHERE key_hash = $1
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > now())
            "#,
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(ActiveApiKeyRecord::from))
    }

    pub async fn touch_last_used_at(&self, id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE api_keys
            SET last_used_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

impl From<ApiKeyRow> for ActiveApiKeyRecord {
    fn from(row: ApiKeyRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            owner: row.owner,
            key_prefix: row.key_prefix,
            allowed_office_ids: row.allowed_office_ids.0,
            allowed_capabilities: row.allowed_capabilities.0,
            can_view_pii: row.can_view_pii,
            expires_at: row.expires_at,
        }
    }
}
