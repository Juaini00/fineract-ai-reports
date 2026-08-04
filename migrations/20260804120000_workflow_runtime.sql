-- Phase 4 durable workflow runtime. Historical jobs remain readable while the
-- workflow runner stores its own stable node identifier in current_node_id.
ALTER TABLE chat_jobs
    ADD COLUMN workflow_id UUID NULL,
    ADD COLUMN workflow_contract_version SMALLINT NULL,
    ADD COLUMN workflow_revision BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN current_node_id TEXT NULL;

CREATE TABLE chat_workflow_node_runs (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id) ON DELETE CASCADE,
    workflow_id UUID NOT NULL,
    node_id TEXT NOT NULL,
    attempt SMALLINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL CHECK (status IN ('runnable', 'running', 'completed', 'failed', 'skipped', 'waiting')),
    output_json JSONB NULL,
    provenance_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    rows_returned INT NOT NULL DEFAULT 0,
    duration_ms INT NULL,
    started_at TIMESTAMPTZ NULL,
    finished_at TIMESTAMPTZ NULL,
    UNIQUE (job_id, workflow_id, node_id, attempt)
);

CREATE INDEX idx_chat_workflow_node_runs_job_workflow
    ON chat_workflow_node_runs(job_id, workflow_id, node_id, attempt);

-- Preserve the legacy string only as historical state inside state_json.  The
-- current step is normalized to the workflow lifecycle vocabulary; its node
-- identity is the separate current_node_id column.
UPDATE chat_jobs
SET state_json = state_json || jsonb_build_object('legacy_step', current_step),
    current_step = CASE current_step
        WHEN 'queued' THEN 'queued'
        WHEN 'checking_context' THEN 'understanding'
        WHEN 'embedding' THEN 'understanding'
        WHEN 'taking_decision' THEN 'planning'
        WHEN 'route_intent' THEN 'understanding'
        WHEN 'plan_retrieval' THEN 'planning'
        WHEN 'retrieve_knowledge' THEN 'planning'
        WHEN 'evaluate_evidence' THEN 'verifying'
        WHEN 'plan_tool_or_capability' THEN 'planning'
        WHEN 'guard_execution' THEN 'verifying'
        WHEN 'authorizing' THEN 'verifying'
        WHEN 'estimating_cost' THEN 'verifying'
        WHEN 'executing_query' THEN 'executing_node'
        WHEN 'execute_tool_or_sql' THEN 'executing_node'
        WHEN 'shaping_result' THEN 'composing'
        WHEN 'build_structured_response' THEN 'composing'
        WHEN 'formatting_response' THEN 'composing'
        WHEN 'render_response' THEN 'composing'
        WHEN 'response' THEN 'completed'
        WHEN 'complete_or_wait' THEN CASE WHEN status = 'waiting_for_user_input' THEN 'waiting_for_user_input' ELSE 'completed' END
        ELSE 'queued'
    END,
    resume_from_step = CASE resume_from_step
        WHEN 'checking_context' THEN 'understanding'
        WHEN 'embedding' THEN 'understanding'
        WHEN 'taking_decision' THEN 'planning'
        WHEN 'route_intent' THEN 'understanding'
        WHEN 'plan_retrieval' THEN 'planning'
        WHEN 'retrieve_knowledge' THEN 'planning'
        WHEN 'evaluate_evidence' THEN 'verifying'
        WHEN 'plan_tool_or_capability' THEN 'planning'
        WHEN 'guard_execution' THEN 'verifying'
        WHEN 'authorizing' THEN 'verifying'
        WHEN 'estimating_cost' THEN 'verifying'
        WHEN 'executing_query' THEN 'executing_node'
        WHEN 'execute_tool_or_sql' THEN 'executing_node'
        WHEN 'shaping_result' THEN 'composing'
        WHEN 'build_structured_response' THEN 'composing'
        WHEN 'formatting_response' THEN 'composing'
        WHEN 'render_response' THEN 'composing'
        WHEN 'response' THEN 'completed'
        WHEN 'complete_or_wait' THEN 'waiting_for_user_input'
        WHEN 'queued' THEN 'queued'
        ELSE NULL
    END;

ALTER TABLE chat_jobs DROP CONSTRAINT IF EXISTS chk_chat_jobs_current_step;
ALTER TABLE chat_jobs DROP CONSTRAINT IF EXISTS chk_chat_jobs_resume_from_step;
ALTER TABLE chat_jobs
    ADD CONSTRAINT chk_chat_jobs_current_step
    CHECK (current_step IN ('queued', 'understanding', 'planning', 'verifying', 'executing_node', 'composing', 'waiting_for_user_input', 'completed', 'failed'));
ALTER TABLE chat_jobs
    ADD CONSTRAINT chk_chat_jobs_resume_from_step
    CHECK (resume_from_step IS NULL OR resume_from_step IN ('queued', 'understanding', 'planning', 'verifying', 'executing_node', 'composing', 'waiting_for_user_input', 'completed', 'failed'));

ALTER TABLE chat_job_events DROP CONSTRAINT IF EXISTS chk_chat_job_events_type;
ALTER TABLE chat_job_events
    ADD CONSTRAINT chk_chat_job_events_type
    CHECK (event_type IN (
        'status', 'clarification', 'partial_result', 'final', 'error', 'heartbeat',
        'stage', 'delta', 'workflow_node_started', 'workflow_node_completed',
        'workflow_branch_decided', 'workflow_paused', 'workflow_resumed'
    ));

ALTER TABLE chat_job_checkpoints DROP CONSTRAINT IF EXISTS chk_chat_job_checkpoints_type;
ALTER TABLE chat_job_checkpoints
    ADD CONSTRAINT chk_chat_job_checkpoints_type
    CHECK (checkpoint_type IN (
        'job_created', 'step_started', 'step_completed', 'clarification_required',
        'user_response_received', 'decision_completed', 'query_completed',
        'response_completed', 'job_failed', 'job_cancelled',
        'node_started', 'node_completed', 'workflow_paused', 'workflow_resumed'
    ));
