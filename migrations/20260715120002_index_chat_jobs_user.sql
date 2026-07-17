-- no-transaction
CREATE INDEX CONCURRENTLY idx_chat_jobs_user_id ON chat_jobs(user_id);
