use sqlx::PgPool;
use uuid::Uuid;

use super::AuditEvent;

pub(super) struct AuditRepository {
    pool: PgPool,
}

impl AuditRepository {
    pub(super) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(super) async fn insert_batch(&self, events: &[AuditEvent]) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        for event in events {
            sqlx::query(
                r#"
            INSERT INTO chat_job_audit_events (
                id, job_id, session_id, user_id, api_key_id, event_type, stage, layer,
                blueprint_step, status, duration_ms, input_summary_json,
                output_summary_json, decision_json, flags_json, error_json
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            "#,
            )
            .bind(Uuid::new_v4())
            .bind(event.job_id)
            .bind(event.session_id)
            .bind(event.user_id)
            .bind(event.legacy_api_key_id)
            .bind(&event.event_type)
            .bind(&event.stage)
            .bind(&event.layer)
            .bind(&event.blueprint_step)
            .bind(&event.status)
            .bind(event.duration_ms)
            .bind(&event.input_summary_json)
            .bind(&event.output_summary_json)
            .bind(&event.decision_json)
            .bind(&event.flags_json)
            .bind(&event.error_json)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
