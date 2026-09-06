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
    pub llm: LlmConfig,
    pub embedding: EmbeddingConfig,
    pub voyage_ai: VoyageAiConfig,
    pub catalog: CatalogConfig,
    pub chat_features: ChatFeatureConfig,
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
    pub bootstrap_admin_enabled: bool,
    pub bootstrap_admin_username: String,
    pub bootstrap_admin_password: String,
    pub bootstrap_admin_email: String,
    pub jwt_access_secret: String,
    pub jwt_refresh_secret: String,
    pub jwt_access_token_expiry_seconds: i64,
    pub jwt_refresh_token_expiry_seconds: i64,
    pub refresh_cookie_name: String,
    pub refresh_cookie_secure: bool,
    pub refresh_cookie_same_site: String,
    pub refresh_cookie_path: String,
    pub api_key_prefix: String,
    pub api_key_default_expiration_days: u32,
}

#[derive(Clone, Debug)]
pub struct QueryConfig {
    pub default_timeout_ms: u64,
    /// Row ceiling used when a capability has no declared `hard_cap`.
    pub global_max_rows: i64,
}

#[derive(Clone, Debug)]
pub struct LlmConfig {
    pub provider: String,
    pub api_key: String,
    pub chat_completions_url: String,
    pub base_url: String,
    pub model: String,
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub max_output_tokens: u32,
    pub temperature: f32,
}

#[derive(Clone, Debug)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub dimensions: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LlmPrice {
    pub input_usd_per_1m: f64,
    pub output_usd_per_1m: f64,
}

pub fn llm_pricing(provider: &str, model: &str) -> Option<LlmPrice> {
    match (provider, model) {
        ("deepseek", "deepseek-chat") => Some(LlmPrice {
            input_usd_per_1m: 0.27,
            output_usd_per_1m: 1.10,
        }),
        ("openai", "gpt-4o-mini") => Some(LlmPrice {
            input_usd_per_1m: 0.15,
            output_usd_per_1m: 0.60,
        }),
        ("anthropic", "claude-3-5-haiku-latest") => Some(LlmPrice {
            input_usd_per_1m: 0.80,
            output_usd_per_1m: 4.00,
        }),
        _ => None,
    }
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

#[derive(Clone, Debug)]
pub struct ChatFeatureConfig {
    pub lqr_enabled: bool,
    pub context_soft_token_limit: usize,
    pub context_hard_token_limit: usize,
    pub context_max_recent_messages: usize,
    pub context_max_relevant_jobs: usize,
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
                bootstrap_admin_enabled: get_env_or("AUTH_BOOTSTRAP_ADMIN_ENABLED", "false")
                    .parse()
                    .context("AUTH_BOOTSTRAP_ADMIN_ENABLED must be true or false")?,
                bootstrap_admin_username: get_env_or("AUTH_BOOTSTRAP_ADMIN_USERNAME", "admin"),
                bootstrap_admin_password: get_env_or(
                    "AUTH_BOOTSTRAP_ADMIN_PASSWORD",
                    "password123",
                ),
                bootstrap_admin_email: get_env_or(
                    "AUTH_BOOTSTRAP_ADMIN_EMAIL",
                    "admin@example.com",
                ),
                jwt_access_secret: get_required_env("JWT_ACCESS_SECRET")?,
                jwt_refresh_secret: get_required_env("JWT_REFRESH_SECRET")?,
                jwt_access_token_expiry_seconds: get_env_or(
                    "JWT_ACCESS_TOKEN_EXPIRY_SECONDS",
                    "900",
                )
                .parse()
                .context("JWT_ACCESS_TOKEN_EXPIRY_SECONDS must be an integer")?,
                jwt_refresh_token_expiry_seconds: get_env_or(
                    "JWT_REFRESH_TOKEN_EXPIRY_SECONDS",
                    "604800",
                )
                .parse()
                .context("JWT_REFRESH_TOKEN_EXPIRY_SECONDS must be an integer")?,
                refresh_cookie_name: get_env_or("AUTH_REFRESH_COOKIE_NAME", "refresh_token"),
                refresh_cookie_secure: get_env_or("AUTH_REFRESH_COOKIE_SECURE", "true")
                    .parse()
                    .context("AUTH_REFRESH_COOKIE_SECURE must be true or false")?,
                refresh_cookie_same_site: get_env_or("AUTH_REFRESH_COOKIE_SAME_SITE", "strict"),
                refresh_cookie_path: get_env_or("AUTH_REFRESH_COOKIE_PATH", "/"),
                api_key_prefix: get_env_or("API_KEY_PREFIX", "air_test"),
                api_key_default_expiration_days: get_env_or("API_KEY_DEFAULT_EXPIRATION_DAYS", "0")
                    .parse()
                    .context("API_KEY_DEFAULT_EXPIRATION_DAYS must be an integer")?,
            },
            query: QueryConfig {
                default_timeout_ms: get_env_or("QUERY_DEFAULT_TIMEOUT_MS", "3000")
                    .parse()
                    .context("QUERY_DEFAULT_TIMEOUT_MS must be an integer")?,
                global_max_rows: get_env_or("QUERY_GLOBAL_MAX_ROWS", "50000")
                    .parse()
                    .context("QUERY_GLOBAL_MAX_ROWS must be an integer")?,
            },
            llm: LlmConfig {
                provider: get_env_or("LLM_PROVIDER", "deepseek"),
                api_key: get_env_or("LLM_API_KEY", &get_env_or("DEEPSEEK_API_KEY", "")),
                chat_completions_url: get_env_or(
                    "LLM_CHAT_COMPLETIONS_URL",
                    &get_env_or(
                        "DEEPSEEK_CHAT_COMPLETIONS_URL",
                        "https://api.deepseek.com/chat/completions",
                    ),
                ),
                base_url: get_env_or("LLM_BASE_URL", ""),
                model: get_env_or("LLM_MODEL", &get_env_or("DEEPSEEK_MODEL", "deepseek-chat")),
                timeout_ms: get_env_or(
                    "LLM_TIMEOUT_MS",
                    &get_env_or("DEEPSEEK_TIMEOUT_MS", "30000"),
                )
                .parse()
                .context("LLM_TIMEOUT_MS must be an integer")?,
                max_retries: get_env_or("LLM_MAX_RETRIES", "3")
                    .parse()
                    .context("LLM_MAX_RETRIES must be an integer")?,
                max_output_tokens: get_env_or(
                    "LLM_MAX_OUTPUT_TOKENS",
                    &get_env_or("DEEPSEEK_MAX_OUTPUT_TOKENS", "4000"),
                )
                .parse()
                .context("LLM_MAX_OUTPUT_TOKENS must be an integer")?,
                temperature: get_env_or(
                    "LLM_TEMPERATURE",
                    &get_env_or("DEEPSEEK_TEMPERATURE", "0.1"),
                )
                .parse()
                .context("LLM_TEMPERATURE must be a number")?,
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
            embedding: EmbeddingConfig {
                provider: get_env_or("EMBEDDING_PROVIDER", "voyageai"),
                api_key: get_env_or("EMBEDDING_API_KEY", &get_env_or("VOYAGEAI_API_KEY", "")),
                base_url: get_env_or(
                    "EMBEDDING_BASE_URL",
                    &get_env_or("VOYAGEAI_BASE_URL", "https://api.voyageai.com/v1"),
                ),
                model: get_env_or(
                    "EMBEDDING_MODEL",
                    &get_env_or("VOYAGEAI_EMBEDDING_MODEL", "voyage-3-large"),
                ),
                timeout_ms: get_env_or(
                    "EMBEDDING_TIMEOUT_MS",
                    &get_env_or("VOYAGEAI_TIMEOUT_MS", "30000"),
                )
                .parse()
                .context("EMBEDDING_TIMEOUT_MS must be an integer")?,
                max_retries: get_env_or("EMBEDDING_MAX_RETRIES", "3")
                    .parse()
                    .context("EMBEDDING_MAX_RETRIES must be an integer")?,
                dimensions: get_env_or("EMBEDDING_DIMENSIONS", "1024")
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
            chat_features: ChatFeatureConfig {
                lqr_enabled: get_env_or("LQR_ENABLED", "true").eq_ignore_ascii_case("true"),
                context_soft_token_limit: get_env_or("CHAT_CONTEXT_SOFT_TOKEN_LIMIT", "6000")
                    .parse()
                    .context("CHAT_CONTEXT_SOFT_TOKEN_LIMIT must be an integer")?,
                context_hard_token_limit: get_env_or("CHAT_CONTEXT_HARD_TOKEN_LIMIT", "8000")
                    .parse()
                    .context("CHAT_CONTEXT_HARD_TOKEN_LIMIT must be an integer")?,
                context_max_recent_messages: get_env_or("CHAT_CONTEXT_MAX_RECENT_MESSAGES", "12")
                    .parse()
                    .context("CHAT_CONTEXT_MAX_RECENT_MESSAGES must be an integer")?,
                context_max_relevant_jobs: get_env_or("CHAT_CONTEXT_MAX_RELEVANT_JOBS", "3")
                    .parse()
                    .context("CHAT_CONTEXT_MAX_RELEVANT_JOBS must be an integer")?,
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
