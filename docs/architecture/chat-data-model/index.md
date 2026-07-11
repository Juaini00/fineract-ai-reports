# Chat Data Model

Source: `docs-old/chat-data-model.md`

This is the split, readable version of the original document. Content was migrated section-by-section so no old context is dropped.

## Original introduction


This document defines the data model for chat sessions, chat messages, chat jobs, checkpoints, audit events, and live progress state. Per-job clarification memory is defined in `docs/job-memory.md`. The detailed audit design is defined in `docs/audit-trail-design.md`.

The system must support long-running report generation, clarification flows, SSE progress updates, and resumable jobs without relying on in-memory state as the source of truth.

## Sections

- [1. Storage Principles](./01-1-storage-principles.md)
- [2. Main Entities](./02-2-main-entities.md)
- [3. chat_sessions](./03-3-chatsessions.md)
- [4. chat_messages](./04-4-chatmessages.md)
- [5. chat_jobs](./05-5-chatjobs.md)
- [6. chat_job_checkpoints](./06-6-chatjobcheckpoints.md)
- [7. chat_job_events](./07-7-chatjobevents.md)
- [7.1. chat_job_audit_events](./08-7-1-chatjobauditevents.md)
- [8. Redis Live State](./09-8-redis-live-state.md)
- [9. Clarification Flow State](./10-9-clarification-flow-state.md)
- [10. Endpoint Relationship](./11-10-endpoint-relationship.md)
- [11. Indexes](./12-11-indexes.md)
- [12. Retention Policy](./13-12-retention-policy.md)
- [13. Important Rules](./14-13-important-rules.md)
