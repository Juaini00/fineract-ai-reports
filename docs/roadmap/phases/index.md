# Implementation Steps

Source: `docs-old/implementation-steps.md`

This is the split, readable version of the original document. Content was migrated section-by-section so no old context is dropped.

## Original introduction


This document defines the step-by-step implementation order for the AI Reporting Service.

The goal is to build the system incrementally, with each step producing a testable milestone. Do not jump directly into AI planning or report execution before the application foundation, authentication, and observability are ready.

## Sections

- [Phase 0: Project Baseline](./01-phase-0-project-baseline.md)
- [Phase 1: Application Bootstrap](./02-phase-1-application-bootstrap.md)
- [Phase 2: Database Connections](./03-phase-2-database-connections.md)
- [Phase 3: Health And Readiness Endpoints](./04-phase-3-health-and-readiness-endpoints.md)
- [Phase 4: App Database Migrations](./05-phase-4-app-database-migrations.md)
- [Phase 5: API Key Generation](./06-phase-5-api-key-generation.md)
- [Phase 6: API Key Authentication Middleware](./07-phase-6-api-key-authentication-middleware.md)
- [Phase 7: Authorization Guards](./08-phase-7-authorization-guards.md)
- [Phase 8: Chat Session And Job Data Model](./09-phase-8-chat-session-and-job-data-model.md)
- [Phase 9: Chat Job API Foundation](./10-phase-9-chat-job-api-foundation.md)
- [Phase 10: Catalog Foundation](./11-phase-10-catalog-foundation.md)
- [Phase 11: Query Validation](./12-phase-11-query-validation.md)
- [Phase 12: Local Classifier MVP](./13-phase-12-local-classifier-mvp.md)
- [Phase 13: Execution Plan And Policy Guard](./14-phase-13-execution-plan-and-policy-guard.md)
- [Phase 14: Query Executor MVP](./15-phase-14-query-executor-mvp.md)
- [Phase 15: Event-Driven Audit Trail](./16-phase-15-event-driven-audit-trail.md)
- [Phase 16: Response Formatting](./17-phase-16-response-formatting.md)
- [Phase 17: LLM Provider Integration](./18-phase-17-llm-provider-integration.md)
- [Phase 18: Vector Indexing](./19-phase-18-vector-indexing.md)
- [Phase 19: Reporting Expansion](./20-phase-19-reporting-expansion.md)
- [Phase 20: LQR Retrieval Overlay](./21-phase-20-lqr-retrieval-overlay.md)
- [Phase 21: Client and Organization Full Support](./22-phase-21-client-and-organization-full-support.md)
- [Recommended Implementation Order](./23-recommended-implementation-order.md)
