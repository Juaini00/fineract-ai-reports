use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    auth::{
        api_key,
        model::{ClientContext, CreateApiKeyInput, CreatedApiKey, NewApiKeyRecord},
        repository::ApiKeyRepository,
    },
    config::AuthConfig,
};

#[derive(Clone)]
pub struct AuthService {
    config: AuthConfig,
    api_key_repository: ApiKeyRepository,
}

impl AuthService {
    pub fn new(config: AuthConfig, api_key_repository: ApiKeyRepository) -> Self {
        Self {
            config,
            api_key_repository,
        }
    }

    pub async fn create_api_key(&self, input: CreateApiKeyInput) -> Result<CreatedApiKey> {
        validate_create_api_key_input(&input)?;

        let id = Uuid::new_v4();
        let raw_key = api_key::generate_api_key(&self.config.api_key_prefix);
        let record = NewApiKeyRecord {
            id,
            name: input.name,
            owner: input.owner,
            key_prefix: api_key::key_display_prefix(&raw_key),
            key_hash: api_key::hash_api_key(&raw_key),
            allowed_office_ids: input.allowed_office_ids,
            allowed_capabilities: input.allowed_capabilities,
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

        self.api_key_repository
            .touch_last_used_at(record.id)
            .await?;

        Ok(Some(record.into()))
    }

    fn default_expiration(&self) -> Option<DateTime<Utc>> {
        if self.config.api_key_default_expiration_days == 0 {
            return None;
        }

        let duration = chrono::Duration::days(self.config.api_key_default_expiration_days as i64);
        Some(Utc::now() + duration)
    }
}

fn validate_create_api_key_input(input: &CreateApiKeyInput) -> Result<()> {
    if input.name.trim().is_empty() {
        bail!("API key name is required");
    }

    if input.owner.trim().is_empty() {
        bail!("API key owner is required");
    }

    if input.allowed_capabilities.is_empty() {
        bail!("at least one allowed capability is required");
    }

    Ok(())
}
