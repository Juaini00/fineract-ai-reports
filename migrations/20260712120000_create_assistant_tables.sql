CREATE TABLE assistant_job_memory (
    job_id UUID PRIMARY KEY REFERENCES chat_jobs(id) ON DELETE CASCADE,
    graph_state TEXT NOT NULL,
    terminal_state TEXT NULL,
    intent_json JSONB NULL,
    retrieval_evidence_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    selected_capability TEXT NULL,
    selected_tool TEXT NULL,
    policy_decision_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    execution_summary_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    structured_response_json JSONB NULL,
    warnings_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    revision BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_assistant_job_memory_graph_state ON assistant_job_memory (graph_state);
CREATE INDEX idx_assistant_job_memory_terminal_state ON assistant_job_memory (terminal_state);
CREATE INDEX idx_assistant_job_memory_intent_domain ON assistant_job_memory ((intent_json->>'domain'));
CREATE INDEX idx_assistant_job_memory_selected_capability ON assistant_job_memory (selected_capability);

CREATE TABLE assistant_session_memory (
    session_id UUID PRIMARY KEY REFERENCES chat_sessions(id) ON DELETE CASCADE,
    summary TEXT NULL,
    active_domain TEXT NULL,
    pending_clarification_json JSONB NULL,
    entities_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    revision BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_assistant_session_memory_active_domain ON assistant_session_memory (active_domain);
CREATE INDEX idx_assistant_session_memory_updated_at ON assistant_session_memory (updated_at);

CREATE TABLE assistant_graph_checkpoints (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id) ON DELETE CASCADE,
    graph_state TEXT NOT NULL,
    terminal_state TEXT NULL,
    memory_revision BIGINT NOT NULL,
    state_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_assistant_graph_checkpoints_job_id ON assistant_graph_checkpoints (job_id);
CREATE INDEX idx_assistant_graph_checkpoints_job_state ON assistant_graph_checkpoints (job_id, graph_state);
CREATE INDEX idx_assistant_graph_checkpoints_created_at ON assistant_graph_checkpoints (created_at);

CREATE TABLE assistant_llm_traces (
    id UUID PRIMARY KEY,
    job_id UUID NULL REFERENCES chat_jobs(id) ON DELETE SET NULL,
    session_id UUID NULL REFERENCES chat_sessions(id) ON DELETE SET NULL,
    api_key_id UUID NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    graph_state TEXT NULL,
    purpose TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    total_tokens INTEGER NOT NULL,
    cost_usd NUMERIC(10, 6) NULL,
    latency_ms INTEGER NOT NULL,
    status TEXT NOT NULL,
    error_kind TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_assistant_llm_traces_status CHECK (status IN ('ok', 'malformed', 'timeout', 'error'))
);

CREATE INDEX idx_assistant_llm_traces_created_at ON assistant_llm_traces (created_at);
CREATE INDEX idx_assistant_llm_traces_api_key_created_at ON assistant_llm_traces (api_key_id, created_at);
CREATE INDEX idx_assistant_llm_traces_provider_model ON assistant_llm_traces (provider, model);
CREATE INDEX idx_assistant_llm_traces_purpose ON assistant_llm_traces (purpose);
