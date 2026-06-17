use std::path::Path;

use anyhow::{Context, Result};
use redis::AsyncCommands;
use serde::Serialize;
use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::config::AppConfig;

#[derive(Clone)]
pub struct DatabasePools {
    pub app: PgPool,
    pub fineract: PgPool,
    pub redis: Option<redis::Client>,
}

#[derive(Debug, Serialize)]
pub struct ReadinessChecks {
    pub app_database: CheckStatus,
    pub fineract_database: CheckStatus,
    pub pgvector: CheckStatus,
    pub redis: CheckStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Disabled,
    Error { message: String },
}

impl ReadinessChecks {
    pub fn is_ready(&self) -> bool {
        self.app_database.is_ok()
            && self.fineract_database.is_ok()
            && self.pgvector.is_ok()
            && self.redis.is_ok_or_disabled()
    }
}

impl CheckStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }

    pub fn is_ok_or_disabled(&self) -> bool {
        matches!(self, Self::Ok | Self::Disabled)
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Ok => "ok",
            Self::Disabled => "disabled",
            Self::Error { .. } => "error",
        }
    }
}

impl DatabasePools {
    pub async fn connect(config: &AppConfig) -> Result<Self> {
        let app = PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.app_database_url)
            .await?;

        let fineract = PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.fineract_database_url)
            .await?;

        let redis = if config.redis.enabled {
            Some(redis::Client::open(config.redis.url.as_str())?)
        } else {
            None
        };

        Ok(Self {
            app,
            fineract,
            redis,
        })
    }

    pub async fn run_app_migrations(&self) -> Result<()> {
        let migrations_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        let migrator = sqlx::migrate::Migrator::new(migrations_path.as_path())
            .await
            .with_context(|| {
                format!(
                    "failed to load app database migrations from {}",
                    migrations_path.display()
                )
            })?;

        migrator.run(&self.app).await?;
        Ok(())
    }

    pub async fn readiness(&self) -> ReadinessChecks {
        ReadinessChecks {
            app_database: check_postgres(&self.app).await,
            fineract_database: check_postgres(&self.fineract).await,
            pgvector: check_pgvector(&self.app).await,
            redis: check_redis(self.redis.as_ref()).await,
        }
    }
}

async fn check_postgres(pool: &PgPool) -> CheckStatus {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
    {
        Ok(_) => CheckStatus::Ok,
        Err(error) => CheckStatus::Error {
            message: error.to_string(),
        },
    }
}

async fn check_pgvector(pool: &PgPool) -> CheckStatus {
    let result = sqlx::query_scalar::<_, String>(
        "SELECT extversion FROM pg_extension WHERE extname = 'vector'",
    )
    .fetch_optional(pool)
    .await;

    match result {
        Ok(Some(_)) => CheckStatus::Ok,
        Ok(None) => CheckStatus::Error {
            message: "pgvector extension is not enabled".to_string(),
        },
        Err(error) => CheckStatus::Error {
            message: error.to_string(),
        },
    }
}

async fn check_redis(client: Option<&redis::Client>) -> CheckStatus {
    let Some(client) = client else {
        return CheckStatus::Disabled;
    };

    let mut connection = match client.get_multiplexed_async_connection().await {
        Ok(connection) => connection,
        Err(error) => {
            return CheckStatus::Error {
                message: error.to_string(),
            };
        }
    };

    match connection.ping::<String>().await {
        Ok(_) => CheckStatus::Ok,
        Err(error) => CheckStatus::Error {
            message: error.to_string(),
        },
    }
}
