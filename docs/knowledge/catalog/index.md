# Knowledge Catalog

Source: `docs-old/knowledge-catalog.md`

This is the split, readable version of the original document. Content was migrated section-by-section so no old context is dropped.

## Original introduction


This document defines the knowledge system for the AI Reporting Service.

Knowledge is controlled application context. It helps the service understand user requests, map them to approved reporting capabilities, execute approved SQL, and format safe responses. It must also keep the application aligned with the approved reporting data scope.

Knowledge is not a free-form prompt dump. It is structured application data that Rust loads, validates, and enforces.

## Sections

- [1. Source Of Truth](./01-1-source-of-truth.md)
- [2. Knowledge Layers](./02-2-knowledge-layers.md)
- [3. Currently Loaded Knowledge](./03-3-currently-loaded-knowledge.md)
- [4. Recommended Directory Structure](./04-4-recommended-directory-structure.md)
- [5. Knowledge Pipeline](./05-5-knowledge-pipeline.md)
- [6. Machine-Readable File Contracts](./06-6-machine-readable-file-contracts.md)
- [7. Validation Rules](./07-7-validation-rules.md)
- [8. Runtime Decision Rules](./08-8-runtime-decision-rules.md)
- [9. Storage And Refresh Policy](./09-9-storage-and-refresh-policy.md)
- [10. Audit Requirements](./10-10-audit-requirements.md)
- [11. Implementation Order](./11-11-implementation-order.md)
- [12. Non-Goals For The Catalog](./12-12-non-goals-for-the-catalog.md)
- [13. References](./13-13-references.md)
- [14. Adding A Capability](./14-14-adding-a-capability.md)
