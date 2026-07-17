ALTER TABLE chat_sessions
    ADD COLUMN user_id UUID NULL REFERENCES users(id) ON DELETE RESTRICT;
ALTER TABLE chat_jobs
    ADD COLUMN user_id UUID NULL REFERENCES users(id) ON DELETE RESTRICT;
ALTER TABLE assistant_llm_traces
    ADD COLUMN user_id UUID NULL REFERENCES users(id) ON DELETE RESTRICT;
ALTER TABLE chat_job_audit_events
    ADD COLUMN user_id UUID NULL REFERENCES users(id) ON DELETE RESTRICT;

UPDATE chat_sessions s
SET user_id = k.user_id
FROM api_keys k
WHERE s.api_key_id = k.id AND k.user_id IS NOT NULL;

WITH owners AS (
    SELECT j.id, (ARRAY_AGG(v.user_id))[1] AS user_id
    FROM chat_jobs j JOIN chat_sessions s ON s.id = j.session_id
    LEFT JOIN api_keys k ON k.id = j.api_key_id
    CROSS JOIN LATERAL (VALUES (s.user_id), (k.user_id)) v(user_id)
    WHERE v.user_id IS NOT NULL GROUP BY j.id HAVING COUNT(DISTINCT v.user_id) = 1
)
UPDATE chat_jobs j SET user_id = owners.user_id FROM owners WHERE j.id = owners.id;

WITH owners AS (
    SELECT t.id, (ARRAY_AGG(v.user_id))[1] AS user_id
    FROM assistant_llm_traces t LEFT JOIN chat_jobs j ON j.id = t.job_id
    LEFT JOIN chat_sessions s ON s.id = t.session_id LEFT JOIN api_keys k ON k.id = t.api_key_id
    LEFT JOIN chat_sessions js ON js.id = j.session_id LEFT JOIN api_keys jk ON jk.id = j.api_key_id
    CROSS JOIN LATERAL (VALUES (s.user_id), (k.user_id), (js.user_id), (jk.user_id)) v(user_id)
    WHERE v.user_id IS NOT NULL GROUP BY t.id HAVING COUNT(DISTINCT v.user_id) = 1
)
UPDATE assistant_llm_traces t SET user_id = owners.user_id FROM owners WHERE t.id = owners.id;

WITH owners AS (
    SELECT a.id, (ARRAY_AGG(v.user_id))[1] AS user_id
    FROM chat_job_audit_events a LEFT JOIN chat_jobs j ON j.id = a.job_id
    LEFT JOIN chat_sessions s ON s.id = a.session_id LEFT JOIN api_keys k ON k.id = a.api_key_id
    LEFT JOIN chat_sessions js ON js.id = j.session_id LEFT JOIN api_keys jk ON jk.id = j.api_key_id
    CROSS JOIN LATERAL (VALUES (s.user_id), (k.user_id), (js.user_id), (jk.user_id)) v(user_id)
    WHERE v.user_id IS NOT NULL GROUP BY a.id HAVING COUNT(DISTINCT v.user_id) = 1
)
UPDATE chat_job_audit_events a SET user_id = owners.user_id FROM owners WHERE a.id = owners.id;

ALTER TABLE chat_sessions ALTER COLUMN api_key_id DROP NOT NULL;
ALTER TABLE chat_jobs ALTER COLUMN api_key_id DROP NOT NULL;
ALTER TABLE assistant_llm_traces ALTER COLUMN api_key_id DROP NOT NULL;

ALTER TABLE chat_sessions ADD CONSTRAINT chk_chat_sessions_owner
    CHECK (user_id IS NOT NULL OR api_key_id IS NOT NULL) NOT VALID;
ALTER TABLE chat_jobs ADD CONSTRAINT chk_chat_jobs_owner
    CHECK (user_id IS NOT NULL OR api_key_id IS NOT NULL) NOT VALID;
ALTER TABLE assistant_llm_traces ADD CONSTRAINT chk_assistant_llm_traces_owner
    CHECK (user_id IS NOT NULL OR api_key_id IS NOT NULL) NOT VALID;

ALTER TABLE chat_sessions VALIDATE CONSTRAINT chk_chat_sessions_owner;
ALTER TABLE chat_jobs VALIDATE CONSTRAINT chk_chat_jobs_owner;
ALTER TABLE assistant_llm_traces VALIDATE CONSTRAINT chk_assistant_llm_traces_owner;
