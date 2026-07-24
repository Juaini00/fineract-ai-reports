CREATE TABLE management_audit_outbox (
    id UUID PRIMARY KEY,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('chat_job', 'chat_session', 'management')),
    aggregate_id UUID NOT NULL,
    job_id UUID NULL REFERENCES chat_jobs(id) ON DELETE SET NULL,
    session_id UUID NULL REFERENCES chat_sessions(id) ON DELETE SET NULL,
    actor_user_id UUID NULL,
    role TEXT NULL CHECK (role IS NULL OR role = 'admin'),
    correlation_id UUID NULL,
    contract_version SMALLINT NOT NULL DEFAULT 1 CHECK (contract_version > 0),
    payload JSONB NOT NULL CHECK (jsonb_typeof(payload) = 'object'),
    occurred_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ NULL,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error_code TEXT NULL CHECK (last_error_code IS NULL OR last_error_code IN (
        'database_unavailable', 'dispatcher_unavailable', 'serialization_failed', 'unknown'
    )),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_management_audit_outbox_due
    ON management_audit_outbox (next_attempt_at, created_at)
    WHERE published_at IS NULL;
CREATE INDEX idx_management_audit_outbox_job_timeline
    ON management_audit_outbox (job_id, occurred_at);
CREATE INDEX idx_management_audit_outbox_session_timeline
    ON management_audit_outbox (session_id, occurred_at);
CREATE INDEX idx_management_audit_outbox_correlation
    ON management_audit_outbox (correlation_id, occurred_at);

CREATE TABLE management_audit_events (
    id UUID PRIMARY KEY,
    outbox_id UUID NULL UNIQUE REFERENCES management_audit_outbox(id) ON DELETE SET NULL,
    job_id UUID NULL REFERENCES chat_jobs(id) ON DELETE SET NULL,
    session_id UUID NULL REFERENCES chat_sessions(id) ON DELETE SET NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('chat_job', 'chat_session', 'management')),
    aggregate_id UUID NOT NULL,
    actor_user_id UUID NULL,
    role TEXT NULL CHECK (role IS NULL OR role = 'admin'),
    event_type TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('success', 'blocked', 'clarification', 'unsupported', 'failed')),
    correlation_id UUID NULL,
    contract_version SMALLINT NOT NULL DEFAULT 1 CHECK (contract_version > 0),
    catalog_version_id UUID NULL,
    catalog_content_hash TEXT NULL,
    index_version_id UUID NULL,
    summary_json JSONB NOT NULL CHECK (jsonb_typeof(summary_json) = 'object'),
    sanitized_error_json JSONB NULL CHECK (sanitized_error_json IS NULL OR jsonb_typeof(sanitized_error_json) = 'object'),
    occurred_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_management_audit_events_job_timeline
    ON management_audit_events (job_id, occurred_at, id);
CREATE INDEX idx_management_audit_events_session_timeline
    ON management_audit_events (session_id, occurred_at, id);
CREATE INDEX idx_management_audit_events_correlation
    ON management_audit_events (correlation_id, occurred_at, id);
CREATE INDEX idx_management_audit_events_feed
    ON management_audit_events (occurred_at DESC, id DESC);

CREATE FUNCTION reject_management_audit_event_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'management audit events are immutable';
END;
$$;

CREATE TRIGGER management_audit_events_immutable
BEFORE UPDATE OR DELETE ON management_audit_events
FOR EACH ROW EXECUTE FUNCTION reject_management_audit_event_mutation();

CREATE TABLE management_telemetry_counters (
    counter_date DATE NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN (
        'telemetry_enqueued', 'telemetry_persisted', 'telemetry_dropped_queue_full',
        'telemetry_persist_failed', 'telemetry_retry_exhausted'
    )),
    value BIGINT NOT NULL DEFAULT 0 CHECK (value >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (counter_date, kind)
);

ALTER TABLE assistant_llm_traces
    DROP CONSTRAINT assistant_llm_traces_api_key_id_fkey,
    ADD CONSTRAINT assistant_llm_traces_api_key_id_fkey
        FOREIGN KEY (api_key_id) REFERENCES api_keys(id) ON DELETE SET NULL,
    ADD COLUMN correlation_id UUID NULL,
    ADD COLUMN context_contract_version SMALLINT NULL CHECK (context_contract_version > 0),
    ADD COLUMN price_version TEXT NULL,
    ADD COLUMN cost_currency TEXT NULL,
    ADD COLUMN error_code TEXT NULL CHECK (error_code IS NULL OR error_code IN (
        'provider_unavailable', 'provider_timeout', 'provider_malformed', 'database_unavailable', 'unknown'
    )),
    ADD COLUMN catalog_version_id UUID NULL,
    ADD COLUMN index_version_id UUID NULL,
    ADD COLUMN actor_user_id UUID NULL,
    ADD COLUMN actor_api_key_id UUID NULL;

UPDATE assistant_llm_traces
SET actor_user_id = user_id,
    actor_api_key_id = api_key_id
WHERE actor_user_id IS NULL AND actor_api_key_id IS NULL;

ALTER TABLE assistant_llm_traces
    DROP CONSTRAINT chk_assistant_llm_traces_owner,
    ADD CONSTRAINT chk_assistant_llm_traces_owner CHECK (
        user_id IS NOT NULL OR api_key_id IS NOT NULL
        OR actor_user_id IS NOT NULL OR actor_api_key_id IS NOT NULL
    );

CREATE FUNCTION snapshot_assistant_llm_trace_actor()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.actor_user_id IS NULL THEN
        NEW.actor_user_id := NEW.user_id;
    END IF;
    IF NEW.actor_api_key_id IS NULL THEN
        NEW.actor_api_key_id := NEW.api_key_id;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER assistant_llm_traces_actor_snapshot
BEFORE INSERT OR UPDATE OF user_id, api_key_id ON assistant_llm_traces
FOR EACH ROW EXECUTE FUNCTION snapshot_assistant_llm_trace_actor();

CREATE INDEX idx_assistant_llm_traces_correlation ON assistant_llm_traces (correlation_id);
