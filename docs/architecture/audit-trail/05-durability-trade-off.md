# Audit Trail Design: Durability Trade-Off

Source: `docs-old/audit-trail-design.md`

## Durability Trade-Off

The first implementation is near-real-time and non-blocking, but not zero-loss. If the process crashes before the worker flushes, recent audit events in memory can be lost.

Upgrade paths:

1. Redis Stream if audit must survive app process crashes without putting DB writes in the request path.
2. DB outbox if audit must be transactionally attached to job state changes.
3. Kafka/NATS/RabbitMQ only if the system grows beyond this service boundary.

For now, do not add a new third-party dependency. Tokio, SQLx, PostgreSQL, and existing tracing are enough.
