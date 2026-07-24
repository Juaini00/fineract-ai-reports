use anyhow::{Result, anyhow};
use rust_decimal::{Decimal, prelude::FromPrimitive};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmTraceErrorCode {
    ProviderUnavailable,
    ProviderTimeout,
    ProviderMalformed,
    Unknown,
}

impl LlmTraceErrorCode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ProviderUnavailable => "provider_unavailable",
            Self::ProviderTimeout => "provider_timeout",
            Self::ProviderMalformed => "provider_malformed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmTraceUsageStatus {
    ProviderReported,
    Estimated,
    Unavailable,
}

impl LlmTraceUsageStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ProviderReported => "provider_reported",
            Self::Estimated => "estimated",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmTrace {
    pub job_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub user_id: Uuid,
    pub legacy_api_key_id: Option<Uuid>,
    pub graph_state: Option<String>,
    pub correlation_id: Option<Uuid>,
    pub context_contract_version: Option<i16>,
    pub catalog_version_id: Option<Uuid>,
    pub index_version_id: Option<Uuid>,
    pub purpose: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub usage_status: LlmTraceUsageStatus,
    pub cost_usd: Option<f64>,
    pub price_version: Option<String>,
    pub cost_currency: Option<String>,
    pub latency_ms: i32,
    pub status: String,
    /// Compatibility-only legacy field. Do not add new consumers of it.
    pub error_kind: Option<String>,
    pub error_code: Option<LlmTraceErrorCode>,
}

#[derive(Debug, Clone, FromRow)]
pub struct LlmTraceRecord {
    pub id: Uuid,
    pub job_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub legacy_api_key_id: Option<Uuid>,
    pub graph_state: Option<String>,
    pub correlation_id: Option<Uuid>,
    pub context_contract_version: Option<i16>,
    pub catalog_version_id: Option<Uuid>,
    pub index_version_id: Option<Uuid>,
    pub purpose: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub total_tokens: Option<i32>,
    pub usage_status: String,
    pub cost_usd: Option<Decimal>,
    pub price_version: Option<String>,
    pub cost_currency: Option<String>,
    pub latency_ms: i32,
    pub status: String,
    pub error_kind: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Clone)]
pub struct LlmTraceRepository {
    pool: PgPool,
}

impl LlmTraceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn record(&self, trace: &LlmTrace) -> Result<()> {
        let (input_tokens, output_tokens, total_tokens) = match trace.usage_status {
            LlmTraceUsageStatus::Unavailable => (None, None, None),
            LlmTraceUsageStatus::ProviderReported | LlmTraceUsageStatus::Estimated => {
                let (input_tokens, output_tokens) = trace
                    .input_tokens
                    .zip(trace.output_tokens)
                    .ok_or_else(|| anyhow!("reported LLM usage requires both token counts"))?;
                (
                    Some(input_tokens),
                    Some(output_tokens),
                    Some(input_tokens + output_tokens),
                )
            }
        };
        let cost = trace.cost_usd.and_then(Decimal::from_f64);
        sqlx::query(
            r#"
            INSERT INTO assistant_llm_traces (
                id, job_id, session_id, user_id, api_key_id, graph_state,
                correlation_id, context_contract_version, catalog_version_id, index_version_id,
                purpose, provider, model, input_tokens, output_tokens, total_tokens, usage_status,
                cost_usd, price_version, cost_currency, latency_ms, status, error_kind, error_code
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(trace.job_id)
        .bind(trace.session_id)
        .bind(trace.user_id)
        .bind(trace.legacy_api_key_id)
        .bind(&trace.graph_state)
        .bind(trace.correlation_id)
        .bind(trace.context_contract_version)
        .bind(trace.catalog_version_id)
        .bind(trace.index_version_id)
        .bind(&trace.purpose)
        .bind(&trace.provider)
        .bind(&trace.model)
        .bind(input_tokens)
        .bind(output_tokens)
        .bind(total_tokens)
        .bind(trace.usage_status.as_str())
        .bind(cost)
        .bind(&trace.price_version)
        .bind(&trace.cost_currency)
        .bind(trace.latency_ms)
        .bind(&trace.status)
        .bind(&trace.error_kind)
        .bind(trace.error_code.map(LlmTraceErrorCode::as_str))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_for_job(&self, job_id: Uuid) -> Result<Vec<LlmTraceRecord>> {
        self.list_for_job_filtered(job_id, None, None).await
    }

    pub async fn list_for_job_filtered(
        &self,
        job_id: Uuid,
        purpose: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<LlmTraceRecord>> {
        Ok(sqlx::query_as::<_, LlmTraceRecord>(
            r#"
            SELECT id, job_id, session_id, user_id, api_key_id AS legacy_api_key_id,
                graph_state, correlation_id, context_contract_version, catalog_version_id, index_version_id,
                purpose, provider, model, input_tokens, output_tokens, total_tokens, usage_status,
                cost_usd, price_version, cost_currency, latency_ms, status, error_kind, error_code
            FROM assistant_llm_traces
            WHERE job_id = $1
              AND ($2::text IS NULL OR purpose = $2)
              AND ($3::text IS NULL OR status = $3)
            ORDER BY created_at DESC
            "#,
        )
        .bind(job_id)
        .bind(purpose)
        .bind(status)
        .fetch_all(&self.pool)
        .await?)
    }
}
