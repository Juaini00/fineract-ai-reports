use std::time::Duration;

use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::mpsc;
use tracing::warn;
use uuid::Uuid;

const AUDIT_QUEUE_CAPACITY: usize = 1024;
const AUDIT_BATCH_SIZE: usize = 50;
const AUDIT_FLUSH_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEvent {
    pub job_id: Uuid,
    pub session_id: Option<Uuid>,
    pub api_key_id: Option<Uuid>,
    pub event_type: String,
    pub stage: String,
    pub layer: String,
    pub blueprint_step: Option<String>,
    pub status: String,
    pub duration_ms: Option<i64>,
    pub input_summary_json: Value,
    pub output_summary_json: Value,
    pub decision_json: Value,
    pub flags_json: Value,
    pub error_json: Option<Value>,
}

#[derive(Clone)]
pub struct AuditHandle {
    sender: Option<mpsc::Sender<AuditEvent>>,
}

impl AuditHandle {
    pub fn new_disabled() -> Self {
        Self { sender: None }
    }

    pub fn record(&self, event: AuditEvent) {
        let Some(sender) = &self.sender else {
            return;
        };
        if let Err(error) = sender.try_send(event) {
            warn!(error = %error, "audit event dropped");
        }
    }
}

impl AuditEvent {
    pub fn new(job_id: Uuid, stage: &str, layer: &str, status: &str) -> Self {
        Self {
            job_id,
            session_id: None,
            api_key_id: None,
            event_type: "pipeline".to_string(),
            stage: stage.to_string(),
            layer: layer.to_string(),
            blueprint_step: None,
            status: status.to_string(),
            duration_ms: None,
            input_summary_json: json!({}),
            output_summary_json: json!({}),
            decision_json: json!({}),
            flags_json: json!({}),
            error_json: None,
        }
    }
}

pub fn spawn_audit_worker(pool: PgPool) -> AuditHandle {
    let (sender, receiver) = mpsc::channel(AUDIT_QUEUE_CAPACITY);
    tokio::spawn(run_audit_worker(pool, receiver));
    AuditHandle {
        sender: Some(sender),
    }
}

async fn run_audit_worker(pool: PgPool, mut receiver: mpsc::Receiver<AuditEvent>) {
    let mut batch = Vec::with_capacity(AUDIT_BATCH_SIZE);
    let mut interval = tokio::time::interval(AUDIT_FLUSH_INTERVAL);

    loop {
        tokio::select! {
            event = receiver.recv() => {
                match event {
                    Some(event) => {
                        batch.push(event);
                        if batch.len() >= AUDIT_BATCH_SIZE {
                            flush(&pool, &mut batch).await;
                        }
                    }
                    None => {
                        flush(&pool, &mut batch).await;
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                flush(&pool, &mut batch).await;
            }
        }
    }
}

async fn flush(pool: &PgPool, batch: &mut Vec<AuditEvent>) {
    if batch.is_empty() {
        return;
    }
    let events = std::mem::take(batch);
    if let Err(error) = insert_batch(pool, &events).await {
        warn!(error = %error, count = events.len(), "audit batch insert failed");
    }
}

async fn insert_batch(pool: &PgPool, events: &[AuditEvent]) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for event in events {
        sqlx::query(
            r#"
            INSERT INTO chat_job_audit_events (
                id, job_id, session_id, api_key_id, event_type, stage, layer,
                blueprint_step, status, duration_ms, input_summary_json,
                output_summary_json, decision_json, flags_json, error_json
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(event.job_id)
        .bind(event.session_id)
        .bind(event.api_key_id)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_handle_drops_without_panic() {
        let handle = AuditHandle::new_disabled();
        handle.record(AuditEvent::new(
            Uuid::nil(),
            "request_received",
            "http",
            "completed",
        ));
    }

    #[test]
    fn event_new_uses_safe_defaults() {
        let event = AuditEvent::new(Uuid::nil(), "policy_evaluated", "policy", "completed");
        assert_eq!(event.event_type, "pipeline");
        assert_eq!(event.stage, "policy_evaluated");
        assert_eq!(event.layer, "policy");
        assert_eq!(event.input_summary_json, json!({}));
        assert!(event.error_json.is_none());
    }
}
