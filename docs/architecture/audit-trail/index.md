# Audit Trail Design

Source: `docs-old/audit-trail-design.md`

This is the split, readable version of the original document. Content was migrated section-by-section so no old context is dropped.

## Original introduction


This document defines the durable audit trail for chat/report requests. The audit trail is separate from runtime logs and live SSE events. Its job is to make each request explainable after it completes, especially when analyzing whether the system followed `docs/Modern_RAG_Architecture_Blueprint.md`.

## Sections

- [Goals](./01-goals.md)
- [Non-Goals](./02-non-goals.md)
- [Storage Model](./03-storage-model.md)
- [Event-Driven Writer](./04-event-driven-writer.md)
- [Durability Trade-Off](./05-durability-trade-off.md)
- [Audit Stages](./06-audit-stages.md)
- [Blueprint Step Mapping](./07-blueprint-step-mapping.md)
- [Status Values](./08-status-values.md)
- [Flags](./09-flags.md)
- [Safe Payload Rules](./10-safe-payload-rules.md)
- [Relationship To Existing Tables](./11-relationship-to-existing-tables.md)
- [Example Timeline](./12-example-timeline.md)
- [Management Queries](./13-management-queries.md)
- [API Access](./14-api-access.md)
- [First Implementation Scope](./15-first-implementation-scope.md)
