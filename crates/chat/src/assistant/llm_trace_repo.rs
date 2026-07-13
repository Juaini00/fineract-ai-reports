use anyhow::Result;
use rust_decimal::{Decimal, prelude::FromPrimitive};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct LlmTrace {
    pub job_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub api_key_id: Uuid,
    pub graph_state: Option<String>,
    pub purpose: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub cost_usd: Option<f64>,
    pub latency_ms: i32,
    pub status: String,
    pub error_kind: Option<String>,
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
        let cost = trace.cost_usd.and_then(Decimal::from_f64);
        sqlx::query(
            r#"
            INSERT INTO assistant_llm_traces (
                id, job_id, session_id, api_key_id, graph_state, purpose, provider, model,
                input_tokens, output_tokens, total_tokens, cost_usd, latency_ms, status, error_kind
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(trace.job_id)
        .bind(trace.session_id)
        .bind(trace.api_key_id)
        .bind(&trace.graph_state)
        .bind(&trace.purpose)
        .bind(&trace.provider)
        .bind(&trace.model)
        .bind(trace.input_tokens)
        .bind(trace.output_tokens)
        .bind(trace.input_tokens + trace.output_tokens)
        .bind(cost)
        .bind(trace.latency_ms)
        .bind(&trace.status)
        .bind(&trace.error_kind)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
