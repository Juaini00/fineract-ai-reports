use anyhow::Result;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::{
    graph::{AssistantGraphTopology, GraphTransition},
    memory::JobMemory,
};

#[derive(Clone)]
pub struct JobMemoryRepository {
    pool: PgPool,
}

impl JobMemoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, job_id: Uuid, graph_state: &str) -> Result<JobMemory> {
        let row = sqlx::query_as::<_, JobMemoryRow>(
            r#"
            INSERT INTO assistant_job_memory (job_id, graph_state)
            VALUES ($1, $2)
            RETURNING job_id, graph_state, terminal_state, current_user_message_metadata_json,
                intent_json, source_intent_json, retrieval_plan_json, retrieval_evidence_json,
                evidence_decision_json, selected_capability, selected_tool, tool_params_json,
                policy_decision_json, execution_summary_json, structured_response_json,
                warnings_json, revision
            "#,
        )
        .bind(job_id)
        .bind(graph_state)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    pub async fn get(&self, job_id: Uuid) -> Result<Option<JobMemory>> {
        let row = sqlx::query_as::<_, JobMemoryRow>(
            r#"
            SELECT job_id, graph_state, terminal_state, current_user_message_metadata_json,
                intent_json, source_intent_json, retrieval_plan_json, retrieval_evidence_json,
                evidence_decision_json, selected_capability, selected_tool, tool_params_json,
                policy_decision_json, execution_summary_json, structured_response_json,
                warnings_json, revision
            FROM assistant_job_memory
            WHERE job_id = $1
            "#,
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    pub async fn save(&self, memory: &JobMemory, expected_revision: i64) -> Result<JobMemory> {
        let row = sqlx::query_as::<_, JobMemoryRow>(
            r#"
            UPDATE assistant_job_memory
            SET graph_state = $1, terminal_state = $2, current_user_message_metadata_json = $3,
                intent_json = $4, source_intent_json = $5, retrieval_plan_json = $6,
                retrieval_evidence_json = $7, evidence_decision_json = $8,
                selected_capability = $9, selected_tool = $10, tool_params_json = $11,
                policy_decision_json = $12, execution_summary_json = $13,
                structured_response_json = $14, warnings_json = $15,
                revision = revision + 1, updated_at = now()
            WHERE job_id = $16 AND revision = $17
            RETURNING job_id, graph_state, terminal_state, current_user_message_metadata_json,
                intent_json, source_intent_json, retrieval_plan_json, retrieval_evidence_json,
                evidence_decision_json, selected_capability, selected_tool, tool_params_json,
                policy_decision_json, execution_summary_json, structured_response_json,
                warnings_json, revision
            "#,
        )
        .bind(&memory.graph_state)
        .bind(
            memory
                .terminal_state
                .map(serde_json::to_value)
                .transpose()?
                .and_then(|v| v.as_str().map(str::to_string)),
        )
        .bind(&memory.current_user_message_metadata)
        .bind(
            memory
                .intent
                .as_ref()
                .map(serde_json::to_value)
                .transpose()?,
        )
        .bind(&memory.source_intent)
        .bind(&memory.retrieval_plan)
        .bind(&memory.retrieval_evidence)
        .bind(&memory.evidence_decision)
        .bind(&memory.selected_capability)
        .bind(&memory.selected_tool)
        .bind(&memory.tool_params)
        .bind(&memory.policy_decision)
        .bind(&memory.execution_summary)
        .bind(
            memory
                .structured_response
                .as_ref()
                .map(serde_json::to_value)
                .transpose()?,
        )
        .bind(&memory.warnings)
        .bind(memory.job_id)
        .bind(expected_revision)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Into::into)
            .ok_or_else(|| anyhow::anyhow!("assistant job memory was updated by another request"))
    }

    pub async fn insert_checkpoint(
        &self,
        memory: &JobMemory,
        state_json: serde_json::Value,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO assistant_graph_checkpoints
                (id, job_id, graph_state, previous_state, current_state, terminal_state, memory_revision, state_json)
            VALUES ($1, $2, $3, NULL, $3, $4, $5, $6)
            "#,
        )
        .bind(id)
        .bind(memory.job_id)
        .bind(&memory.graph_state)
        .bind(
            memory
                .terminal_state
                .map(serde_json::to_value)
                .transpose()?
                .and_then(|v| v.as_str().map(str::to_string)),
        )
        .bind(memory.revision)
        .bind(state_json)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn checkpoint_transition(
        &self,
        job_id: Uuid,
        transition: &GraphTransition,
        memory_revision: i64,
        state_json: serde_json::Value,
    ) -> Result<Uuid> {
        AssistantGraphTopology::new().validate_transition(transition)?;
        let id = Uuid::new_v4();
        let graph_state = serde_json::to_value(transition.to.unwrap_or(transition.from))?
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        sqlx::query(
            r#"
            INSERT INTO assistant_graph_checkpoints
                (id, job_id, graph_state, previous_state, current_state, terminal_state, transition_reason,
                 memory_revision, state_json, event_metadata_json)
            VALUES ($1, $2, $4, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(id)
        .bind(job_id)
        .bind(
            serde_json::to_value(transition.from)?
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
        )
        .bind(graph_state)
        .bind(
            transition
                .terminal
                .map(serde_json::to_value)
                .transpose()?
                .and_then(|value| value.as_str().map(str::to_string)),
        )
        .bind(&transition.reason)
        .bind(memory_revision)
        .bind(&state_json)
        .bind(state_json)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn list_latest_checkpoints(
        &self,
        job_id: Uuid,
        limit: i64,
    ) -> Result<Vec<GraphCheckpoint>> {
        Ok(sqlx::query_as::<_, GraphCheckpoint>(
            r#"
            SELECT id, job_id, previous_state, current_state, terminal_state, transition_reason,
                memory_revision, state_json, event_metadata_json
            FROM assistant_graph_checkpoints
            WHERE job_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(job_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct GraphCheckpoint {
    pub id: Uuid,
    pub job_id: Uuid,
    pub previous_state: Option<String>,
    pub current_state: String,
    pub terminal_state: Option<String>,
    pub transition_reason: Option<String>,
    pub memory_revision: i64,
    pub state_json: serde_json::Value,
    pub event_metadata_json: serde_json::Value,
}

#[derive(FromRow)]
struct JobMemoryRow {
    job_id: Uuid,
    graph_state: String,
    terminal_state: Option<String>,
    current_user_message_metadata_json: serde_json::Value,
    intent_json: Option<serde_json::Value>,
    source_intent_json: Option<serde_json::Value>,
    retrieval_plan_json: serde_json::Value,
    retrieval_evidence_json: serde_json::Value,
    evidence_decision_json: serde_json::Value,
    selected_capability: Option<String>,
    selected_tool: Option<String>,
    tool_params_json: serde_json::Value,
    policy_decision_json: serde_json::Value,
    execution_summary_json: serde_json::Value,
    structured_response_json: Option<serde_json::Value>,
    warnings_json: serde_json::Value,
    revision: i64,
}

impl From<JobMemoryRow> for JobMemory {
    fn from(row: JobMemoryRow) -> Self {
        let terminal_state = row
            .terminal_state
            .and_then(|value| serde_json::from_value(serde_json::Value::String(value)).ok());
        let intent = row
            .intent_json
            .and_then(|value| serde_json::from_value(value).ok());
        let structured_response = row
            .structured_response_json
            .and_then(|value| serde_json::from_value(value).ok());
        Self {
            job_id: row.job_id,
            graph_state: row.graph_state,
            terminal_state,
            current_user_message_metadata: row.current_user_message_metadata_json,
            intent,
            source_intent: row.source_intent_json,
            retrieval_plan: row.retrieval_plan_json,
            retrieval_evidence: row.retrieval_evidence_json,
            evidence_decision: row.evidence_decision_json,
            selected_capability: row.selected_capability,
            selected_tool: row.selected_tool,
            tool_params: row.tool_params_json,
            policy_decision: row.policy_decision_json,
            execution_summary: row.execution_summary_json,
            structured_response,
            warnings: row.warnings_json,
            revision: row.revision,
        }
    }
}
