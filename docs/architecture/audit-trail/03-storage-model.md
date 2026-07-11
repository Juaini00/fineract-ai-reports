# Audit Trail Design: Storage Model

Source: `docs-old/audit-trail-design.md`

## Storage Model

Add a new table named `chat_job_audit_events`.

Recommended schema:

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

CREATE INDEX idx_chat_job_audit_events_job_id ON chat_job_audit_events(job_id, created_at);
CREATE INDEX idx_chat_job_audit_events_stage ON chat_job_audit_events(stage, created_at);
CREATE INDEX idx_chat_job_audit_events_blueprint_step ON chat_job_audit_events(blueprint_step, created_at);
CREATE INDEX idx_chat_job_audit_events_api_key_id ON chat_job_audit_events(api_key_id, created_at);
```

`chat_job_audit_events` is append-only. Do not update old audit rows during normal processing.
