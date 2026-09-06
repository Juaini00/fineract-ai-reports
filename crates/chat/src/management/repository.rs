use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::model::{
    AuditAggregateType, AuditEventType, AuditOutcome, AuditSummary, SanitizedError,
};

pub const OUTBOX_MAX_ATTEMPTS: i32 = 10;

#[derive(Debug, Clone, FromRow)]
pub struct OutboxHealth {
    pub pending: i64,
    pub oldest_pending_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub exhausted: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManagementAuditEvent {
    pub aggregate_type: AuditAggregateType,
    pub aggregate_id: Uuid,
    pub job_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub event_type: AuditEventType,
    pub outcome: AuditOutcome,
    pub summary: AuditSummary,
    pub sanitized_error: Option<SanitizedError>,
    pub occurred_at: DateTime<Utc>,
}

/// Returns the pending backlog and retry state without exposing outbox payloads.
pub async fn outbox_health(pool: &PgPool) -> Result<OutboxHealth> {
    Ok(sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE published_at IS NULL) AS pending, MIN(created_at) FILTER (WHERE published_at IS NULL) AS oldest_pending_at, MIN(next_attempt_at) FILTER (WHERE published_at IS NULL AND attempt_count > 0) AS next_retry_at, COUNT(*) FILTER (WHERE published_at IS NULL AND attempt_count >= $1) AS exhausted FROM management_audit_outbox",
    )
    .bind(OUTBOX_MAX_ATTEMPTS)
    .fetch_one(pool)
    .await?)
}

pub async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    event: ManagementAuditEvent,
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    let payload = serde_json::to_value(&event)?;
    sqlx::query(
        "INSERT INTO management_audit_outbox (id, aggregate_type, aggregate_id, job_id, session_id, actor_user_id, contract_version, payload, occurred_at) VALUES ($1, $2, $3, $4, $5, $6, 1, $7, $8)",
    )
    .bind(id)
    .bind(event.aggregate_type.as_str())
    .bind(event.aggregate_id)
    .bind(event.job_id)
    .bind(event.session_id)
    .bind(event.actor_user_id)
    .bind(payload)
    .bind(event.occurred_at)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}
