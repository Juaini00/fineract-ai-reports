-- no-transaction
CREATE INDEX CONCURRENTLY idx_chat_job_audit_events_user_id ON chat_job_audit_events(user_id);
