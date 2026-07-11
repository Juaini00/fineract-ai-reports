# Chat Data Model: 3. chat_sessions

Source: `docs-old/chat-data-model.md`

## 3. chat_sessions

Represents a conversation context for one API key/client.

Purpose:

1. Group multiple messages together.
2. Preserve lightweight conversation context.
3. Allow follow-up questions.
4. Track session lifecycle.

Schema:

```sql
CREATE TABLE chat_sessions (
    id UUID PRIMARY KEY,
    api_key_id UUID NOT NULL REFERENCES api_keys(id),
    title TEXT NULL,
    status TEXT NOT NULL,
    context_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NULL,
    archived_at TIMESTAMPTZ NULL
);
```

Recommended statuses:

```text
active
archived
expired
```

`context_json` should store compact context only, for example:

```json
{
  "last_domain": "savings",
  "last_capability": "savings_deposit_total",
  "last_params": {
    "from_date": "2026-01-01",
    "to_date": "2026-05-31"
  },
  "last_result_summary": "Total deposit was IDR 920,000,000"
}
```

Do not store full prompt history in `context_json`.
