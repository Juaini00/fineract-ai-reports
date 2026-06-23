use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub app: ServerConfig,
    pub app_database_url: String,
    pub app_database_migrate_on_startup: bool,
    pub fineract_database_url: String,
    pub redis: RedisConfig,
    pub auth: AuthConfig,
    pub query: QueryConfig,
    pub voyage_ai: VoyageAiConfig,
    pub catalog: CatalogConfig,
}

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub env: String,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug)]
pub struct RedisConfig {
    pub enabled: bool,
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub bootstrap_admin_token: String,
    pub api_key_prefix: String,
    pub api_key_default_expiration_days: u32,
}

#[derive(Clone, Debug)]
pub struct QueryConfig {
    pub default_timeout_ms: u64,
}

#[derive(Clone, Debug)]
pub struct VoyageAiConfig {
    pub api_key: String,
    pub base_url: String,
    pub embedding_model: String,
    pub timeout_ms: u64,
    pub embedding_dimensions: i32,
}

#[derive(Clone, Debug)]
pub struct CatalogConfig {
    pub path: String,
    pub query_path: String,
    pub validate_on_startup: bool,
    pub sync_on_startup: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            app: ServerConfig {
                env: get_env_or("APP_ENV", "local"),
                host: get_env_or("APP_HOST", "127.0.0.1"),
                port: get_env_or("APP_PORT", "3000")
                    .parse()
                    .context("APP_PORT must be a valid port")?,
            },
            app_database_url: get_required_env("APP_DATABASE_URL")?,
            app_database_migrate_on_startup: get_env_or("APP_DATABASE_MIGRATE_ON_STARTUP", "false")
                .parse()
                .context("APP_DATABASE_MIGRATE_ON_STARTUP must be true or false")?,
            fineract_database_url: get_required_env("FINERACT_DATABASE_URL")?,
            redis: RedisConfig {
                enabled: get_env_or("REDIS_ENABLED", "false")
                    .parse()
                    .context("REDIS_ENABLED must be true or false")?,
                url: get_env_or("REDIS_URL", "redis://127.0.0.1:6379/0"),
            },
            auth: AuthConfig {
                bootstrap_admin_token: get_required_env("AUTH_BOOTSTRAP_ADMIN_TOKEN")?,
                api_key_prefix: get_env_or("API_KEY_PREFIX", "air_test"),
                api_key_default_expiration_days: get_env_or("API_KEY_DEFAULT_EXPIRATION_DAYS", "0")
                    .parse()
                    .context("API_KEY_DEFAULT_EXPIRATION_DAYS must be an integer")?,
            },
            query: QueryConfig {
                default_timeout_ms: get_env_or("QUERY_DEFAULT_TIMEOUT_MS", "3000")
                    .parse()
                    .context("QUERY_DEFAULT_TIMEOUT_MS must be an integer")?,
            },
            voyage_ai: VoyageAiConfig {
                api_key: get_env_or("VOYAGEAI_API_KEY", ""),
                base_url: get_env_or("VOYAGEAI_BASE_URL", "https://api.voyageai.com/v1"),
                embedding_model: get_env_or("VOYAGEAI_EMBEDDING_MODEL", "voyage-3-large"),
                timeout_ms: get_env_or("VOYAGEAI_TIMEOUT_MS", "30000")
                    .parse()
                    .context("VOYAGEAI_TIMEOUT_MS must be an integer")?,
                embedding_dimensions: get_env_or("EMBEDDING_DIMENSIONS", "1024")
                    .parse()
                    .context("EMBEDDING_DIMENSIONS must be an integer")?,
            },
            catalog: CatalogConfig {
                path: get_env_or("CATALOG_PATH", "knowledge"),
                query_path: get_env_or("QUERY_PATH", "queries"),
                validate_on_startup: get_env_or("CATALOG_VALIDATE_ON_STARTUP", "true")
                    .parse()
                    .context("CATALOG_VALIDATE_ON_STARTUP must be true or false")?,
                sync_on_startup: get_env_or("CATALOG_SYNC_ON_STARTUP", "false")
                    .parse()
                    .context("CATALOG_SYNC_ON_STARTUP must be true or false")?,
            },
        })
    }
}

fn get_required_env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing required environment variable {key}"))
}

fn get_env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
