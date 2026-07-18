use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use uuid::Uuid;

use crate::{
    auth::{
        api_key,
        model::{
            AuthenticatedUserRecord, ClientContext, CreateApiKeyInput, CreatedApiKey, LoginInput,
            LoginResult, NewApiKeyRecord, NewRefreshTokenRecord, NewSessionRecord, NewUserRecord,
            RefreshResult, UserProfile, UserRecord,
        },
        password,
        repository::{ApiKeyRepository, SessionRepository, UserRepository},
        token::{self, TokenService},
    },
    config::AuthConfig,
};

#[derive(Clone)]
pub struct AuthService {
    config: AuthConfig,
    api_key_repository: ApiKeyRepository,
    user_repository: UserRepository,
    session_repository: SessionRepository,
    token_service: TokenService,
    redis: Option<redis::Client>,
}

impl AuthService {
    pub fn new(
        config: AuthConfig,
        api_key_repository: ApiKeyRepository,
        user_repository: UserRepository,
        session_repository: SessionRepository,
        redis: Option<redis::Client>,
    ) -> Self {
        let token_service = TokenService::new(config.clone());
        Self {
            config,
            api_key_repository,
            user_repository,
            session_repository,
            token_service,
            redis,
        }
    }

    pub async fn bootstrap_admin(&self) -> Result<()> {
        if !self.config.bootstrap_admin_enabled {
            return Ok(());
        }

        if self
            .user_repository
            .find_by_username(&self.config.bootstrap_admin_username)
            .await?
            .is_some()
        {
            return Ok(());
        }

        let user_id = Uuid::new_v4();
        self.user_repository
            .insert(NewUserRecord {
                id: user_id,
                username: self.config.bootstrap_admin_username.clone(),
                email: Some(self.config.bootstrap_admin_email.clone()),
                password_hash: password::hash_password(&self.config.bootstrap_admin_password)?,
                full_name: Some("Administrator".to_string()),
                role: "admin".to_string(),
            })
            .await?;

        if self.api_key_repository.count_for_user(user_id).await? == 0 {
            let _ = self
                .create_api_key(CreateApiKeyInput {
                    name: "Default admin API key".to_string(),
                    owner: self.config.bootstrap_admin_username.clone(),
                    expires_at: None,
                    allowed_office_ids: Vec::new(),
                    allowed_capabilities: Vec::new(),
                    allow_all_offices: true,
                    allow_all_capabilities: true,
                    can_view_pii: true,
                    user_id: Some(user_id),
                })
                .await?;
        }

        Ok(())
    }

    pub async fn create_api_key(&self, input: CreateApiKeyInput) -> Result<CreatedApiKey> {
        validate_create_api_key_input(&input)?;

        let id = Uuid::new_v4();
        let raw_key = api_key::generate_api_key(&self.config.api_key_prefix);
        let record = NewApiKeyRecord {
            id,
            user_id: input.user_id,
            name: input.name,
            owner: input.owner,
            key_prefix: api_key::key_display_prefix(&raw_key),
            key_hash: api_key::hash_api_key(&raw_key),
            allowed_office_ids: input.allowed_office_ids,
            allowed_capabilities: input.allowed_capabilities,
            allow_all_offices: input.allow_all_offices,
            allow_all_capabilities: input.allow_all_capabilities,
            can_view_pii: input.can_view_pii,
            expires_at: input.expires_at.or_else(|| self.default_expiration()),
        };

        self.api_key_repository.insert(record).await?;

        Ok(CreatedApiKey { id, raw_key })
    }

    pub async fn authenticate_api_key(&self, raw_key: &str) -> Result<Option<ClientContext>> {
        let key_hash = api_key::hash_api_key(raw_key);
        let Some(record) = self
            .api_key_repository
            .find_active_by_hash(&key_hash)
            .await?
        else {
            return Ok(None);
        };

        if record.user_id.is_none() {
            return Ok(None);
        }

        self.api_key_repository
            .touch_last_used_at(record.id)
            .await?;

        Ok(Some(record.into()))
    }

    pub async fn login(&self, input: LoginInput) -> Result<(LoginResult, String)> {
        let user = self
            .user_repository
            .find_by_username(input.username.trim())
            .await?
            .ok_or_else(|| anyhow::anyhow!("invalid credentials"))?;

        if !user.is_active || !password::verify_password(&input.password, &user.password_hash)? {
            bail!("invalid credentials");
        }

        let session_id = Uuid::new_v4();
        let refresh = self.token_service.issue_refresh_token();
        let access = self
            .token_service
            .issue_access_token(user.id, session_id, &user.role)?;

        self.session_repository
            .insert_session(NewSessionRecord {
                id: session_id,
                user_id: user.id,
                user_agent: input.user_agent,
                ip_address: input.ip_address,
                expires_at: refresh.expires_at,
            })
            .await?;
        self.session_repository
            .insert_refresh_token(NewRefreshTokenRecord {
                id: refresh.id,
                session_id,
                user_id: user.id,
                token_hash: refresh.token_hash,
                expires_at: refresh.expires_at,
            })
            .await?;
        self.user_repository.touch_last_login_at(user.id).await?;

        Ok((
            LoginResult {
                access_token: access.token,
                token_type: "Bearer",
                expires_in: access.expires_in,
                user: user_profile(user),
            },
            refresh.raw_token,
        ))
    }

    pub async fn refresh(&self, raw_refresh_token: &str) -> Result<Option<RefreshResult>> {
        let hash = token::hash_token(raw_refresh_token);
        if self.refresh_token_revoked_in_redis(&hash).await {
            return Ok(None);
        }
        let Some(refresh) = self
            .session_repository
            .find_active_refresh_token(&hash)
            .await?
        else {
            return Ok(None);
        };
        let Some(user) = self.user_repository.find_by_id(refresh.user_id).await? else {
            return Ok(None);
        };
        if !user.is_active {
            return Ok(None);
        }
        let access =
            self.token_service
                .issue_access_token(user.id, refresh.session_id, &user.role)?;
        Ok(Some(RefreshResult {
            access_token: access.token,
            token_type: "Bearer",
            expires_in: access.expires_in,
        }))
    }

    pub async fn logout(&self, raw_refresh_token: &str) -> Result<()> {
        let hash = token::hash_token(raw_refresh_token);
        if let Some(refresh) = self
            .session_repository
            .find_active_refresh_token(&hash)
            .await?
        {
            self.session_repository
                .revoke_session(refresh.session_id)
                .await?;
            self.cache_refresh_token_revocation(&hash, refresh.expires_at)
                .await;
        }
        Ok(())
    }

    pub async fn get_user(&self, user_id: Uuid) -> Result<Option<UserProfile>> {
        Ok(self
            .user_repository
            .find_by_id(user_id)
            .await?
            .filter(|user| user.is_active)
            .map(user_profile))
    }

    pub async fn authenticate_access_token(
        &self,
        token: &str,
    ) -> Result<Option<AuthenticatedUserRecord>> {
        let claims = match self.token_service.verify_access_token(token) {
            Ok(claims) => claims,
            Err(_) => return Ok(None),
        };

        self.session_repository
            .find_authenticated_user(claims.sub, claims.sid)
            .await
    }

    async fn refresh_token_revoked_in_redis(&self, token_hash: &str) -> bool {
        let Some(client) = &self.redis else {
            return false;
        };
        let Ok(mut connection) = client.get_multiplexed_async_connection().await else {
            return false;
        };
        let key = format!("auth:revoked_refresh:{token_hash}");
        connection.exists::<_, bool>(key).await.unwrap_or(false)
    }

    async fn cache_refresh_token_revocation(&self, token_hash: &str, expires_at: DateTime<Utc>) {
        let Some(client) = &self.redis else {
            return;
        };
        let ttl = (expires_at - Utc::now()).num_seconds().max(1) as u64;
        let Ok(mut connection) = client.get_multiplexed_async_connection().await else {
            return;
        };
        let key = format!("auth:revoked_refresh:{token_hash}");
        let _: redis::RedisResult<()> = connection.set_ex(key, "1", ttl).await;
    }

    fn default_expiration(&self) -> Option<DateTime<Utc>> {
        if self.config.api_key_default_expiration_days == 0 {
            return None;
        }

        let duration = chrono::Duration::days(self.config.api_key_default_expiration_days as i64);
        Some(Utc::now() + duration)
    }
}

fn user_profile(user: UserRecord) -> UserProfile {
    UserProfile {
        id: user.id,
        username: user.username,
        email: user.email,
        full_name: user.full_name,
        role: user.role,
        is_active: user.is_active,
        created_at: user.created_at,
        last_login_at: user.last_login_at,
    }
}

fn validate_create_api_key_input(input: &CreateApiKeyInput) -> Result<()> {
    if input.name.trim().is_empty() {
        bail!("API key name is required");
    }

    if input.owner.trim().is_empty() {
        bail!("API key owner is required");
    }

    // A key explicitly marked not-all-offices with no offices listed must grant
    // NO access. An empty restricted scope reads as "unrestricted" downstream, so
    // reject the fail-open combination at creation rather than persist the trap.
    if !input.allow_all_offices && input.allowed_office_ids.is_empty() {
        bail!("API key with allow_all_offices=false must list at least one allowed office");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> CreateApiKeyInput {
        CreateApiKeyInput {
            name: "key".into(),
            owner: "owner".into(),
            expires_at: None,
            allowed_office_ids: Vec::new(),
            allowed_capabilities: Vec::new(),
            allow_all_offices: false,
            allow_all_capabilities: false,
            can_view_pii: false,
            user_id: None,
        }
    }

    #[test]
    fn rejects_restricted_key_with_empty_office_scope() {
        // allow_all_offices=false + no offices would collapse to full tenant downstream.
        let input = base_input();
        assert!(validate_create_api_key_input(&input).is_err());
    }

    #[test]
    fn accepts_restricted_key_with_offices() {
        let input = CreateApiKeyInput {
            allowed_office_ids: vec![1],
            ..base_input()
        };
        assert!(validate_create_api_key_input(&input).is_ok());
    }

    #[test]
    fn accepts_all_offices_key_without_offices() {
        let input = CreateApiKeyInput {
            allow_all_offices: true,
            ..base_input()
        };
        assert!(validate_create_api_key_input(&input).is_ok());
    }
}
