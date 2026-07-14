ALTER TABLE assistant_job_memory
    ADD COLUMN IF NOT EXISTS current_user_message_metadata_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS source_intent_json JSONB NULL,
    ADD COLUMN IF NOT EXISTS retrieval_plan_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS evidence_decision_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS tool_params_json JSONB NOT NULL DEFAULT '{}'::jsonb;

CREATE INDEX IF NOT EXISTS idx_assistant_job_memory_source_intent_domain
    ON assistant_job_memory ((source_intent_json->>'domain'));

ALTER TABLE assistant_session_memory
    ADD COLUMN IF NOT EXISTS pending_clarification_source_intent_json JSONB NULL,
    ADD COLUMN IF NOT EXISTS relevant_jobs_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS context_warnings_json JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE assistant_graph_checkpoints
    ADD COLUMN IF NOT EXISTS previous_state TEXT NULL,
    ADD COLUMN IF NOT EXISTS current_state TEXT NULL,
    ADD COLUMN IF NOT EXISTS transition_reason TEXT NULL,
    ADD COLUMN IF NOT EXISTS event_metadata_json JSONB NOT NULL DEFAULT '{}'::jsonb;

UPDATE assistant_graph_checkpoints
SET current_state = graph_state
WHERE current_state IS NULL;

ALTER TABLE assistant_graph_checkpoints
    ALTER COLUMN current_state SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_assistant_graph_checkpoints_job_current_state
    ON assistant_graph_checkpoints (job_id, current_state);
