use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{AssertSqlSafe, FromRow, PgPool};

use crate::api::dto::management::{LlmGroupBy, LlmUsageQuery, WarningCode};

#[derive(Clone)]
pub struct LlmUsageRepository {
    pool: PgPool,
}

impl LlmUsageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn aggregate(&self, query: &LlmUsageQuery) -> Result<LlmUsageResponse> {
        let expression = match query.group_by {
            LlmGroupBy::Day => {
                "to_char(date_trunc('day', created_at) AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')"
            }
            LlmGroupBy::Model => "model",
            LlmGroupBy::Purpose => "purpose",
            LlmGroupBy::Status => "status",
        };
        // `expression` is selected exclusively from the closed enum above; no request
        // value is interpolated into SQL.
        let sql = format!(
            "SELECT {expression} AS group_key, count(*)::bigint AS calls, \
             sum(input_tokens)::bigint AS input_tokens, sum(output_tokens)::bigint AS output_tokens, \
             sum(total_tokens)::bigint AS total_tokens, \
             count(*) FILTER (WHERE usage_status = 'unavailable')::bigint AS unknown_usage, \
             count(*) FILTER (WHERE status <> 'ok')::bigint AS errors, \
             percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms)::bigint AS p95_latency_ms, \
             sum(cost_usd) AS cost_amount, min(cost_currency) AS cost_currency, min(price_version) AS price_version, \
             count(DISTINCT (cost_currency, price_version)) FILTER (WHERE cost_usd IS NOT NULL)::bigint AS price_versions \
             FROM assistant_llm_traces WHERE created_at >= $1 AND created_at < $2 \
             GROUP BY {expression} ORDER BY group_key"
        );
        let rows = sqlx::query_as::<_, UsageRow>(AssertSqlSafe(sql))
            .bind(query.from)
            .bind(query.to)
            .fetch_all(&self.pool)
            .await?;
        let usage_missing = rows.iter().any(|row| row.unknown_usage > 0);
        let price_mismatch = rows.iter().any(|row| row.price_versions > 1);
        let groups = rows.into_iter().map(UsageGroup::from).collect::<Vec<_>>();
        let warnings = [
            usage_missing.then_some(WarningCode::UsageMissing),
            price_mismatch.then_some(WarningCode::PriceVersionMismatch),
        ]
        .into_iter()
        .flatten()
        .collect();
        Ok(LlmUsageResponse {
            range: UsageRange {
                from: query.from,
                to: query.to,
            },
            groups,
            warnings,
        })
    }
}

#[derive(Debug, FromRow)]
struct UsageRow {
    group_key: String,
    calls: i64,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    total_tokens: Option<i64>,
    unknown_usage: i64,
    errors: i64,
    p95_latency_ms: Option<i64>,
    cost_amount: Option<Decimal>,
    cost_currency: Option<String>,
    price_version: Option<String>,
    price_versions: i64,
}

#[derive(Debug, Serialize)]
pub struct LlmUsageResponse {
    pub range: UsageRange,
    pub groups: Vec<UsageGroup>,
    pub warnings: Vec<WarningCode>,
}

#[derive(Debug, Serialize)]
pub struct UsageRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct UsageGroup {
    pub key: String,
    pub calls: i64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub unknown_usage_calls: i64,
    pub errors: i64,
    pub p95_latency_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<UsageCost>,
}

#[derive(Debug, Serialize)]
pub struct UsageCost {
    pub amount: Decimal,
    pub currency: String,
    pub price_version: String,
}

impl From<UsageRow> for UsageGroup {
    fn from(row: UsageRow) -> Self {
        let estimated_cost = (row.price_versions <= 1)
            .then_some((row.cost_amount, row.cost_currency, row.price_version))
            .and_then(|(amount, currency, price_version)| {
                Some(UsageCost {
                    amount: amount?,
                    currency: currency?,
                    price_version: price_version?,
                })
            });
        Self {
            key: row.group_key,
            calls: row.calls,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            total_tokens: row.total_tokens,
            unknown_usage_calls: row.unknown_usage,
            errors: row.errors,
            p95_latency_ms: row.p95_latency_ms,
            estimated_cost,
        }
    }
}
