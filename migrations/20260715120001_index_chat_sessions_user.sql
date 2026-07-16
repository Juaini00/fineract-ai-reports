-- no-transaction
CREATE INDEX CONCURRENTLY idx_chat_sessions_user_id ON chat_sessions(user_id, updated_at);
