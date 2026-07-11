# Chat Data Model: 11. Indexes

Source: `docs-old/chat-data-model.md`

## 11. Indexes

Recommended indexes:

```sql
CREATE INDEX idx_chat_sessions_api_key_id ON chat_sessions(api_key_id);
CREATE INDEX idx_chat_sessions_status ON chat_sessions(status);

CREATE INDEX idx_chat_messages_session_id ON chat_messages(session_id);
CREATE INDEX idx_chat_messages_job_id ON chat_messages(job_id);

CREATE INDEX idx_chat_jobs_session_id ON chat_jobs(session_id);
CREATE INDEX idx_chat_jobs_api_key_id ON chat_jobs(api_key_id);
CREATE INDEX idx_chat_jobs_status ON chat_jobs(status);
CREATE INDEX idx_chat_jobs_expires_at ON chat_jobs(expires_at);

CREATE INDEX idx_chat_job_checkpoints_job_id ON chat_job_checkpoints(job_id);
CREATE INDEX idx_chat_job_checkpoints_job_step ON chat_job_checkpoints(job_id, step);

CREATE INDEX idx_chat_job_events_job_id ON chat_job_events(job_id);
CREATE INDEX idx_chat_job_events_job_type ON chat_job_events(job_id, event_type);

CREATE INDEX idx_chat_job_audit_events_job_id ON chat_job_audit_events(job_id, created_at);
CREATE INDEX idx_chat_job_audit_events_stage ON chat_job_audit_events(stage, created_at);
CREATE INDEX idx_chat_job_audit_events_blueprint_step ON chat_job_audit_events(blueprint_step, created_at);
```
