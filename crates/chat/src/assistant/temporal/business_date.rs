use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::management::model::{
    AuditAggregateType, AuditEventType, AuditOutcome, AuditSummary, SafeIdentifier,
};
use crate::management::{ManagementAuditEvent, enqueue};

/// A tenant business date resolution with provenance so downstream code can
/// audit whether "today" came from Fineract or a wall-clock fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusinessDate {
    pub date: NaiveDate,
    pub source: BusinessDateSource,
    pub resolved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusinessDateSource {
    Fineract,
    WallClockFallback,
}

#[derive(Debug)]
pub enum BusinessDateError {
    Query(anyhow::Error),
}

impl std::fmt::Display for BusinessDateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Query(_) => write!(f, "failed to query Fineract business date"),
        }
    }
}

impl std::error::Error for BusinessDateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Query(err) => Some(err.as_ref()),
        }
    }
}

#[async_trait]
pub trait BusinessDateProvider: Send + Sync {
    async fn today(&self) -> Result<BusinessDate, BusinessDateError>;
}

/// Test double: returns a preconfigured value with the caller's chosen source.
pub struct StaticBusinessDateProvider {
    pub value: NaiveDate,
    pub source: BusinessDateSource,
}

#[async_trait]
impl BusinessDateProvider for StaticBusinessDateProvider {
    async fn today(&self) -> Result<BusinessDate, BusinessDateError> {
        Ok(BusinessDate {
            date: self.value,
            source: self.source,
            resolved_at: Utc::now(),
        })
    }
}

/// Reads `SELECT date FROM m_business_date WHERE type = 'BUSINESS_DATE'` from
/// the Fineract read replica. Falls back to the wall clock if the row is
/// missing or the query fails; the caller (typically the auditing wrapper)
/// is responsible for turning the fallback into a management audit event.
pub struct FineractBusinessDateProvider {
    pool: PgPool,
}

impl FineractBusinessDateProvider {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BusinessDateProvider for FineractBusinessDateProvider {
    async fn today(&self) -> Result<BusinessDate, BusinessDateError> {
        let row: Option<NaiveDate> = sqlx::query_scalar(
            "SELECT date FROM m_business_date WHERE type = 'BUSINESS_DATE' LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| BusinessDateError::Query(e.into()))?;
        match row {
            Some(date) => Ok(BusinessDate {
                date,
                source: BusinessDateSource::Fineract,
                resolved_at: Utc::now(),
            }),
            None => Ok(BusinessDate {
                date: Utc::now().date_naive(),
                source: BusinessDateSource::WallClockFallback,
                resolved_at: Utc::now(),
            }),
        }
    }
}

/// Wraps any provider and, whenever it returns `WallClockFallback`, enqueues a
/// `business_date.fallback_used` management audit event in its own short
/// transaction. Never fails the underlying lookup on audit-write errors —
/// audit is best-effort; correctness of "today" is not.
pub struct AuditingBusinessDateProvider<P: BusinessDateProvider> {
    inner: P,
    app_pool: PgPool,
}

impl<P: BusinessDateProvider> AuditingBusinessDateProvider<P> {
    pub fn new(inner: P, app_pool: PgPool) -> Self {
        Self { inner, app_pool }
    }
}

#[async_trait]
impl<P: BusinessDateProvider> BusinessDateProvider for AuditingBusinessDateProvider<P> {
    async fn today(&self) -> Result<BusinessDate, BusinessDateError> {
        let result = self.inner.today().await?;
        if matches!(result.source, BusinessDateSource::WallClockFallback)
            && let Err(error) = enqueue_fallback_event(&self.app_pool, result).await
        {
            tracing::warn!(
                error = %error,
                "business_date.fallback_used audit enqueue failed; continuing"
            );
        }
        Ok(result)
    }
}

async fn enqueue_fallback_event(pool: &PgPool, resolved: BusinessDate) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let event = ManagementAuditEvent {
        aggregate_type: AuditAggregateType::Management,
        aggregate_id: Uuid::nil(),
        job_id: None,
        session_id: None,
        actor_user_id: None,
        event_type: AuditEventType::BusinessDateFallback,
        outcome: AuditOutcome::Success,
        summary: AuditSummary::BusinessDateFallback {
            resolved_date: SafeIdentifier::try_from(resolved.date.to_string())
                .map_err(|_| anyhow::anyhow!("resolved date failed safe identifier check"))?,
        },
        sanitized_error: None,
        occurred_at: resolved.resolved_at,
    };
    enqueue(&mut tx, event).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn static_provider_returns_configured_date_and_source() {
        let provider = StaticBusinessDateProvider {
            value: NaiveDate::from_ymd_opt(2026, 7, 24).unwrap(),
            source: BusinessDateSource::Fineract,
        };
        let result = provider.today().await.unwrap();
        assert_eq!(result.date, NaiveDate::from_ymd_opt(2026, 7, 24).unwrap());
        assert!(matches!(result.source, BusinessDateSource::Fineract));
    }

    #[tokio::test]
    async fn static_provider_can_signal_wall_clock_fallback() {
        let provider = StaticBusinessDateProvider {
            value: NaiveDate::from_ymd_opt(2026, 7, 24).unwrap(),
            source: BusinessDateSource::WallClockFallback,
        };
        assert!(matches!(
            provider.today().await.unwrap().source,
            BusinessDateSource::WallClockFallback
        ));
    }
}
