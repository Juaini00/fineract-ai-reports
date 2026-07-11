# Audit Trail Design: Safe Payload Rules

Source: `docs-old/audit-trail-design.md`

## Safe Payload Rules

Allowed:

```text
job_id
session_id
api_key_id
key_prefix
owner
allowed_office_ids
allowed_capabilities
domain
capability
query_id
confidence
candidate source ids and scores
SQL file path
row_count
duration_ms
sanitized error code/message
```

Not allowed:

```text
raw API key
authorization header
raw embeddings
hidden prompts
full SQL result rows
raw SQL when not needed
secret config values
unmasked PII unless explicitly required and policy-approved
```

The user message already lives in `chat_messages`; audit events should store a compact input summary rather than duplicating full message content by default.
