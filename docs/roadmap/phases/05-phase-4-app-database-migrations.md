# Implementation Steps: Phase 4: App Database Migrations

Source: `docs-old/implementation-steps.md`

## Phase 4: App Database Migrations

Goal: create the minimum database schema needed before auth and audit.

Important rule:

```text
schema changes must live in migration files, not in application startup code
```

Migration behavior:

```text
APP_DATABASE_MIGRATE_ON_STARTUP=false by default
APP_DATABASE_MIGRATE_ON_STARTUP=true allows local/dev startup migrations
```

Current local value:

```text
APP_DATABASE_MIGRATE_ON_STARTUP=true
```

Initial tables:

```text
api_keys
chat_sessions
chat_messages
chat_jobs
chat_job_checkpoints
chat_job_events
audit_logs, later
execution_logs, later
token_usage_logs, later
```

MVP can start with:

```text
api_keys
```

`api_keys` table fields:

```text
id
name
owner
key_prefix
key_hash
allowed_office_ids
allowed_capabilities
can_view_pii
expires_at
revoked_at
created_at
last_used_at
```

Important rule:

```text
never store raw API keys
```

Chat data model reference:

```text
docs/chat-data-model.md
```

Validation:

```bash
sqlx migrate run
```

Expected result:

```text
migrations run successfully
api_keys table exists
```

Current status:

```text
DONE

api_keys migration exists.
chat session/job migration exists.
knowledge catalog/index migration exists.
Local/dev startup migration is controlled by APP_DATABASE_MIGRATE_ON_STARTUP.
```
