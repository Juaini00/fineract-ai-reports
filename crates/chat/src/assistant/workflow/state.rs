use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::assistant::ClarificationPayload;

use super::contract::{BindingSource, ExecutionWorkflow, NodeId, NodeInput};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeRunStatus {
    Runnable,
    Running,
    Completed,
    Failed,
    Skipped,
    Waiting,
}

impl NodeRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Runnable => "runnable",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Waiting => "waiting",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowNodeRun {
    pub id: Uuid,
    pub job_id: Uuid,
    pub workflow_id: Uuid,
    pub node_id: NodeId,
    pub attempt: i16,
    pub status: NodeRunStatus,
    pub output_json: Option<Value>,
    pub provenance_json: Value,
    pub rows_returned: i32,
    pub duration_ms: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowResumeRequest {
    pub job_id: Uuid,
    pub user_id: Uuid,
    pub workflow_id: Uuid,
    pub node_id: NodeId,
    pub clarification_id: Uuid,
    pub workflow_revision: i64,
    pub selected_value: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeOutcome {
    Resumed,
    NotFound,
    NotWaiting,
    Stale,
}

#[derive(Clone)]
pub struct WorkflowStateRepository {
    pool: PgPool,
}

impl WorkflowStateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn install_workflow(
        &self,
        job_id: Uuid,
        user_id: Uuid,
        workflow: &ExecutionWorkflow,
    ) -> Result<()> {
        let workflow_json = serde_json::to_value(workflow)?;
        let changed = sqlx::query(
            r#"
            UPDATE chat_jobs
            SET workflow_id = $1,
                workflow_contract_version = $2,
                workflow_revision = workflow_revision + 1,
                current_node_id = NULL,
                current_step = 'planning',
                state_json = state_json || jsonb_build_object(
                    'workflow', $3::jsonb,
                    'workflow_runtime', jsonb_build_object(
                        'completed_node_ids', '[]'::jsonb,
                        'node_outputs', '{}'::jsonb,
                        'budget_consumed', jsonb_build_object('queries', 0, 'rows', 0, 'ms', 0)
                    )
                ),
                updated_at = now()
            WHERE id = $4 AND user_id = $5
            "#,
        )
        .bind(workflow.id)
        .bind(
            i16::try_from(workflow.contract_version)
                .map_err(|_| anyhow::anyhow!("workflow contract version is invalid"))?,
        )
        .bind(workflow_json)
        .bind(job_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        if changed.rows_affected() != 1 {
            bail!("workflow job was not found");
        }
        Ok(())
    }

    pub async fn load_workflow(
        &self,
        job_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<ExecutionWorkflow>> {
        let state: Option<Value> =
            sqlx::query_scalar("SELECT state_json FROM chat_jobs WHERE id = $1 AND user_id = $2")
                .bind(job_id)
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;
        state
            .map(|state| {
                let workflow = state
                    .get("workflow")
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("workflow is not installed"))?;
                serde_json::from_value(workflow).map_err(Into::into)
            })
            .transpose()
    }

    /// Reads back the clarification payload `mark_workflow_paused` persisted
    /// to `assistant_job_memory.pending_clarification_json` for a job that is
    /// currently `WaitingForUserInput`.
    pub async fn load_pending_clarification(
        &self,
        job_id: Uuid,
    ) -> Result<Option<ClarificationPayload>> {
        let row: Option<Option<Value>> = sqlx::query_scalar(
            "SELECT pending_clarification_json FROM assistant_job_memory WHERE job_id = $1",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;
        row.flatten()
            .map(|value| serde_json::from_value(value).map_err(Into::into))
            .transpose()
    }

    pub async fn node_runs(&self, job_id: Uuid, workflow_id: Uuid) -> Result<Vec<WorkflowNodeRun>> {
        let rows = sqlx::query_as::<_, NodeRunRow>(
            r#"
            SELECT id, job_id, workflow_id, node_id, attempt, status, output_json,
                   provenance_json, rows_returned, duration_ms, started_at, finished_at
            FROM chat_workflow_node_runs
            WHERE job_id = $1 AND workflow_id = $2
            ORDER BY started_at NULLS FIRST, id
            "#,
        )
        .bind(job_id)
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub async fn begin_node(
        &self,
        job_id: Uuid,
        workflow_id: Uuid,
        node_id: &NodeId,
        attempt: i16,
        provenance_json: Value,
    ) -> Result<WorkflowNodeRun> {
        let row = sqlx::query_as::<_, NodeRunRow>(
            r#"
            INSERT INTO chat_workflow_node_runs (
                id, job_id, workflow_id, node_id, attempt, status, provenance_json, started_at
            ) VALUES ($1, $2, $3, $4, $5, 'running', $6, now())
            RETURNING id, job_id, workflow_id, node_id, attempt, status, output_json,
                      provenance_json, rows_returned, duration_ms, started_at, finished_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(job_id)
        .bind(workflow_id)
        .bind(node_id.as_str())
        .bind(attempt)
        .bind(provenance_json)
        .fetch_one(&self.pool)
        .await?;
        let run: WorkflowNodeRun = row.try_into()?;
        self.insert_checkpoint(
            job_id,
            node_id.as_str(),
            "node_started",
            json!({
                "workflow_id": workflow_id, "node_id": node_id, "attempt": attempt,
            }),
        )
        .await?;
        self.insert_event(
            job_id,
            "workflow_node_started",
            Some(node_id.as_str()),
            json!({
                "workflow_id": workflow_id, "node_id": node_id, "attempt": attempt,
            }),
        )
        .await?;
        Ok(run)
    }

    pub async fn complete_node(
        &self,
        run: &WorkflowNodeRun,
        output_json: Value,
        rows_returned: i32,
        duration_ms: i32,
    ) -> Result<()> {
        let changed = sqlx::query(
            r#"
            UPDATE chat_workflow_node_runs
            SET status = 'completed', output_json = $1, rows_returned = $2,
                duration_ms = $3, finished_at = now()
            WHERE id = $4 AND status = 'running'
            "#,
        )
        .bind(output_json)
        .bind(rows_returned)
        .bind(duration_ms)
        .bind(run.id)
        .execute(&self.pool)
        .await?;
        if changed.rows_affected() != 1 {
            bail!("workflow node completion was stale");
        }
        self.insert_checkpoint(
            run.job_id,
            run.node_id.as_str(),
            "node_completed",
            json!({
                "workflow_id": run.workflow_id, "node_id": run.node_id, "attempt": run.attempt,
                "rows_returned": rows_returned,
            }),
        )
        .await?;
        self.insert_event(
            run.job_id,
            "workflow_node_completed",
            Some(run.node_id.as_str()),
            json!({
                "workflow_id": run.workflow_id, "node_id": run.node_id, "attempt": run.attempt,
                "rows_returned": rows_returned,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn record_branch_decision(
        &self,
        job_id: Uuid,
        workflow_id: Uuid,
        node_id: &NodeId,
        cardinality: &str,
    ) -> Result<()> {
        self.insert_event(
            job_id,
            "workflow_branch_decided",
            Some(node_id.as_str()),
            json!({
                "workflow_id": workflow_id, "node_id": node_id, "cardinality": cardinality,
            }),
        )
        .await
    }

    /// Records that composition dropped fields exceeding the principal's
    /// permitted sensitivity. The event payload carries field names only,
    /// never values, matching every other audit path in this module.
    pub async fn record_sensitivity_drop(
        &self,
        job_id: Uuid,
        workflow_id: Uuid,
        node_id: &NodeId,
        dropped_fields: &[String],
    ) -> Result<()> {
        if dropped_fields.is_empty() {
            return Ok(());
        }
        self.insert_event(
            job_id,
            "workflow_field_dropped",
            Some(node_id.as_str()),
            json!({
                "workflow_id": workflow_id, "node_id": node_id, "dropped_fields": dropped_fields,
            }),
        )
        .await
    }

    pub async fn fail_node(&self, run: &WorkflowNodeRun, duration_ms: i32) -> Result<()> {
        sqlx::query(
            "UPDATE chat_workflow_node_runs SET status = 'failed', duration_ms = $1, finished_at = now() WHERE id = $2 AND status = 'running'",
        )
        .bind(duration_ms)
        .bind(run.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_workflow_paused(
        &self,
        job_id: Uuid,
        user_id: Uuid,
        workflow_id: Uuid,
        node_id: &NodeId,
        clarification: &ClarificationPayload,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let waiting = sqlx::query(
            "UPDATE chat_workflow_node_runs SET status = 'waiting' WHERE job_id = $1 AND workflow_id = $2 AND node_id = $3 AND status = 'running'",
        )
        .bind(job_id)
        .bind(workflow_id)
        .bind(node_id.as_str())
        .execute(&mut *tx)
        .await?;
        if waiting.rows_affected() != 1 {
            bail!("workflow interrupt node was not running");
        }
        let updated = sqlx::query(
            r#"
            UPDATE chat_jobs
            SET status = 'waiting_for_user_input', current_step = 'waiting_for_user_input',
                current_node_id = $1, workflow_revision = workflow_revision + 1,
                state_json = state_json || jsonb_build_object('workflow_runtime', jsonb_build_object(
                    'waiting_node_id', $1, 'clarification_id', $2, 'workflow_id', $3
                )), updated_at = now()
            WHERE id = $4 AND user_id = $5 AND workflow_id = $3
            "#,
        )
        .bind(node_id.as_str())
        .bind(clarification.id)
        .bind(workflow_id)
        .bind(job_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            bail!("workflow pause was stale");
        }
        sqlx::query(
            "UPDATE assistant_job_memory SET pending_clarification_json = $1 WHERE job_id = $2",
        )
        .bind(serde_json::to_value(clarification)?)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        self.insert_checkpoint_in_tx(
            &mut tx,
            job_id,
            node_id.as_str(),
            "workflow_paused",
            json!({
                "workflow_id": workflow_id,
                "node_id": node_id,
                "clarification_id": clarification.id,
            }),
        )
        .await?;
        self.insert_event_in_tx(
            &mut tx,
            job_id,
            "workflow_paused",
            Some(node_id.as_str()),
            json!({
                "status": "waiting_for_user_input", "workflow_id": workflow_id, "node_id": node_id,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The five durable identity values are checked under one job-row lock;
    /// a stale answer never becomes a different workflow continuation.
    pub async fn resume(&self, request: WorkflowResumeRequest) -> Result<ResumeOutcome> {
        let mut tx = self.pool.begin().await?;
        let target = sqlx::query_as::<_, ResumeTargetRow>(
            r#"
            SELECT cj.status, cj.workflow_id, cj.workflow_revision, cj.current_node_id,
                   ajm.pending_clarification_json
            FROM chat_jobs cj
            LEFT JOIN assistant_job_memory ajm ON ajm.job_id = cj.id
            WHERE cj.id = $1 AND cj.user_id = $2
            FOR UPDATE OF cj
            "#,
        )
        .bind(request.job_id)
        .bind(request.user_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(target) = target else {
            return Ok(ResumeOutcome::NotFound);
        };
        if target.status != "waiting_for_user_input" {
            return Ok(ResumeOutcome::NotWaiting);
        };
        let payload = target
            .pending_clarification_json
            .and_then(|value| serde_json::from_value::<ClarificationPayload>(value).ok());
        let matches = target.workflow_id == Some(request.workflow_id)
            && target.workflow_revision == request.workflow_revision
            && target.current_node_id.as_deref() == Some(request.node_id.as_str())
            && payload.is_some_and(|payload| payload.id == request.clarification_id);
        if !matches {
            return Ok(ResumeOutcome::Stale);
        };

        let updated = sqlx::query(
            r#"
            UPDATE chat_jobs
            SET status = 'queued', current_step = 'executing_node',
                current_node_id = $1, workflow_revision = workflow_revision + 1,
                state_json = state_json || jsonb_build_object('workflow_runtime', jsonb_build_object(
                    'selected_value', $2::jsonb, 'resumed_from_node_id', $1
                )), updated_at = now()
            WHERE id = $3
            "#,
        )
        .bind(request.node_id.as_str())
        .bind(request.selected_value)
        .bind(request.job_id)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            bail!("workflow resume update failed")
        };
        sqlx::query(
            r#"
            UPDATE chat_workflow_node_runs SET status = 'completed', finished_at = now()
            WHERE job_id = $1 AND workflow_id = $2 AND node_id = $3 AND status = 'waiting'
            "#,
        )
        .bind(request.job_id)
        .bind(request.workflow_id)
        .bind(request.node_id.as_str())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE assistant_job_memory SET pending_clarification_json = NULL WHERE job_id = $1",
        )
        .bind(request.job_id)
        .execute(&mut *tx)
        .await?;
        self.insert_checkpoint_in_tx(
            &mut tx,
            request.job_id,
            request.node_id.as_str(),
            "workflow_resumed",
            json!({
                "workflow_id": request.workflow_id, "node_id": request.node_id,
            }),
        )
        .await?;
        self.insert_event_in_tx(
            &mut tx,
            request.job_id,
            "workflow_resumed",
            Some(request.node_id.as_str()),
            json!({
                "workflow_id": request.workflow_id, "node_id": request.node_id,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(ResumeOutcome::Resumed)
    }

    /// Adds to the job's `state_json.workflow_runtime.budget_consumed` ledger
    /// (queries/rows/ms) in a single UPDATE, so concurrent node completions
    /// serialize on Postgres's row lock instead of racing a read-modify-write
    /// in application code. Only the `budget_consumed` path is touched —
    /// sibling `workflow_runtime` fields (e.g. `waiting_node_id`) survive.
    pub async fn add_budget_consumed(
        &self,
        job_id: Uuid,
        delta_queries: i64,
        delta_rows: i64,
        delta_ms: i64,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE chat_jobs
            SET state_json = jsonb_set(
                    state_json,
                    '{workflow_runtime,budget_consumed}',
                    jsonb_build_object(
                        'queries', COALESCE((state_json #>> '{workflow_runtime,budget_consumed,queries}')::bigint, 0) + $1,
                        'rows', COALESCE((state_json #>> '{workflow_runtime,budget_consumed,rows}')::bigint, 0) + $2,
                        'ms', COALESCE((state_json #>> '{workflow_runtime,budget_consumed,ms}')::bigint, 0) + $3
                    )
                ),
                updated_at = now()
            WHERE id = $4
            "#,
        )
        .bind(delta_queries)
        .bind(delta_rows)
        .bind(delta_ms)
        .bind(job_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn insert_checkpoint(
        &self,
        job_id: Uuid,
        step: &str,
        checkpoint_type: &str,
        state: Value,
    ) -> Result<()> {
        sqlx::query("INSERT INTO chat_job_checkpoints (id, job_id, step, checkpoint_type, state_json) VALUES ($1, $2, $3, $4, $5)")
            .bind(Uuid::new_v4()).bind(job_id).bind(step).bind(checkpoint_type).bind(state)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn insert_event(
        &self,
        job_id: Uuid,
        event_type: &str,
        step: Option<&str>,
        payload: Value,
    ) -> Result<()> {
        sqlx::query("INSERT INTO chat_job_events (id, job_id, event_type, step, payload_json) VALUES ($1, $2, $3, $4, $5)")
            .bind(Uuid::new_v4()).bind(job_id).bind(event_type).bind(step).bind(payload)
            .execute(&self.pool).await?;
        Ok(())
    }

    async fn insert_checkpoint_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        job_id: Uuid,
        step: &str,
        checkpoint_type: &str,
        state: Value,
    ) -> Result<()> {
        sqlx::query("INSERT INTO chat_job_checkpoints (id, job_id, step, checkpoint_type, state_json) VALUES ($1, $2, $3, $4, $5)")
            .bind(Uuid::new_v4()).bind(job_id).bind(step).bind(checkpoint_type).bind(state)
            .execute(&mut **tx).await?;
        Ok(())
    }

    async fn insert_event_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        job_id: Uuid,
        event_type: &str,
        step: Option<&str>,
        payload: Value,
    ) -> Result<()> {
        sqlx::query("INSERT INTO chat_job_events (id, job_id, event_type, step, payload_json) VALUES ($1, $2, $3, $4, $5)")
            .bind(Uuid::new_v4()).bind(job_id).bind(event_type).bind(step).bind(payload)
            .execute(&mut **tx).await?;
        Ok(())
    }
}

/// Exact sensitive inputs are intentionally absent from durable output JSON.
pub fn persisted_output(inputs: &[NodeInput], output: Value) -> Value {
    if inputs
        .iter()
        .any(|input| matches!(input.source, BindingSource::ExactSensitiveInput))
    {
        json!({ "typed_output": output.get("typed_output").cloned().unwrap_or(Value::Null) })
    } else {
        output
    }
}

#[derive(FromRow)]
struct NodeRunRow {
    id: Uuid,
    job_id: Uuid,
    workflow_id: Uuid,
    node_id: String,
    attempt: i16,
    status: String,
    output_json: Option<Value>,
    provenance_json: Value,
    rows_returned: i32,
    duration_ms: Option<i32>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
}
impl TryFrom<NodeRunRow> for WorkflowNodeRun {
    type Error = anyhow::Error;
    fn try_from(row: NodeRunRow) -> Result<Self> {
        Ok(Self {
            id: row.id,
            job_id: row.job_id,
            workflow_id: row.workflow_id,
            node_id: NodeId::new(row.node_id)
                .map_err(|_| anyhow::anyhow!("persisted node id is invalid"))?,
            attempt: row.attempt,
            status: match row.status.as_str() {
                "runnable" => NodeRunStatus::Runnable,
                "running" => NodeRunStatus::Running,
                "completed" => NodeRunStatus::Completed,
                "failed" => NodeRunStatus::Failed,
                "skipped" => NodeRunStatus::Skipped,
                "waiting" => NodeRunStatus::Waiting,
                _ => bail!("persisted workflow node status is invalid"),
            },
            output_json: row.output_json,
            provenance_json: row.provenance_json,
            rows_returned: row.rows_returned,
            duration_ms: row.duration_ms,
            started_at: row.started_at,
            finished_at: row.finished_at,
        })
    }
}
#[derive(FromRow)]
struct ResumeTargetRow {
    status: String,
    workflow_id: Option<Uuid>,
    workflow_revision: i64,
    current_node_id: Option<String>,
    pending_clarification_json: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::workflow::contract::{BindingSource, NodeInput};
    use crate::knowledge::catalog::parameter_policy::ParameterType;

    #[test]
    fn exact_sensitive_input_is_not_persisted() {
        let persisted = persisted_output(
            &[NodeInput {
                parameter: "account_number".into(),
                kind: ParameterType::String,
                source: BindingSource::ExactSensitiveInput,
            }],
            json!({"account_number": "secret", "typed_output": {"masked": "***"}}),
        );
        assert_eq!(persisted, json!({"typed_output": {"masked": "***"}}));
        assert!(!persisted.to_string().contains("secret"));
    }
}
