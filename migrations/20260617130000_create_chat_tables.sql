CREATE TABLE IF NOT EXISTS chat_sessions (
    id UUID PRIMARY KEY,
    api_key_id UUID NOT NULL REFERENCES api_keys(id),
    title TEXT NULL,
    status TEXT NOT NULL,
    context_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NULL,
    archived_at TIMESTAMPTZ NULL,
    CONSTRAINT chk_chat_sessions_status CHECK (status IN ('active', 'archived', 'expired'))
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    job_id UUID NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    metadata_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_chat_messages_role CHECK (role IN ('user', 'assistant', 'system', 'tool', 'clarification'))
);

CREATE TABLE IF NOT EXISTS chat_jobs (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    api_key_id UUID NOT NULL REFERENCES api_keys(id),
    user_message_id UUID NULL REFERENCES chat_messages(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    current_step TEXT NOT NULL,
    resume_from_step TEXT NULL,
    message TEXT NOT NULL,
    state_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    result_json JSONB NULL,
    error_json JSONB NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ NULL,
    failed_at TIMESTAMPTZ NULL,
    cancelled_at TIMESTAMPTZ NULL,
    CONSTRAINT chk_chat_jobs_status CHECK (status IN ('queued', 'running', 'waiting_for_user_input', 'completed', 'failed', 'expired', 'cancelled')),
    CONSTRAINT chk_chat_jobs_current_step CHECK (current_step IN ('queued', 'checking_context', 'embedding', 'taking_decision', 'response', 'authorizing', 'estimating_cost', 'executing_query', 'shaping_result', 'formatting_response')),
    CONSTRAINT chk_chat_jobs_resume_from_step CHECK (resume_from_step IS NULL OR resume_from_step IN ('queued', 'checking_context', 'embedding', 'taking_decision', 'response', 'authorizing', 'estimating_cost', 'executing_query', 'shaping_result', 'formatting_response'))
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_chat_messages_job_id'
    ) THEN
        ALTER TABLE chat_messages
            ADD CONSTRAINT fk_chat_messages_job_id
            FOREIGN KEY (job_id) REFERENCES chat_jobs(id) ON DELETE SET NULL;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS chat_job_checkpoints (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id) ON DELETE CASCADE,
    step TEXT NOT NULL,
    checkpoint_type TEXT NOT NULL,
    state_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_chat_job_checkpoints_type CHECK (checkpoint_type IN ('job_created', 'step_started', 'step_completed', 'clarification_required', 'user_response_received', 'decision_completed', 'query_completed', 'response_completed', 'job_failed', 'job_cancelled'))
);

CREATE TABLE IF NOT EXISTS chat_job_events (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    step TEXT NULL,
    payload_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_chat_job_events_type CHECK (event_type IN ('status', 'clarification', 'partial_result', 'final', 'error', 'heartbeat'))
);

CREATE INDEX IF NOT EXISTS idx_chat_sessions_api_key_id ON chat_sessions(api_key_id);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_status ON chat_sessions(status);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_expires_at ON chat_sessions(expires_at);

CREATE INDEX IF NOT EXISTS idx_chat_messages_session_id ON chat_messages(session_id);
CREATE INDEX IF NOT EXISTS idx_chat_messages_job_id ON chat_messages(job_id);
CREATE INDEX IF NOT EXISTS idx_chat_messages_created_at ON chat_messages(created_at);

CREATE INDEX IF NOT EXISTS idx_chat_jobs_session_id ON chat_jobs(session_id);
CREATE INDEX IF NOT EXISTS idx_chat_jobs_api_key_id ON chat_jobs(api_key_id);
CREATE INDEX IF NOT EXISTS idx_chat_jobs_status ON chat_jobs(status);
CREATE INDEX IF NOT EXISTS idx_chat_jobs_expires_at ON chat_jobs(expires_at);
CREATE INDEX IF NOT EXISTS idx_chat_jobs_created_at ON chat_jobs(created_at);

CREATE INDEX IF NOT EXISTS idx_chat_job_checkpoints_job_id ON chat_job_checkpoints(job_id);
CREATE INDEX IF NOT EXISTS idx_chat_job_checkpoints_job_step ON chat_job_checkpoints(job_id, step);
CREATE INDEX IF NOT EXISTS idx_chat_job_checkpoints_created_at ON chat_job_checkpoints(created_at);

CREATE INDEX IF NOT EXISTS idx_chat_job_events_job_id ON chat_job_events(job_id);
CREATE INDEX IF NOT EXISTS idx_chat_job_events_job_type ON chat_job_events(job_id, event_type);
CREATE INDEX IF NOT EXISTS idx_chat_job_events_created_at ON chat_job_events(created_at);
