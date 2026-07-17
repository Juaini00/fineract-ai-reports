use anyhow::Result;
use serde_json::json;
use sqlx::{FromRow, PgPool, types::Json};
use uuid::Uuid;

use crate::auth::model::{
    ActiveApiKeyRecord, AuthenticatedUserRecord, NewApiKeyRecord, NewRefreshTokenRecord,
    NewSessionRecord, NewUserRecord, UserRecord,
};

#[derive(Debug, FromRow)]
struct ApiKeyRow {
    id: Uuid,
    user_id: Option<Uuid>,
    name: String,
    owner: String,
    key_prefix: String,
    allowed_office_ids: Json<Vec<i64>>,
    allowed_capabilities: Json<Vec<String>>,
    allow_all_offices: bool,
    allow_all_capabilities: bool,
    can_view_pii: bool,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    email: Option<String>,
    password_hash: String,
    full_name: Option<String>,
    role: String,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    last_login_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, FromRow)]
pub struct ActiveRefreshTokenRecord {
    pub id: Uuid,
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub expires_at: chrono::DateTime<chrono::Utc>,
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
                user_id,
                name,
                owner,
                key_prefix,
                key_hash,
                allowed_office_ids,
                allowed_capabilities,
                allow_all_offices,
                allow_all_capabilities,
                can_view_pii,
                expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(record.id)
        .bind(record.user_id)
        .bind(record.name)
        .bind(record.owner)
        .bind(record.key_prefix)
        .bind(record.key_hash)
        .bind(Json(json!(record.allowed_office_ids)))
        .bind(Json(json!(record.allowed_capabilities)))
        .bind(record.allow_all_offices)
        .bind(record.allow_all_capabilities)
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
                user_id,
                name,
                owner,
                key_prefix,
                allowed_office_ids,
                allowed_capabilities,
                allow_all_offices,
                allow_all_capabilities,
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

    pub async fn count_for_user(&self, user_id: Uuid) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT count(*)
            FROM api_keys
            WHERE user_id = $1
              AND revoked_at IS NULL
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }
}

#[derive(Clone)]
pub struct UserRepository {
    pool: PgPool,
}

impl UserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn find_by_username(&self, username: &str) -> Result<Option<UserRecord>> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT id, username, email, password_hash, full_name, role, is_active, created_at, last_login_at
            FROM users
            WHERE username = $1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(UserRecord::from))
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<UserRecord>> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT id, username, email, password_hash, full_name, role, is_active, created_at, last_login_at
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(UserRecord::from))
    }

    pub async fn insert(&self, record: NewUserRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, password_hash, full_name, role)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(record.id)
        .bind(record.username)
        .bind(record.email)
        .bind(record.password_hash)
        .bind(record.full_name)
        .bind(record.role)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn touch_last_login_at(&self, id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE users
            SET last_login_at = now(), updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct SessionRepository {
    pool: PgPool,
}

impl SessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert_session(&self, record: NewSessionRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO user_sessions (id, user_id, user_agent, ip_address, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(record.id)
        .bind(record.user_id)
        .bind(record.user_agent)
        .bind(record.ip_address)
        .bind(record.expires_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn find_authenticated_user(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> Result<Option<AuthenticatedUserRecord>> {
        let row = sqlx::query_as::<_, (Uuid, Uuid, String)>(
            r#"
            SELECT u.id, us.id, u.role
            FROM users u
            JOIN user_sessions us ON us.user_id = u.id
            WHERE u.id = $1
              AND us.id = $2
              AND u.is_active = true
              AND us.revoked_at IS NULL
              AND us.expires_at > now()
            "#,
        )
        .bind(user_id)
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(
            row.map(|(user_id, session_id, role)| AuthenticatedUserRecord {
                user_id,
                session_id,
                role,
            }),
        )
    }

    pub async fn insert_refresh_token(&self, record: NewRefreshTokenRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO refresh_tokens (id, session_id, user_id, token_hash, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(record.id)
        .bind(record.session_id)
        .bind(record.user_id)
        .bind(record.token_hash)
        .bind(record.expires_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn find_active_refresh_token(
        &self,
        hash: &str,
    ) -> Result<Option<ActiveRefreshTokenRecord>> {
        let row = sqlx::query_as::<_, ActiveRefreshTokenRecord>(
            r#"
            SELECT rt.id, rt.session_id, rt.user_id, rt.expires_at
            FROM refresh_tokens rt
            JOIN user_sessions us ON us.id = rt.session_id
            WHERE rt.token_hash = $1
              AND rt.revoked_at IS NULL
              AND rt.expires_at > now()
              AND us.revoked_at IS NULL
              AND us.expires_at > now()
            "#,
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn revoke_session(&self, session_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE user_sessions
            SET revoked_at = now()
            WHERE id = $1
            "#,
        )
        .bind(session_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

impl From<ApiKeyRow> for ActiveApiKeyRecord {
    fn from(row: ApiKeyRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            name: row.name,
            owner: row.owner,
            key_prefix: row.key_prefix,
            allowed_office_ids: row.allowed_office_ids.0,
            allowed_capabilities: row.allowed_capabilities.0,
            allow_all_offices: row.allow_all_offices,
            allow_all_capabilities: row.allow_all_capabilities,
            can_view_pii: row.can_view_pii,
            expires_at: row.expires_at,
        }
    }
}

impl From<UserRow> for UserRecord {
    fn from(row: UserRow) -> Self {
        Self {
            id: row.id,
            username: row.username,
            email: row.email,
            password_hash: row.password_hash,
            full_name: row.full_name,
            role: row.role,
            is_active: row.is_active,
            created_at: row.created_at,
            last_login_at: row.last_login_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_row_preserves_user_id() {
        let user_id = Uuid::new_v4();
        let record = ActiveApiKeyRecord::from(ApiKeyRow {
            id: Uuid::new_v4(),
            user_id: Some(user_id),
            name: "name".to_string(),
            owner: "owner".to_string(),
            key_prefix: "air_12345678".to_string(),
            allowed_office_ids: Json(vec![1]),
            allowed_capabilities: Json(vec!["savings".to_string()]),
            allow_all_offices: false,
            allow_all_capabilities: false,
            can_view_pii: false,
            expires_at: None,
        });

        assert_eq!(record.user_id, Some(user_id));
    }
}
