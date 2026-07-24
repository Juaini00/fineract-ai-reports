use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::api::dto::management::{
    AttentionItem, AttentionSeverity, AuditEventResponse, AuditQuery, CatalogStatusResponse,
    DashboardDailyActivity, DashboardJobSummary, DashboardKnowledgeSummary, DashboardLlmUsage,
    DashboardRange, DashboardResponse, IndexStatusResponse, KnowledgeQuery, KnowledgeStatus,
    LlmGroupBy, LlmUsageQuery, ManagementFeaturesResponse, ManagementStatusResponse,
    ProviderStatusResponse, TelemetryStatusResponse, WarningCode,
};
use crate::knowledge::model::KnowledgeCatalog;

use super::audit::ManagementAuditRepository;
use super::knowledge::KnowledgeService;
use super::repository::outbox_health;
use super::usage::LlmUsageRepository;

/// Composes the `/management/dashboard` response by reusing the same
/// repositories the other management endpoints already trust.
pub struct DashboardService {
    pool: PgPool,
    catalog: Arc<KnowledgeCatalog>,
    provider_name: String,
    provider_model: String,
}

impl DashboardService {
    pub fn new(
        pool: PgPool,
        catalog: Arc<KnowledgeCatalog>,
        provider_name: String,
        provider_model: String,
    ) -> Self {
        Self {
            pool,
            catalog,
            provider_name,
            provider_model,
        }
    }

    pub async fn snapshot(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<DashboardResponse> {
        let generated_at = Utc::now();
        let knowledge = KnowledgeService::new(self.catalog.clone());
        let health = outbox_health(&self.pool).await?;

        let jobs = self.job_summary(from, to).await?;
        let activity_by_day = self.daily_activity(from, to).await?;
        let llm_usage = self.llm_usage_totals(from, to).await?;
        let knowledge_summary = self.knowledge_summary(&knowledge);
        let recent_audit_events = self.recent_audit_events(from, to).await?;

        let catalog_status_hash = knowledge.catalog_version();
        let catalog_status_validation = "valid";
        let audit_status = if health.exhausted > 0 || health.pending > 0 {
            "delayed"
        } else {
            "healthy"
        };
        let status = ManagementStatusResponse {
            provider: ProviderStatusResponse {
                name: self.provider_name.clone(),
                model: self.provider_model.clone(),
            },
            catalog: CatalogStatusResponse {
                content_hash: catalog_status_hash.clone(),
                validation_status: catalog_status_validation,
            },
            index: IndexStatusResponse {
                status: "unavailable",
                version_id: None,
            },
            audit: crate::api::dto::management::AuditStatusResponse {
                decision_audit_status: audit_status,
                telemetry: TelemetryStatusResponse {
                    dropped_events: 0,
                    last_persisted_at: None,
                },
            },
            features: ManagementFeaturesResponse {
                reference_knowledge: false,
                cost_warnings: true,
            },
        };

        let attention_items =
            attention_from_state(&health, &llm_usage, catalog_status_validation, generated_at);

        Ok(DashboardResponse {
            range: DashboardRange { from, to },
            generated_at,
            status,
            jobs,
            activity_by_day,
            llm_usage,
            knowledge: knowledge_summary,
            recent_audit_events,
            attention_items,
        })
    }

    async fn job_summary(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<DashboardJobSummary> {
        let row: (i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT \
                COUNT(*) FILTER (WHERE created_at >= $1 AND created_at < $2)::bigint, \
                COUNT(*) FILTER (WHERE completed_at >= $1 AND completed_at < $2)::bigint, \
                COUNT(*) FILTER (WHERE failed_at >= $1 AND failed_at < $2)::bigint, \
                COUNT(*) FILTER (WHERE status = 'waiting_for_user_input')::bigint, \
                COUNT(*) FILTER (WHERE status IN ('queued','running'))::bigint \
             FROM chat_jobs",
        )
        .bind(from)
        .bind(to)
        .fetch_one(&self.pool)
        .await?;
        let blocked: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT job_id) FROM management_audit_events \
             WHERE outcome = 'blocked' AND occurred_at >= $1 AND occurred_at < $2 AND job_id IS NOT NULL",
        )
        .bind(from)
        .bind(to)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        Ok(DashboardJobSummary {
            created: row.0,
            completed: row.1,
            failed: row.2,
            blocked,
            awaiting_clarification: row.3,
            active: row.4,
        })
    }

    async fn daily_activity(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<DashboardDailyActivity>> {
        let rows: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
            "WITH days AS (\
                SELECT generate_series(\
                    date_trunc('day', $1::timestamptz),\
                    date_trunc('day', $2::timestamptz - interval '1 microsecond'),\
                    interval '1 day'\
                ) AS d\
            )\
            SELECT to_char(days.d, 'YYYY-MM-DD') AS date, \
                COALESCE(SUM(CASE WHEN j.created_at   >= days.d AND j.created_at   < days.d + interval '1 day' AND j.created_at   >= $1 AND j.created_at   < $2 THEN 1 ELSE 0 END), 0)::bigint AS created, \
                COALESCE(SUM(CASE WHEN j.completed_at >= days.d AND j.completed_at < days.d + interval '1 day' AND j.completed_at >= $1 AND j.completed_at < $2 THEN 1 ELSE 0 END), 0)::bigint AS completed, \
                COALESCE(SUM(CASE WHEN j.failed_at    >= days.d AND j.failed_at    < days.d + interval '1 day' AND j.failed_at    >= $1 AND j.failed_at    < $2 THEN 1 ELSE 0 END), 0)::bigint AS failed, \
                COALESCE(SUM(CASE WHEN b.job_id IS NOT NULL AND b.occurred_at >= days.d AND b.occurred_at < days.d + interval '1 day' AND b.occurred_at >= $1 AND b.occurred_at < $2 THEN 1 ELSE 0 END), 0)::bigint AS blocked \
            FROM days \
            LEFT JOIN chat_jobs j ON TRUE \
            LEFT JOIN LATERAL (\
                SELECT DISTINCT ON (job_id) job_id, occurred_at FROM management_audit_events \
                WHERE outcome = 'blocked' AND job_id = j.id\
            ) b ON TRUE \
            GROUP BY days.d ORDER BY days.d",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(date, created, completed, failed, blocked)| DashboardDailyActivity {
                    date,
                    created,
                    completed,
                    failed,
                    blocked,
                },
            )
            .collect())
    }

    async fn llm_usage_totals(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<DashboardLlmUsage> {
        // Reuse the existing repository with a coarse grouping, then fold the
        // rows into a single totals payload so the dashboard cannot diverge
        // from `/management/llm-usage` semantics.
        let usage = LlmUsageRepository::new(self.pool.clone())
            .aggregate(&LlmUsageQuery {
                from,
                to,
                group_by: LlmGroupBy::Status,
            })
            .await?;
        let mut totals = DashboardLlmUsage {
            calls: 0,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            unknown_usage_calls: 0,
            errors: 0,
            p95_latency_ms: None,
            estimated_cost: None,
            warnings: usage.warnings.clone(),
        };
        let mut any_input = false;
        let mut any_output = false;
        let mut any_total = false;
        let mut latency_max: Option<i64> = None;
        for group in usage.groups.iter() {
            totals.calls += group.calls;
            totals.unknown_usage_calls += group.unknown_usage_calls;
            totals.errors += group.errors;
            if let Some(v) = group.input_tokens {
                totals.input_tokens = Some(totals.input_tokens.unwrap_or(0) + v);
                any_input = true;
            }
            if let Some(v) = group.output_tokens {
                totals.output_tokens = Some(totals.output_tokens.unwrap_or(0) + v);
                any_output = true;
            }
            if let Some(v) = group.total_tokens {
                totals.total_tokens = Some(totals.total_tokens.unwrap_or(0) + v);
                any_total = true;
            }
            if let Some(v) = group.p95_latency_ms {
                latency_max = Some(latency_max.map(|prev| prev.max(v)).unwrap_or(v));
            }
        }
        if !any_input {
            totals.input_tokens = None;
        }
        if !any_output {
            totals.output_tokens = None;
        }
        if !any_total {
            totals.total_tokens = None;
        }
        totals.p95_latency_ms = latency_max;
        if totals.unknown_usage_calls > 0 && !totals.warnings.contains(&WarningCode::UsageMissing) {
            totals.warnings.push(WarningCode::UsageMissing);
        }
        Ok(totals)
    }

    fn knowledge_summary(&self, service: &KnowledgeService) -> DashboardKnowledgeSummary {
        let list = service
            .list(&KnowledgeQuery {
                kind: None,
                status: None,
                domain_id: None,
                cursor: None,
                limit: Some(1000),
            })
            .ok();
        let mut total = 0i64;
        let mut available = 0i64;
        let mut deferred = 0i64;
        let mut unavailable = 0i64;
        let mut domains: std::collections::BTreeSet<String> = Default::default();
        if let Some(list) = list.as_ref() {
            for item in list.items.iter() {
                total += 1;
                domains.insert(item.domain_id.clone());
                match item.status {
                    KnowledgeStatus::Available => available += 1,
                    KnowledgeStatus::Deferred => deferred += 1,
                    KnowledgeStatus::Unavailable => unavailable += 1,
                }
            }
        }
        DashboardKnowledgeSummary {
            total,
            available,
            deferred,
            unavailable,
            domains: domains.len() as i64,
            catalog_version: service.catalog_version(),
            index_version: list.and_then(|l| l.index_version),
        }
    }

    async fn recent_audit_events(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<AuditEventResponse>> {
        let list = ManagementAuditRepository::new(self.pool.clone())
            .list(&AuditQuery {
                from,
                to,
                event_type: None,
                outcome: None,
                job_id: None,
                session_id: None,
                cursor: None,
                limit: Some(10),
            })
            .await
            .map_err(|error| match error {
                super::audit::AuditLookupError::Internal(error) => error,
                super::audit::AuditLookupError::InvalidCursor => {
                    anyhow::anyhow!("recent audit slice built a bad cursor")
                }
            })?;
        Ok(list.items)
    }
}

fn attention_from_state(
    health: &super::repository::OutboxHealth,
    llm: &DashboardLlmUsage,
    catalog_validation: &str,
    now: DateTime<Utc>,
) -> Vec<AttentionItem> {
    let mut items: Vec<AttentionItem> = Vec::new();
    if health.exhausted > 0 {
        items.push(AttentionItem {
            id: format!("audit_delayed:global:{}", now.date_naive()),
            kind: "audit_delayed".into(),
            severity: AttentionSeverity::Critical,
            message: "Audit outbox has exhausted retry attempts.".into(),
            occurred_at: now,
            resource: None,
        });
    } else if health.pending > 0 {
        items.push(AttentionItem {
            id: format!("audit_delayed:global:{}", now.date_naive()),
            kind: "audit_delayed".into(),
            severity: AttentionSeverity::Warning,
            message: "Audit outbox has a pending backlog.".into(),
            occurred_at: now,
            resource: None,
        });
    }
    if catalog_validation != "valid" {
        items.push(AttentionItem {
            id: format!("catalog_invalid:global:{}", now.date_naive()),
            kind: "catalog_invalid".into(),
            severity: AttentionSeverity::Critical,
            message: "Loaded catalog is not valid.".into(),
            occurred_at: now,
            resource: None,
        });
    }
    if llm.errors > 0 && llm.calls > 0 {
        let rate = (llm.errors as f64) / (llm.calls as f64);
        if rate >= 0.20 {
            items.push(AttentionItem {
                id: format!("llm_error_rate_high:global:{}", now.date_naive()),
                kind: "llm_error_rate_high".into(),
                severity: AttentionSeverity::Warning,
                message: "LLM error rate is elevated.".into(),
                occurred_at: now,
                resource: None,
            });
        }
    }
    if llm.unknown_usage_calls > 0 {
        items.push(AttentionItem {
            id: format!("usage_missing:global:{}", now.date_naive()),
            kind: "usage_missing".into(),
            severity: AttentionSeverity::Info,
            message: "One or more LLM calls have unknown token usage.".into(),
            occurred_at: now,
            resource: None,
        });
    }
    items.truncate(10);
    items
}
