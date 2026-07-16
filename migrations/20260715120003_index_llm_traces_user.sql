-- no-transaction
CREATE INDEX CONCURRENTLY idx_assistant_llm_traces_user_id ON assistant_llm_traces(user_id);
