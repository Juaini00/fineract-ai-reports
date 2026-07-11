# Chat Data Model: 4. chat_messages

Source: `docs-old/chat-data-model.md`

## 4. chat_messages

Stores user and assistant messages.

Purpose:

1. Keep chat transcript.
2. Link messages to sessions.
3. Link assistant responses to jobs.
4. Support audit/debugging.

Schema:

```sql
CREATE TABLE chat_messages (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES chat_sessions(id),
    job_id UUID NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    metadata_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Roles:

```text
user
assistant
system
tool
clarification
```

Example user message:

```json
{
  "role": "user",
  "content": "Show savings data from January to May 2026"
}
```

Example assistant message:

```json
{
  "role": "assistant",
  "content": "Apakah Anda ingin total gabungan atau per bulan?",
  "metadata_json": {
    "type": "clarification",
    "response_key": "output_mode"
  }
}
```
