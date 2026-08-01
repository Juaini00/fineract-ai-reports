-- Streaming adds two event types: `stage` for pipeline progress and `delta`
-- for progressive prose. Both are durable in Postgres so a reconnecting client
-- can replay them; Redis remains live coordination only.
ALTER TABLE chat_job_events
    DROP CONSTRAINT IF EXISTS chk_chat_job_events_type;

ALTER TABLE chat_job_events
    ADD CONSTRAINT chk_chat_job_events_type
    CHECK (event_type IN (
        'status', 'clarification', 'partial_result',
        'final', 'error', 'heartbeat',
        'stage', 'delta'
    ));
