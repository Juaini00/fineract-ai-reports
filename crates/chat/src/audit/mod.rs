pub mod llm_trace_repository;
mod repository;

pub use llm_trace_repository::{LlmTrace, LlmTraceRecord, LlmTraceRepository};

use std::time::Duration;

use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::mpsc;
use tracing::warn;
use uuid::Uuid;

use repository::AuditRepository;

const AUDIT_QUEUE_CAPACITY: usize = 1024;
const AUDIT_BATCH_SIZE: usize = 50;
const AUDIT_FLUSH_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEvent {
    pub job_id: Uuid,
    pub session_id: Option<Uuid>,
    pub user_id: Uuid,
    pub legacy_api_key_id: Option<Uuid>,
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
    pub fn new(user_id: Uuid, job_id: Uuid, stage: &str, layer: &str, status: &str) -> Self {
        Self {
            job_id,
            session_id: None,
            user_id,
            legacy_api_key_id: None,
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
    tokio::spawn(run_audit_worker(AuditRepository::new(pool), receiver));
    AuditHandle {
        sender: Some(sender),
    }
}

async fn run_audit_worker(repository: AuditRepository, mut receiver: mpsc::Receiver<AuditEvent>) {
    let mut batch = Vec::with_capacity(AUDIT_BATCH_SIZE);
    let mut interval = tokio::time::interval(AUDIT_FLUSH_INTERVAL);

    loop {
        tokio::select! {
            event = receiver.recv() => {
                match event {
                    Some(event) => {
                        batch.push(event);
                        if batch.len() >= AUDIT_BATCH_SIZE {
                            flush(&repository, &mut batch).await;
                        }
                    }
                    None => {
                        flush(&repository, &mut batch).await;
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                flush(&repository, &mut batch).await;
            }
        }
    }
}

async fn flush(repository: &AuditRepository, batch: &mut Vec<AuditEvent>) {
    if batch.is_empty() {
        return;
    }
    let events = std::mem::take(batch);
    if let Err(error) = repository.insert_batch(&events).await {
        warn!(error = %error, count = events.len(), "audit batch insert failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_handle_drops_without_panic() {
        let handle = AuditHandle::new_disabled();
        handle.record(AuditEvent::new(
            Uuid::nil(),
            Uuid::nil(),
            "request_received",
            "http",
            "completed",
        ));
    }

    #[test]
    fn event_new_uses_safe_defaults() {
        let event = AuditEvent::new(
            Uuid::nil(),
            Uuid::nil(),
            "policy_evaluated",
            "policy",
            "completed",
        );
        assert_eq!(event.event_type, "pipeline");
        assert_eq!(event.stage, "policy_evaluated");
        assert_eq!(event.layer, "policy");
        assert_eq!(event.user_id, Uuid::nil());
        assert!(event.legacy_api_key_id.is_none());
        assert_eq!(event.input_summary_json, json!({}));
        assert!(event.error_json.is_none());
    }
}
