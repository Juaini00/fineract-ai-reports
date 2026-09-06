use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::{FromRow, PgPool};
use tokio::time::MissedTickBehavior;
use tracing::warn;
use uuid::Uuid;

use super::{model::NormalizedErrorCode, repository::ManagementAuditEvent};

const DISPATCH_BATCH_SIZE: i64 = 50;
const DISPATCH_POLL_INTERVAL: Duration = Duration::from_secs(1);
const RETRY_BASE_SECONDS: u64 = 1;
const RETRY_MAX_SECONDS: u64 = 300;

#[derive(Clone)]
pub struct ManagementOutboxDispatcher {
    pool: PgPool,
}

impl ManagementOutboxDispatcher {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Materialize due rows once. Row locks make concurrent dispatchers safe;
    /// the unique outbox_id constraint makes a retry after a crash idempotent.
    pub async fn dispatch_due(&self, limit: i64) -> Result<usize> {
        let mut published = 0;
        for _ in 0..limit.clamp(1, DISPATCH_BATCH_SIZE) {
            match self.dispatch_one().await? {
                Some(true) => published += 1,
                Some(false) => {}
                None => break,
            }
        }
        Ok(published)
    }

    async fn dispatch_one(&self) -> Result<Option<bool>> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, OutboxRow>(
            "SELECT id, payload FROM management_audit_outbox WHERE published_at IS NULL AND next_attempt_at <= now() ORDER BY next_attempt_at, created_at FOR UPDATE SKIP LOCKED LIMIT 1",
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };

        let publish_result = publish(&mut tx, &row).await;
        match publish_result {
            Ok(()) => {
                tx.commit().await?;
                Ok(Some(true))
            }
            Err(error) => {
                let code = normalize_failure_code(&error);
                tx.rollback().await?;
                self.record_failure(row.id, code).await?;
                warn!(outbox_id = %row.id, error_code = ?code, "management audit outbox publish failed; retry scheduled");
                Ok(Some(false))
            }
        }
    }

    async fn record_failure(&self, id: Uuid, code: NormalizedErrorCode) -> Result<()> {
        sqlx::query(
            "UPDATE management_audit_outbox SET attempt_count = attempt_count + 1, last_error_code = $2, next_attempt_at = now() + make_interval(secs => LEAST($3::double precision * power(2::double precision, LEAST(attempt_count, $4)), $5::double precision)) WHERE id = $1 AND published_at IS NULL",
        )
        .bind(id)
        .bind(error_code(&code))
        .bind(RETRY_BASE_SECONDS as f64)
        .bind(max_backoff_exponent())
        .bind(RETRY_MAX_SECONDS as f64)
        .execute(&self.pool)
        .await
        .context("record management audit outbox publish failure")?;
        Ok(())
    }
}

/// Starts the bounded polling loop used by the chat application lifecycle.
pub fn spawn_outbox_dispatcher(pool: PgPool) {
    tokio::spawn(async move {
        let dispatcher = ManagementOutboxDispatcher::new(pool);
        let mut interval = tokio::time::interval(DISPATCH_POLL_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = dispatcher.dispatch_due(DISPATCH_BATCH_SIZE).await {
                warn!(error = %error, "management audit outbox dispatch pass failed");
            }
        }
    });
}

async fn publish(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, row: &OutboxRow) -> Result<()> {
    let event: ManagementAuditEvent = serde_json::from_value(row.payload.clone())?;
    sqlx::query(
        "INSERT INTO management_audit_events (id, outbox_id, job_id, session_id, aggregate_type, aggregate_id, actor_user_id, event_type, outcome, contract_version, summary_json, sanitized_error_json, occurred_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 1, $10, $11, $12) ON CONFLICT (outbox_id) DO NOTHING",
    )
    .bind(Uuid::new_v4()).bind(row.id).bind(event.job_id).bind(event.session_id)
    .bind(event.aggregate_type.as_str()).bind(event.aggregate_id).bind(event.actor_user_id)
    .bind(event.event_type.as_str()).bind(event.outcome.as_str())
    .bind(serde_json::to_value(event.summary)?)
    .bind(event.sanitized_error.map(serde_json::to_value).transpose()?)
    .bind(event.occurred_at).execute(&mut **tx).await?;
    sqlx::query("UPDATE management_audit_outbox SET published_at = now(), last_error_code = NULL WHERE id = $1")
        .bind(row.id).execute(&mut **tx).await?;
    Ok(())
}

fn normalize_failure_code(error: &anyhow::Error) -> NormalizedErrorCode {
    if error.downcast_ref::<serde_json::Error>().is_some() {
        return NormalizedErrorCode::SerializationFailed;
    }
    if let Some(sqlx::Error::Database(database_error)) = error.downcast_ref::<sqlx::Error>()
        && database_error.code().as_deref() == Some("40001")
    {
        return NormalizedErrorCode::SerializationFailed;
    }
    if error.downcast_ref::<sqlx::Error>().is_some() {
        return NormalizedErrorCode::DatabaseUnavailable;
    }
    NormalizedErrorCode::DispatcherUnavailable
}

const fn error_code(code: &NormalizedErrorCode) -> &'static str {
    match code {
        NormalizedErrorCode::DatabaseUnavailable => "database_unavailable",
        NormalizedErrorCode::DispatcherUnavailable => "dispatcher_unavailable",
        NormalizedErrorCode::SerializationFailed => "serialization_failed",
        _ => "unknown",
    }
}

const fn max_backoff_exponent() -> i32 {
    let mut exponent = 0;
    let mut seconds = RETRY_BASE_SECONDS;
    while seconds < RETRY_MAX_SECONDS {
        seconds *= 2;
        exponent += 1;
    }
    exponent
}

#[derive(FromRow)]
struct OutboxRow {
    id: Uuid,
    payload: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_codes_are_normalized() {
        let serialization =
            anyhow::Error::from(serde_json::from_str::<serde_json::Value>("{").unwrap_err());
        assert!(matches!(
            normalize_failure_code(&serialization),
            NormalizedErrorCode::SerializationFailed
        ));
        assert!(matches!(
            normalize_failure_code(&anyhow::anyhow!("internal failure")),
            NormalizedErrorCode::DispatcherUnavailable
        ));
    }

    #[test]
    fn polling_batch_is_bounded() {
        assert_eq!(0_i64.clamp(1, DISPATCH_BATCH_SIZE), 1);
        assert_eq!(i64::MAX.clamp(1, DISPATCH_BATCH_SIZE), DISPATCH_BATCH_SIZE);
    }

    #[test]
    fn retry_backoff_has_a_finite_cap() {
        assert!(RETRY_BASE_SECONDS * (1 << max_backoff_exponent()) >= RETRY_MAX_SECONDS);
        assert_eq!(super::super::repository::OUTBOX_MAX_ATTEMPTS, 10);
    }
}
