# Chat Data Model: 12. Retention Policy

Source: `docs-old/chat-data-model.md`

## 12. Retention Policy

Suggested initial retention:

```text
active sessions: until expired or archived
chat messages: 30-90 days depending on policy
completed jobs: 7-30 days
failed jobs: 7-30 days
checkpoints: same as job retention
events: same as job retention
audit events: same as job retention initially; management retention can be longer later
redis live state: 15-60 minutes
```

Retention should be configurable later.
