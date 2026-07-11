# Chat Data Model: 13. Important Rules

Source: `docs-old/chat-data-model.md`

## 13. Important Rules

1. PostgreSQL stores durable state only.
2. Redis stores live progress only.
3. Memory is never the source of truth.
4. Save PostgreSQL checkpoints only at important boundaries.
5. SSE events are not the source of truth.
6. A job must be resumable from PostgreSQL after app restart.
7. Clarification must preserve job state and continue the same job.
8. Do not create a new job when answering clarification.
9. Do not hold DB connections during SSE idle time.
10. Do not store raw API keys, raw SQL, or internal prompts in chat-visible payloads.
11. Audit events must be persisted through a non-blocking background writer, not inline with the main job pipeline.
