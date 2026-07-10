# Event-Driven Audit Trail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a durable, non-blocking audit trail for chat job pipeline stages.

**Architecture:** Add an append-only `chat_job_audit_events` table, a small `chat::audit` module, and a bounded `tokio::sync::mpsc` queue with one background batch writer. The main job pipeline only calls `AuditHandle::record`, which uses `try_send` and never waits on audit DB writes.

**Tech Stack:** Rust, Tokio `mpsc`, SQLx, PostgreSQL JSONB, Axum app composition, existing `tracing`, no new third-party crates.

## Global Constraints

- Workspace crates remain exactly `app`, `core`, and `chat`.
- Do not add new dependencies for audit queueing.
- Schema changes belong in `migrations/*.sql`; do not create/alter tables from app startup code.
- Audit writes must not fail or delay the main chat job pipeline.
- Do not store raw API keys, authorization headers, raw embeddings, hidden prompts, full SQL result rows, or secret config values in audit events.
- First implementation scope is chat job pipeline only; do not audit `/health`, `/ready`, or simple status endpoints yet.
- Use Ponytail: smallest correct implementation, no speculative admin dashboard or zero-loss queue.

---

## File Structure

- Create `migrations/20260709040000_create_chat_job_audit_events.sql`: app DB schema for append-only audit rows and indexes.
- Create `crates/chat/src/audit.rs`: `AuditEvent`, `AuditHandle`, `spawn_audit_worker`, batch insert helper, and unit tests for non-blocking queue behavior.
- Modify `crates/chat/src/lib.rs`: expose `pub mod audit`.
- Modify `crates/chat/src/api/mod.rs`: construct one audit handle during chat state startup and pass it into `JobService`.
- Modify `crates/chat/src/chat/service/job.rs`: add `audit: AuditHandle` to `JobService`, emit audit events at key pipeline stages, keep events sanitized and small.
- Modify `crates/chat/src/chat/service/job/tests.rs` only if constructor tests need the new `AuditHandle` dependency.

---

### Task 1: Migration For Audit Events

**Files:**
- Create: `migrations/20260709040000_create_chat_job_audit_events.sql`

**Interfaces:**
- Produces: PostgreSQL table `chat_job_audit_events` consumed by `chat::audit::insert_batch` in Task 2.

- [ ] **Step 1: Create the migration**

Use `apply_patch` to add:

```sql
CREATE TABLE chat_job_audit_events (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id),
    session_id UUID NULL REFERENCES chat_sessions(id),
    api_key_id UUID NULL REFERENCES api_keys(id),
    event_type TEXT NOT NULL,
    stage TEXT NOT NULL,
    layer TEXT NOT NULL,
    blueprint_step TEXT NULL,
    status TEXT NOT NULL,
    duration_ms BIGINT NULL,
    input_summary_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    output_summary_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    decision_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    flags_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    error_json JSONB NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_chat_job_audit_events_job_id
    ON chat_job_audit_events(job_id, created_at);

CREATE INDEX idx_chat_job_audit_events_stage
    ON chat_job_audit_events(stage, created_at);

CREATE INDEX idx_chat_job_audit_events_blueprint_step
    ON chat_job_audit_events(blueprint_step, created_at);

CREATE INDEX idx_chat_job_audit_events_api_key_id
    ON chat_job_audit_events(api_key_id, created_at);
```

- [ ] **Step 2: Verify migration compiles structurally**

Run:

```bash
cargo check -p chat
```

Expected: command exits 0. This does not apply the migration, but catches workspace breakage before Rust changes.

---

### Task 2: Audit Module And Non-Blocking Queue

**Files:**
- Create: `crates/chat/src/audit.rs`
- Modify: `crates/chat/src/lib.rs`

**Interfaces:**
- Produces: `AuditEvent`, `AuditHandle::new_disabled()`, `AuditHandle::record(AuditEvent)`, `spawn_audit_worker(PgPool) -> AuditHandle`.
- Consumes: SQL table from Task 1.

- [ ] **Step 1: Add the audit module skeleton and tests first**

Use `apply_patch` to create `crates/chat/src/audit.rs` with this testable structure:

```rust
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
    AuditHandle { sender: Some(sender) }
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
        handle.record(AuditEvent::new(Uuid::nil(), "request_received", "http", "completed"));
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
```

- [ ] **Step 2: Expose the module**

Modify `crates/chat/src/lib.rs`:

```rust
pub mod api;
pub mod audit;
pub mod chat;
pub mod knowledge;
pub mod policy;
```

- [ ] **Step 3: Run focused tests**

Run:

```bash
cargo test -p chat audit::tests
```

Expected: audit module tests pass.

---

### Task 3: Wire Audit Worker Into Chat State

**Files:**
- Modify: `crates/chat/src/api/mod.rs`
- Modify: `crates/chat/src/chat/service/job.rs`

**Interfaces:**
- Consumes: `chat::audit::{AuditHandle, spawn_audit_worker}` from Task 2.
- Produces: `JobService::new(..., audit: AuditHandle)` constructor dependency used by Task 4.

- [ ] **Step 1: Modify `JobService` constructor shape**

In `crates/chat/src/chat/service/job.rs`, add imports and field:

```rust
use crate::audit::{AuditEvent, AuditHandle};
```

Add to `JobService`:

```rust
    audit: AuditHandle,
```

Add constructor parameter after `redis: Option<redis::Client>`:

```rust
        audit: AuditHandle,
```

Add field initialization:

```rust
            audit,
```

- [ ] **Step 2: Spawn the worker once in chat app state**

In `crates/chat/src/api/mod.rs`, add:

```rust
use crate::audit::spawn_audit_worker;
```

Before constructing `ChatServices`, add:

```rust
        let audit = spawn_audit_worker(core.pools.app.clone());
```

Pass it to `JobService::new` after Redis:

```rust
                core.pools.redis.clone(),
                audit,
```

- [ ] **Step 3: Fix tests that construct `JobService` directly**

Search:

```bash
rg "JobService::new" crates/chat/src crates/chat/tests
```

For test constructors only, pass:

```rust
chat::audit::AuditHandle::new_disabled()
```

- [ ] **Step 4: Run compile check**

Run:

```bash
cargo check -p chat
```

Expected: command exits 0.

---

### Task 4: Emit Core Pipeline Audit Events

**Files:**
- Modify: `crates/chat/src/chat/service/job.rs`

**Interfaces:**
- Consumes: `AuditEvent::new`, `AuditHandle::record` from Task 2.
- Produces: audit rows for create, classification, plan, policy, execution, formatting, and terminal outcomes.

- [ ] **Step 1: Add a local helper for common job metadata**

Inside `impl JobService`, add:

```rust
    fn audit_event(
        &self,
        session_id: Uuid,
        job_id: Uuid,
        stage: &str,
        layer: &str,
        blueprint_step: Option<&str>,
        status: &str,
    ) -> AuditEvent {
        let mut event = AuditEvent::new(job_id, stage, layer, status);
        event.session_id = Some(session_id);
        event.blueprint_step = blueprint_step.map(str::to_string);
        event
    }
```

- [ ] **Step 2: Record request/job creation**

After `self.jobs.create(...).await?` returns `job` in `create`, add:

```rust
        let mut audit = self.audit_event(
            job.session_id,
            job.job_id,
            "request_received",
            "api",
            Some("conversation_context"),
            "completed",
        );
        audit.api_key_id = Some(input.client.api_key_id);
        audit.input_summary_json = json!({
            "message_len": worker_message.len(),
            "has_session_id": input.session_id.is_some(),
        });
        audit.output_summary_json = json!({
            "current_step": job.current_step,
            "status": job.status,
        });
        self.audit.record(audit);
```

- [ ] **Step 3: Record production strict parser gap once per job**

Before spawning `run_pipeline` in `create`, add:

```rust
        let mut skipped = self.audit_event(
            job.session_id,
            job.job_id,
            "semantic_parser",
            "pipeline",
            Some("semantic_parser"),
            "skipped",
        );
        skipped.api_key_id = Some(input.client.api_key_id);
        skipped.decision_json = json!({
            "reason": "strict_pipeline_not_used_in_production"
        });
        skipped.flags_json = json!({
            "blueprint_deviation": true
        });
        self.audit.record(skipped);
```

- [ ] **Step 4: Record classification result at pipeline start**

At the top of `run_pipeline`, before the clarification check, add:

```rust
        let mut audit = self.audit_event(
            session_id,
            job_id,
            "classification_completed",
            "classification",
            Some("intent_router"),
            "completed",
        );
        audit.output_summary_json = json!({
            "outcome": classification.outcome,
            "domain": classification.domain,
            "capability": classification.capability,
            "confidence": classification.confidence,
            "source": classification.source,
            "candidate_count": classification.candidates.len(),
            "layer_count": classification.layers.len(),
        });
        audit.flags_json = json!({
            "used_lqr": classification.source.as_deref() == Some("lqr"),
            "used_llm": classification.source.as_deref() == Some("llm_planner")
        });
        self.audit.record(audit);
```

- [ ] **Step 5: Record clarification branch**

Inside `if classification.outcome == ClassificationOutcome::ClarificationRequired`, before `write_clarification`, add:

```rust
            let mut audit = self.audit_event(
                session_id,
                job_id,
                "clarification_required",
                "classification",
                Some("ambiguity_detector"),
                "completed",
            );
            audit.output_summary_json = json!({
                "option_count": classification.options.len(),
                "has_question": classification.clarification.is_some(),
            });
            self.audit.record(audit);
```

- [ ] **Step 6: Record plan and policy decision**

Inside the `if let Some(plan) = execution_plan.as_ref()` branch before execution, add:

```rust
            let mut plan_audit = self.audit_event(
                session_id,
                job_id,
                "execution_plan_built",
                "planner",
                Some("answer_planner"),
                "completed",
            );
            plan_audit.output_summary_json = json!({
                "domain": plan.domain,
                "capability": plan.capability,
                "query_id": plan.query_id,
                "output_mode": plan.output_mode,
                "evidence_enough": plan.evidence_evaluation.enough,
            });
            self.audit.record(plan_audit);

            let mut policy_audit = self.audit_event(
                session_id,
                job_id,
                "policy_evaluated",
                "policy",
                Some("evidence_evaluator"),
                if matches!(policy_decision.status, crate::chat::planner::PolicyDecisionStatus::Allowed) {
                    "completed"
                } else {
                    "blocked"
                },
            );
            policy_audit.decision_json = json!({
                "status": policy_decision.status,
                "reason": policy_decision.reason,
                "office_count": policy_decision.office_ids.len(),
                "can_view_pii": policy_decision.can_view_pii,
            });
            policy_audit.flags_json = json!({
                "policy_blocked": !matches!(policy_decision.status, crate::chat::planner::PolicyDecisionStatus::Allowed),
                "authorized_scope_only": true,
                "pii_output_allowed": policy_decision.can_view_pii,
            });
            self.audit.record(policy_audit);
```

- [ ] **Step 7: Record terminal unsupported branch**

Inside the unsupported branch before `fail_unsupported`, add:

```rust
            let mut audit = self.audit_event(
                session_id,
                job_id,
                "job_failed",
                "pipeline",
                Some("grounded_response"),
                "failed",
            );
            audit.error_json = Some(json!({
                "code": "unsupported_request"
            }));
            self.audit.record(audit);
```

- [ ] **Step 8: Run focused service tests**

Run:

```bash
cargo test -p chat chat::service::job::tests
```

Expected: existing job service tests pass after constructor updates.

---

### Task 5: Emit SQL, Formatter, LLM, And Terminal Audit Events

**Files:**
- Modify: `crates/chat/src/chat/service/job.rs`

**Interfaces:**
- Consumes: helper `audit_event` from Task 4.
- Produces: execution and response audit events with duration and row count.

- [ ] **Step 1: Record SQL selected before execution**

At the start of `execute_and_finish`, before `execute_plan`, add:

```rust
        let mut selected = self.audit_event(
            session_id,
            job_id,
            "sql_selected",
            "executor",
            Some("hybrid_retrieval"),
            "completed",
        );
        selected.output_summary_json = json!({
            "query_id": plan.query_id,
            "capability": plan.capability,
            "office_count": policy_decision.office_ids.len(),
        });
        self.audit.record(selected);
```

- [ ] **Step 2: Record successful SQL execution**

Inside the success branch after `latency_ms` and `row_count` are computed, add:

```rust
                let mut executed = self.audit_event(
                    session_id,
                    job_id,
                    "sql_executed",
                    "executor",
                    Some("hybrid_retrieval"),
                    "completed",
                );
                executed.duration_ms = Some(latency_ms as i64);
                executed.output_summary_json = json!({
                    "query_id": plan.query_id,
                    "row_count": row_count,
                });
                self.audit.record(executed);
```

- [ ] **Step 3: Record response formatting**

After `format_report_response` returns `Some(content)`, before `apply_llm_answer`, add:

```rust
                    let mut formatted = self.audit_event(
                        session_id,
                        job_id,
                        "response_formatted",
                        "formatter",
                        Some("grounded_response"),
                        "completed",
                    );
                    formatted.output_summary_json = json!({
                        "content_len": content.len(),
                        "query_id": plan.query_id,
                    });
                    self.audit.record(formatted);
```

- [ ] **Step 4: Record final completion**

After `self.jobs.complete(job_id, result).await?;`, add:

```rust
                let mut completed = self.audit_event(
                    session_id,
                    job_id,
                    "job_completed",
                    "pipeline",
                    Some("grounded_response"),
                    "completed",
                );
                completed.duration_ms = Some(latency_ms as i64);
                completed.output_summary_json = json!({
                    "row_count": row_count,
                    "status": "completed",
                });
                self.audit.record(completed);
```

- [ ] **Step 5: Record execution failure**

Inside the `Err(error)` branch after `latency_ms` is computed, add:

```rust
                let mut failed = self.audit_event(
                    session_id,
                    job_id,
                    "job_failed",
                    "executor",
                    Some("grounded_response"),
                    "failed",
                );
                failed.duration_ms = Some(latency_ms as i64);
                failed.error_json = Some(json!({
                    "code": "execution_failed",
                    "message": "Report execution failed."
                }));
                self.audit.record(failed);
```

- [ ] **Step 6: Keep LLM answer audit minimal**

Do not pass `job_id` into `apply_llm_answer` in this slice. That keeps the diff small. LLM usage is already visible from classification source and final response path. Add LLM-specific audit in a later slice if management needs per-answer LLM latency.

- [ ] **Step 7: Run focused tests**

Run:

```bash
cargo test -p chat chat::service::job::tests
```

Expected: job service tests pass.

---

### Task 6: Verification And Documentation Sync

**Files:**
- Modify only if needed: `docs/audit-trail-design.md`, `docs/chat-data-model.md`, `docs/implementation-steps.md`

**Interfaces:**
- Consumes: implementation from Tasks 1-5.
- Produces: verified code and docs aligned with implemented event names.

- [ ] **Step 1: Verify audit event names match docs**

Search:

```bash
rg "request_received|semantic_parser|classification_completed|policy_evaluated|sql_selected|sql_executed|response_formatted|job_completed|job_failed" crates/chat/src docs/audit-trail-design.md
```

Expected: docs and code use the same stage names.

- [ ] **Step 2: Run package tests**

Run:

```bash
cargo test -p chat
```

Expected: tests pass, except ignored tests remain ignored.

- [ ] **Step 3: Run full check**

Run:

```bash
cargo check
```

Expected: workspace check exits 0.

- [ ] **Step 4: Inspect intended diff only**

Run:

```bash
git diff -- migrations/20260709040000_create_chat_job_audit_events.sql crates/chat/src/audit.rs crates/chat/src/lib.rs crates/chat/src/api/mod.rs crates/chat/src/chat/service/job.rs docs/audit-trail-design.md docs/chat-data-model.md docs/implementation-steps.md
```

Expected: diff only contains audit-trail implementation and docs alignment.

---

## Self-Review

Spec coverage:

- Durable audit table: Task 1.
- Non-blocking event-driven writer: Task 2.
- No third-party dependency: Global constraints and Task 2 use only Tokio/SQLx.
- Chat pipeline first scope: Tasks 3-5 wire only `JobService`.
- Blueprint skipped-stage observability: Task 4 emits `semantic_parser` skipped event.
- Safe payload rules: Global constraints and event snippets store summaries only.

Placeholder scan:

- No incomplete placeholder instructions are used in executable task instructions.

Type consistency:

- `AuditHandle`, `AuditEvent`, and `spawn_audit_worker` are defined in Task 2 and consumed by later tasks.
- `AuditEvent::new(Uuid, &str, &str, &str)` signature is consistent across tasks.
