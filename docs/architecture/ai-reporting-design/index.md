# AI Reporting Service Design

Source: `docs-old/ai-reporting-design.md`

This is the split, readable version of the original document. Content was migrated section-by-section so no old context is dropped.

## Original introduction


This document defines the design for a Rust-based AI Reporting Service that reads data from an existing Apache Fineract database through a read-only user or a read replica.

The AI chatbot exists to help microfinance admins across the whole in-scope reporting surface (savings, client, organization, group/center foundations, plus loan/accounting/tax/audit/custom-datatables once activated). Every query shape enumerated in `docs/capability-coverage-matrix.md` is expected to be supported — currently as a mix of `implemented` and `planned` capabilities, with deferred domains activating over time. Capability shortage today is a known-gap, not the design intent.

The service does not modify Apache Fineract, does not add Fineract plugins, and does not change the Fineract database schema.

## Sections

- [1. Product Goal](./01-1-product-goal.md)
- [2. Core Principles](./02-2-core-principles.md)
- [3. High-Level Runtime Flow](./03-3-high-level-runtime-flow.md)
- [4. Authentication And Authorization](./04-4-authentication-and-authorization.md)
- [5. Knowledge Model](./05-5-knowledge-model.md)
- [6. Output Modes](./06-6-output-modes.md)
- [7. Execution Types](./07-7-execution-types.md)
- [8. Decision Policy](./08-8-decision-policy.md)
- [9. Query Cost Estimation](./09-9-query-cost-estimation.md)
- [10. Query Validation](./10-10-query-validation.md)
- [11. Storage Architecture](./11-11-storage-architecture.md)
- [12. Maintainable Backend Structure](./12-12-maintainable-backend-structure.md)
- [13. Client / Admin UI Design](./13-13-client-admin-ui-design.md)
- [14. API Surface Draft](./14-14-api-surface-draft.md)
- [15. Current Runtime Enable-State](./15-15-current-runtime-enable-state.md)
- [16. Implementation Prompt](./16-16-implementation-prompt.md)
- [18. Planned Architecture Changes](./17-18-planned-architecture-changes.md)
- [19. Next Steps](./18-19-next-steps.md)
