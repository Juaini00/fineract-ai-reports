# Chat Data Model: 12. Retention Policy

Source: `docs-old/chat-data-model.md`

## 12. Retention Policy

Current deletion semantics and suggested initial retention:

```text
active sessions: client-visible until expired or soft-archived
archived sessions: retained in PostgreSQL; no restore or purge endpoint yet
chat messages: 30-90 days depending on policy
completed jobs: 7-30 days
failed jobs: 7-30 days
checkpoints: same as job retention
events: same as job retention
audit events: same as job retention initially; management retention can be longer later
redis live state: 15-60 minutes
```

Deleting a session archives it immediately. It does not physically delete its
messages, jobs, checkpoints, events, or audit records; clean Redis state; or
force-cancel a job already running. Archived session resources return sanitized
`404` responses to clients, while an in-flight synchronous job may finish its
internal persistence safely.

Automated purge and configurable retention remain future work.
